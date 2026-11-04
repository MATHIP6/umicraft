// SPDX-License-Identifier: MIT OR Apache-2.0

//! Where particles come from: the `Particle` packet and the spawn filter of
//! `RenderGlobal.spawnEntityFX`.

use glam::DVec3;
use umicraft_protocol::packets::play::clientbound::Particle as ParticlePacket;

use super::behaviours::{create, digging};
use super::engine::ParticleEngine;
use super::kind::ParticleKind;
use super::particle::Particle;
use umicraft_core::java_random::JavaRandom;
use umicraft_world::BlockState;

/// How far from the viewer a particle is still created, squared (MCP
/// `RenderGlobal.java:2085`: `d0 * d0 + d1 * d1 + d2 * d2 > 256.0D`).
const RANGE_SQUARED: f64 = 256.0;

/// `GameSettings.particleSetting`: how many particles are created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParticleSetting {
    /// Every particle (`0`).
    #[default]
    All,
    /// Two particles out of three (`1`).
    Decreased,
    /// Only the ones that ignore the range, such as explosions (`2`).
    Minimal,
}

/// What to create: a particle type with its extra values, where and how fast.
#[derive(Debug, Clone, Copy)]
pub struct SpawnRequest<'a> {
    /// The particle type.
    pub kind: ParticleKind,
    /// The type's extra values (the block state of `blockcrack`).
    pub arguments: &'a [i32],
    /// Skips the range and setting filters (explosions, long-distance packets).
    pub ignore_range: bool,
    /// Where it appears.
    pub position: DVec3,
    /// Its velocity (a colour for some kinds, see the families).
    pub velocity: DVec3,
}

/// Who is looking and with which setting: the inputs of the spawn filter.
#[derive(Debug, Clone, Copy)]
pub struct SpawnFilter {
    /// The position of the viewer.
    pub viewer: DVec3,
    /// `GameSettings.particleSetting`.
    pub setting: ParticleSetting,
}

impl ParticleSetting {
    /// The setting of the saved value `0`, `1` or `2` (`GameSettings.particleSetting`);
    /// anything else is the default, as a corrupt options file must not break
    /// the game.
    #[must_use]
    pub fn from_index(index: u8) -> Self {
        match index {
            1 => Self::Decreased,
            2 => Self::Minimal,
            _ => Self::All,
        }
    }

    /// The language key of the value label of `GameSettings.getTranslation(
    /// PARTICLES, setting)` (`options.particles.all` ...).
    #[must_use]
    pub fn lang_key(self) -> &'static str {
        match self {
            Self::All => "options.particles.all",
            Self::Decreased => "options.particles.decreased",
            Self::Minimal => "options.particles.minimal",
        }
    }
}

impl ParticleEngine {
    /// Creates the particle of `kind` unless the viewer is too far or the
    /// setting filters it out (MCP `RenderGlobal.spawnEntityFX`, line 2063).
    ///
    /// `ignore_range` skips both filters, as for the explosions and for a
    /// packet with the long-distance flag. `arguments` are the type's extra
    /// values (the block state of `blockcrack` and `blockdust`). Returns
    /// whether a particle was created: kinds the engine cannot build yet (item
    /// cracks, and the families still to port) return `false`.
    pub fn spawn(&mut self, request: &SpawnRequest<'_>, filter: &SpawnFilter) -> bool {
        self.spawn_with(request, filter, |particle, _| particle)
    }

    /// [`spawn`](Self::spawn) for the callers that adjust the new particle
    /// (colour, velocity) before it is added, as the ones of `playAuxSFX` do
    /// with the `EntityFX` that `spawnEntityFX` returns. `adjust` also gets
    /// the engine's random source to keep the draw order of the reference.
    pub(super) fn spawn_with(
        &mut self,
        request: &SpawnRequest<'_>,
        filter: &SpawnFilter,
        adjust: impl FnOnce(Particle, &mut JavaRandom) -> Particle,
    ) -> bool {
        if !request.ignore_range {
            let setting = match filter.setting {
                ParticleSetting::Decreased if self.rng().next_int(3) == 0 => {
                    ParticleSetting::Minimal
                }
                other => other,
            };
            if filter.viewer.distance_squared(request.position) > RANGE_SQUARED
                || setting == ParticleSetting::Minimal
            {
                return false;
            }
        }
        match self.build(request) {
            Some(particle) => {
                let particle = adjust(particle, self.rng());
                self.add(particle);
                true
            }
            None => false,
        }
    }

    /// The particle for `kind`, or `None` when it has no factory yet.
    ///
    /// The block kinds read the block state id (`id | metadata << 12`) from
    /// `arguments`; without a position the reference tints them with
    /// `getRenderColor`, which is white here (see `docs/tasks/p8/README.md`).
    fn build(&mut self, request: &SpawnRequest<'_>) -> Option<Particle> {
        let SpawnRequest {
            kind,
            arguments,
            position,
            velocity,
            ..
        } = *request;
        match kind {
            ParticleKind::BlockCrack | ParticleKind::BlockDust => {
                let state = block_state(*arguments.first()?)?;
                let make = if kind == ParticleKind::BlockDust {
                    digging::dust
                } else {
                    digging::fragment
                };
                Some(make(state, [1.0; 3], position, velocity, self.rng()))
            }
            _ => create(kind, position, velocity, self.rng()),
        }
    }

    /// Spawns what a `Particle` packet asks for (MCP
    /// `NetHandlerPlayClient.handleParticles`, line 2003): a count of 0 is one
    /// particle whose velocity is `speed * offset`; otherwise `count`
    /// particles scattered by a Gaussian of the offsets, each with a Gaussian
    /// velocity of `speed`. Returns how many were created.
    pub fn spawn_from_packet(&mut self, packet: &ParticlePacket, filter: &SpawnFilter) -> usize {
        let Some(kind) = ParticleKind::from_id(packet.particle_id) else {
            return 0;
        };
        let origin = DVec3::new(packet.x.into(), packet.y.into(), packet.z.into());
        let offset = DVec3::new(
            packet.offset_x.into(),
            packet.offset_y.into(),
            packet.offset_z.into(),
        );
        let speed = f64::from(packet.speed);
        let request = |position, velocity| SpawnRequest {
            kind,
            arguments: &packet.arguments,
            ignore_range: packet.long_distance,
            position,
            velocity,
        };
        if packet.count == 0 {
            return usize::from(self.spawn(&request(origin, offset * speed), filter));
        }
        let mut created = 0;
        for _ in 0..packet.count {
            let scatter = self.gaussian_triple() * offset;
            let velocity = self.gaussian_triple() * speed;
            created += usize::from(self.spawn(&request(origin + scatter, velocity), filter));
        }
        created
    }

    /// Three Gaussian draws in the order the reference makes them.
    fn gaussian_triple(&mut self) -> DVec3 {
        let rng = self.rng();
        DVec3::new(
            rng.next_gaussian(),
            rng.next_gaussian(),
            rng.next_gaussian(),
        )
    }
}

/// The block state of a `blockcrack` argument: the id in the low 12 bits, the
/// metadata above (`Block.getStateById`). Air has no fragments.
fn block_state(argument: i32) -> Option<BlockState> {
    let id = u16::try_from(argument & 0xFFF).ok()?;
    let metadata = u8::try_from((argument >> 12) & 0xF).ok()?;
    (id != 0).then(|| BlockState::new(id, metadata))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::Layer;

    fn packet(count: i32) -> ParticlePacket {
        ParticlePacket {
            particle_id: ParticleKind::Flame.id(),
            long_distance: false,
            x: 10.0,
            y: 64.0,
            z: 10.0,
            offset_x: 0.5,
            offset_y: 0.5,
            offset_z: 0.5,
            speed: 0.1,
            count,
            arguments: Vec::new(),
        }
    }

    fn filter(viewer: DVec3, setting: ParticleSetting) -> SpawnFilter {
        SpawnFilter { viewer, setting }
    }

    fn live(engine: &ParticleEngine) -> usize {
        engine.len()
    }

    #[test]
    fn the_saved_index_selects_the_setting_and_defaults_when_corrupt() {
        assert_eq!(ParticleSetting::from_index(1), ParticleSetting::Decreased);
        assert_eq!(
            ParticleSetting::from_index(2).lang_key(),
            "options.particles.minimal"
        );
        assert_eq!(ParticleSetting::from_index(200), ParticleSetting::All);
    }

    #[test]
    fn a_count_of_zero_makes_one_particle() {
        let mut engine = ParticleEngine::new(1);
        let created = engine.spawn_from_packet(
            &packet(0),
            &filter(DVec3::new(10.0, 64.0, 10.0), ParticleSetting::All),
        );
        assert_eq!((created, live(&engine)), (1, 1));
        assert_eq!(engine.particles(Layer::Misc, false).len(), 1);
    }

    #[test]
    fn a_count_makes_that_many_particles() {
        let mut engine = ParticleEngine::new(1);
        let created = engine.spawn_from_packet(
            &packet(7),
            &filter(DVec3::new(10.0, 64.0, 10.0), ParticleSetting::All),
        );
        assert_eq!(created, 7);
    }

    #[test]
    fn far_particles_are_not_created_unless_long_distance() {
        let mut engine = ParticleEngine::new(1);
        let far = DVec3::new(100.0, 64.0, 10.0);
        assert_eq!(
            engine.spawn_from_packet(&packet(3), &filter(far, ParticleSetting::All)),
            0
        );
        let mut long = packet(3);
        long.long_distance = true;
        assert_eq!(
            engine.spawn_from_packet(&long, &filter(far, ParticleSetting::All)),
            3
        );
    }

    #[test]
    fn minimal_creates_only_the_range_ignoring_particles() {
        let mut engine = ParticleEngine::new(1);
        let near = DVec3::new(10.0, 64.0, 10.0);
        assert_eq!(
            engine.spawn_from_packet(&packet(5), &filter(near, ParticleSetting::Minimal)),
            0
        );
        let mut long = packet(5);
        long.long_distance = true;
        assert_eq!(
            engine.spawn_from_packet(&long, &filter(near, ParticleSetting::Minimal)),
            5
        );
    }

    #[test]
    fn decreased_keeps_about_two_thirds() {
        let mut engine = ParticleEngine::new(3);
        let near = DVec3::new(10.0, 64.0, 10.0);
        let created =
            engine.spawn_from_packet(&packet(3000), &filter(near, ParticleSetting::Decreased));
        assert!((1800..2200).contains(&created), "{created}");
    }

    #[test]
    fn a_block_crack_packet_makes_a_fragment_of_its_block() {
        let mut engine = ParticleEngine::new(1);
        let mut crack = packet(1);
        crack.particle_id = ParticleKind::BlockCrack.id();
        // Stone (1) with metadata 3.
        crack.arguments = vec![1 | (3 << 12)];
        let near = DVec3::new(10.0, 64.0, 10.0);
        assert_eq!(
            engine.spawn_from_packet(&crack, &filter(near, ParticleSetting::All)),
            1
        );
        let fragment = &engine.particles(Layer::Terrain, false)[0];
        assert_eq!(
            fragment.texture,
            crate::particles::Texture::Block {
                state: BlockState::new(1, 3)
            }
        );
    }

    #[test]
    fn an_air_crack_or_missing_argument_makes_nothing() {
        let mut engine = ParticleEngine::new(1);
        let near = DVec3::new(10.0, 64.0, 10.0);
        let mut crack = packet(1);
        crack.particle_id = ParticleKind::BlockDust.id();
        crack.arguments = vec![0];
        assert_eq!(
            engine.spawn_from_packet(&crack, &filter(near, ParticleSetting::All)),
            0
        );
        crack.arguments = Vec::new();
        assert_eq!(
            engine.spawn_from_packet(&crack, &filter(near, ParticleSetting::All)),
            0
        );
    }

    #[test]
    fn item_cracks_have_no_factory_yet() {
        let mut engine = ParticleEngine::new(1);
        let mut crack = packet(1);
        crack.particle_id = ParticleKind::ItemCrack.id();
        crack.arguments = vec![1, 0];
        let near = DVec3::new(10.0, 64.0, 10.0);
        assert_eq!(
            engine.spawn_from_packet(&crack, &filter(near, ParticleSetting::All)),
            0
        );
    }
}

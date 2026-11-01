// SPDX-License-Identifier: MIT OR Apache-2.0

//! Where particles come from: the `Particle` packet and the spawn filter of
//! `RenderGlobal.spawnEntityFX`.

use glam::DVec3;
use umicraft_protocol::packets::play::clientbound::Particle as ParticlePacket;

use super::behaviours::create;
use super::engine::ParticleEngine;
use super::kind::ParticleKind;

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

impl ParticleEngine {
    /// Creates the particle of `kind` unless the viewer is too far or the
    /// setting filters it out (MCP `RenderGlobal.spawnEntityFX`, line 2063).
    ///
    /// `ignore_range` skips both filters, as for the explosions and for a
    /// packet with the long-distance flag. Returns whether a particle was
    /// created: kinds the engine cannot build yet (block and item cracks, and
    /// the families still to port) return `false`.
    pub fn spawn(
        &mut self,
        kind: ParticleKind,
        ignore_range: bool,
        position: DVec3,
        velocity: DVec3,
        viewer: DVec3,
        setting: ParticleSetting,
    ) -> bool {
        if !ignore_range {
            let setting = match setting {
                ParticleSetting::Decreased if self.rng().next_int(3) == 0 => {
                    ParticleSetting::Minimal
                }
                other => other,
            };
            if viewer.distance_squared(position) > RANGE_SQUARED
                || setting == ParticleSetting::Minimal
            {
                return false;
            }
        }
        match create(kind, position, velocity, self.rng()) {
            Some(particle) => {
                self.add(particle);
                true
            }
            None => false,
        }
    }

    /// Spawns what a `Particle` packet asks for (MCP
    /// `NetHandlerPlayClient.handleParticles`, line 2003): a count of 0 is one
    /// particle whose velocity is `speed * offset`; otherwise `count`
    /// particles scattered by a Gaussian of the offsets, each with a Gaussian
    /// velocity of `speed`. Returns how many were created.
    pub fn spawn_from_packet(
        &mut self,
        packet: &ParticlePacket,
        viewer: DVec3,
        setting: ParticleSetting,
    ) -> usize {
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
        if packet.count == 0 {
            return usize::from(self.spawn(
                kind,
                packet.long_distance,
                origin,
                offset * speed,
                viewer,
                setting,
            ));
        }
        let mut created = 0;
        for _ in 0..packet.count {
            let scatter = self.gaussian_triple() * offset;
            let velocity = self.gaussian_triple() * speed;
            created += usize::from(self.spawn(
                kind,
                packet.long_distance,
                origin + scatter,
                velocity,
                viewer,
                setting,
            ));
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

    fn live(engine: &ParticleEngine) -> usize {
        engine.len()
    }

    #[test]
    fn a_count_of_zero_makes_one_particle() {
        let mut engine = ParticleEngine::new(1);
        let created = engine.spawn_from_packet(
            &packet(0),
            DVec3::new(10.0, 64.0, 10.0),
            ParticleSetting::All,
        );
        assert_eq!((created, live(&engine)), (1, 1));
        assert_eq!(engine.particles(Layer::Misc, false).len(), 1);
    }

    #[test]
    fn a_count_makes_that_many_particles() {
        let mut engine = ParticleEngine::new(1);
        let created = engine.spawn_from_packet(
            &packet(7),
            DVec3::new(10.0, 64.0, 10.0),
            ParticleSetting::All,
        );
        assert_eq!(created, 7);
    }

    #[test]
    fn far_particles_are_not_created_unless_long_distance() {
        let mut engine = ParticleEngine::new(1);
        let far = DVec3::new(100.0, 64.0, 10.0);
        assert_eq!(
            engine.spawn_from_packet(&packet(3), far, ParticleSetting::All),
            0
        );
        let mut long = packet(3);
        long.long_distance = true;
        assert_eq!(
            engine.spawn_from_packet(&long, far, ParticleSetting::All),
            3
        );
    }

    #[test]
    fn minimal_creates_only_the_range_ignoring_particles() {
        let mut engine = ParticleEngine::new(1);
        let near = DVec3::new(10.0, 64.0, 10.0);
        assert_eq!(
            engine.spawn_from_packet(&packet(5), near, ParticleSetting::Minimal),
            0
        );
        let mut long = packet(5);
        long.long_distance = true;
        assert_eq!(
            engine.spawn_from_packet(&long, near, ParticleSetting::Minimal),
            5
        );
    }

    #[test]
    fn decreased_keeps_about_two_thirds() {
        let mut engine = ParticleEngine::new(3);
        let near = DVec3::new(10.0, 64.0, 10.0);
        let created = engine.spawn_from_packet(&packet(3000), near, ParticleSetting::Decreased);
        assert!((1800..2200).contains(&created), "{created}");
    }

    #[test]
    fn kinds_without_a_factory_create_nothing() {
        let mut engine = ParticleEngine::new(1);
        let mut crack = packet(1);
        crack.particle_id = ParticleKind::BlockCrack.id();
        crack.arguments = vec![1];
        let near = DVec3::new(10.0, 64.0, 10.0);
        assert_eq!(
            engine.spawn_from_packet(&crack, near, ParticleSetting::All),
            0
        );
    }
}

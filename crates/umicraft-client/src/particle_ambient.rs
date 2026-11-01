// SPDX-License-Identifier: MIT OR Apache-2.0

//! The particles the world makes around the player each tick (P8): the
//! sampled `randomDisplayTick` of the blocks, the rain splashes and the dust
//! under a sprinting player.
//!
//! The pure rules live in [`umicraft_game::particles::ambient`]; this module
//! only supplies the world, the biome climate and the player, and feeds the
//! requests through the spawn filter.

use glam::DVec3;
use umicraft_game::particles::ambient::{
    rain_splashes, running_dust, sample_around, RainView, Requests, RunningBody, Weather,
};
use umicraft_game::particles::{ParticleEngine, ParticleSetting, SpawnFilter};
use umicraft_mesh::climate::biome_climate;
use umicraft_world::{BlockPos, World};

/// Width of the player's box (`EntityPlayer.setSize(0.6F, 1.8F)`).
const PLAYER_WIDTH: f64 = 0.6;
/// `World.isRaining`: the rain level above which it counts as raining.
const RAINING_LEVEL: f32 = 0.2;
/// The precipitation threshold of `getFloatTemperature`: colder biomes snow.
const RAIN_TEMPERATURE: f32 = 0.15;

/// What the world looks like to the ambient rules.
struct ClimateWeather<'a> {
    world: &'a World,
    rain_strength: f32,
}

impl ClimateWeather<'_> {
    /// Whether the biome at `pos` rains rather than snows or stays dry.
    fn rains_at(&self, pos: BlockPos) -> bool {
        self.world
            .biome(pos)
            .map(biome_climate)
            .is_some_and(|climate| climate.precipitates && climate.temperature >= RAIN_TEMPERATURE)
    }
}

impl Weather for ClimateWeather<'_> {
    /// `World.canLightningStrike`: raining, open to the sky and in a biome
    /// that rains.
    fn can_lightning_strike(&self, pos: BlockPos) -> bool {
        self.rain_strength > RAINING_LEVEL
            && self
                .world
                .precipitation_height(pos.x, pos.z)
                .is_some_and(|height| height <= pos.y)
            && self.rains_at(pos)
    }

    fn rains_on_biome(&self, pos: BlockPos) -> bool {
        self.rains_at(pos)
    }
}

/// The local player, as far as the ambient rules care.
#[derive(Debug, Clone, Copy)]
pub struct Viewer {
    /// The feet.
    pub position: DVec3,
    /// The motion per tick.
    pub motion: DVec3,
    /// Sprinting on the ground, outside water (`Entity.createRunningParticles`).
    pub running: bool,
    /// Creative mode with a barrier in hand (`showBarrierParticles`).
    pub sees_barriers: bool,
}

/// Queues and spawns the ambient particles of a tick; the buffer is reused.
#[derive(Debug, Default)]
pub struct AmbientSpawner {
    requests: Requests,
}

impl AmbientSpawner {
    /// Runs the ambient sources for one tick.
    ///
    /// `update_count` is the client tick counter that seeds the rain splashes.
    pub fn tick(
        &mut self,
        engine: &mut ParticleEngine,
        world: &World,
        viewer: &Viewer,
        rain_strength: f32,
        update_count: i32,
        setting: ParticleSetting,
    ) {
        let weather = ClimateWeather {
            world,
            rain_strength,
        };
        let feet = block_of(viewer.position);
        let filter = SpawnFilter {
            viewer: viewer.position,
            setting,
        };
        self.requests.clear();
        sample_around(
            world,
            &weather,
            feet,
            viewer.sees_barriers,
            engine.rng(),
            &mut self.requests,
        );
        if rain_strength > 0.0 {
            let view = RainView {
                strength: rain_strength,
                // The video options live on the render thread; fancy is the
                // default and only widens the splash radius.
                fancy_graphics: true,
                setting,
                update_count,
            };
            rain_splashes(world, &weather, feet, &view, &mut self.requests);
        }
        for request in self.requests.drain(..) {
            engine.spawn(&request, &filter);
        }
        if viewer.running {
            spawn_dust(engine, world, viewer, &filter);
        }
    }
}

/// The block under a position.
fn block_of(position: DVec3) -> BlockPos {
    BlockPos::new(
        position.x.floor() as i32,
        position.y.floor() as i32,
        position.z.floor() as i32,
    )
}

/// The sprint dust under the feet (`Entity.createRunningParticles`).
fn spawn_dust(engine: &mut ParticleEngine, world: &World, viewer: &Viewer, filter: &SpawnFilter) {
    let body = RunningBody {
        position: viewer.position,
        width: PLAYER_WIDTH,
        motion: viewer.motion,
    };
    let mut argument = [0];
    if let Some(request) = running_dust(world, &body, &mut argument, engine.rng()) {
        engine.spawn(&request, filter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_happens_in_an_empty_world_without_rain_or_sprint() {
        let mut engine = ParticleEngine::new(1);
        let world = World::new(0);
        let viewer = Viewer {
            position: DVec3::new(0.5, 64.0, 0.5),
            motion: DVec3::ZERO,
            running: false,
            sees_barriers: false,
        };
        AmbientSpawner::default().tick(&mut engine, &world, &viewer, 0.0, 0, ParticleSetting::All);
        assert!(engine.is_empty());
    }

    #[test]
    fn a_player_on_a_torch_world_makes_particles_over_many_ticks() {
        let mut engine = ParticleEngine::new(1);
        let mut world = World::new(0);
        let payload = vec![0u8; 8192 + 2048 + 2048 + 256];
        world
            .apply_chunk(
                umicraft_world::ChunkPos::new(0, 0),
                umicraft_world::ChunkPayload {
                    primary_bit_mask: 1,
                    ground_up: true,
                    data: &payload,
                },
            )
            .expect("an empty column loads");
        // A wall torch (id 50) next to the viewer.
        world.set_block(
            BlockPos::new(2, 5, 2),
            umicraft_world::BlockState::new(50, 5),
        );
        let viewer = Viewer {
            position: DVec3::new(2.5, 5.0, 3.5),
            motion: DVec3::ZERO,
            running: false,
            sees_barriers: false,
        };
        let mut spawner = AmbientSpawner::default();
        for tick in 0..200 {
            spawner.tick(
                &mut engine,
                &world,
                &viewer,
                0.0,
                tick,
                ParticleSetting::All,
            );
        }
        assert!(!engine.is_empty(), "a torch smokes now and then");
    }
}

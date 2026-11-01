// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ambient particles: the ones the world makes by itself around the player,
//! without any packet (MCP `Block.randomDisplayTick`, `WorldClient.
//! doVoidFogParticles`, `EntityRenderer.addRainParticles` and
//! `Entity.createRunningParticles`).
//!
//! | Module | Owns |
//! |---|---|
//! | `flame` | torch, lit furnace |
//! | `fire` | fire |
//! | `redstone` | lit redstone torch, wire, lit ore, powered repeater |
//! | `liquid` | water plankton, lava pops, drips under liquids |
//! | `portal` | Nether portal, end portal, ender chest |
//! | `enchanting` | the glyphs of the enchanting table |
//! | `misc` | mycelium, brewing stand, leaves, wet sponge |
//! | `sample` | [`sample_around`], the 1000 random blocks per tick |
//! | `weather` | [`Weather`], [`rain_splashes`] |
//! | `running` | [`running_dust`] |
//!
//! Every function only queues [`SpawnRequest`]s; the caller feeds them to
//! [`ParticleEngine::spawn`](super::ParticleEngine::spawn) with its viewer and
//! `particleSetting`, so the range and setting filters stay in one place.
//! The randomness is passed in, so a seeded generator replays a tick.
//!
//! **Sounds are not played** (fire crackle, water, lava, portal ambience, rain),
//! and the random draws that only choose them are not made, so a sequence
//! differs from the reference after such a draw. Stairs delegate to a model
//! block that never has a display tick in 1.8 and make nothing. The biome and
//! the weather come through [`Weather`], since the block accessor has neither.

mod emit;
mod enchanting;
mod facing;
mod fire;
mod flame;
mod liquid;
mod misc;
mod portal;
mod redstone;
mod running;
mod sample;
mod surface;
#[cfg(test)]
mod testing;
mod weather;

use umicraft_core::java_random::JavaRandom;
use umicraft_world::fluid::Liquid;
use umicraft_world::BlockPos;

pub use emit::Requests;
pub use running::{running_dust, RunningBody};
pub use sample::{sample_around, SAMPLES};
pub use weather::{rain_splashes, Dry, RainView, Weather};

use crate::shapes::BlockSource;

/// `Block.randomDisplayTick` of the block at `pos`: queues what it spawns.
///
/// Blocks without an effect queue nothing. The ids are those of
/// `Block.registerBlocks`.
///
/// ```
/// use umicraft_core::java_random::JavaRandom;
/// use umicraft_game::particles::ambient::{display_tick, Dry};
/// use umicraft_game::shapes::BlockSource;
/// use umicraft_world::{BlockPos, BlockState};
///
/// /// A world that is one torch and air.
/// struct Torch;
///
/// impl BlockSource for Torch {
///     fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
///         if (x, y, z) == (0, 64, 0) { BlockState::new(50, 5) } else { BlockState::AIR }
///     }
/// }
///
/// let mut out = Vec::new();
/// display_tick(&Torch, &Dry, BlockPos::new(0, 64, 0), &mut JavaRandom::new(1), &mut out);
/// // A torch makes smoke and a flame.
/// assert_eq!(out.len(), 2);
/// ```
pub fn display_tick<S, W>(
    world: &S,
    weather: &W,
    pos: BlockPos,
    rng: &mut JavaRandom,
    out: &mut Requests,
) where
    S: BlockSource + ?Sized,
    W: Weather + ?Sized,
{
    let state = world.block(pos.x, pos.y, pos.z);
    if let Some(kind) = Liquid::of(state) {
        return liquid::tick(world, pos, state, kind, rng, out);
    }
    match state.id() {
        18 | 161 => misc::leaves(world, weather, pos, rng, out),
        19 if state.metadata() == 1 => misc::wet_sponge(world, pos, rng, out),
        50 => flame::torch(pos, state, out),
        51 => fire::fire(world, pos, rng, out),
        55 => redstone::wire(pos, state, rng, out),
        62 => flame::burning_furnace(pos, state, rng, out),
        74 => redstone::lit_ore(world, pos, rng, out),
        76 => redstone::lit_torch(pos, state, rng, out),
        90 => portal::nether_portal(world, pos, rng, out),
        94 => redstone::powered_repeater(pos, state, rng, out),
        110 => misc::mycelium(pos, rng, out),
        116 => enchanting::glyphs(world, pos, rng, out),
        117 => misc::brewing_stand(pos, rng, out),
        119 => portal::end_portal(pos, rng, out),
        130 => portal::ender_chest(pos, rng, out),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::ambient::testing::{kinds, World};
    use crate::particles::ParticleKind;
    use umicraft_world::BlockState;

    fn tick_of(id: u16, metadata: u8) -> Vec<ParticleKind> {
        let mut world = World::default();
        let pos = BlockPos::new(0, 64, 0);
        world.set(0, 64, 0, BlockState::new(id, metadata));
        let mut out = Vec::new();
        let mut rng = JavaRandom::new(2);
        // Several ticks, so the probabilistic blocks show up.
        for _ in 0..400 {
            display_tick(&world, &Dry, pos, &mut rng, &mut out);
        }
        let mut found = kinds(&out);
        found.dedup();
        found
    }

    #[test]
    fn every_ported_block_emits_its_particles() {
        use ParticleKind::*;
        assert_eq!(tick_of(50, 5), [Smoke, Flame].repeat(400));
        assert_eq!(tick_of(62, 2).len(), 800);
        assert_eq!(tick_of(76, 5), [Redstone]);
        assert_eq!(tick_of(55, 9), [Redstone]);
        assert_eq!(tick_of(94, 0), [Redstone]);
        assert_eq!(tick_of(90, 0), [Portal]);
        assert_eq!(tick_of(119, 0), [Smoke]);
        assert_eq!(tick_of(130, 0), [Portal]);
        assert_eq!(tick_of(110, 0), [TownAura]);
        assert_eq!(tick_of(117, 0), [Smoke]);
        assert_eq!(tick_of(19, 1), [DripWater]);
        assert_eq!(tick_of(9, 0), [Suspended]);
        assert_eq!(tick_of(11, 0), [Lava]);
    }

    #[test]
    fn dead_variants_and_plain_blocks_emit_nothing() {
        for (id, metadata) in [
            (1, 0),
            (61, 2),
            (75, 5),
            (93, 0),
            (73, 0),
            (55, 0),
            (19, 0),
            (0, 0),
        ] {
            assert!(tick_of(id, metadata).is_empty(), "block {id}");
        }
    }

    #[test]
    fn fire_on_nothing_with_no_fuel_is_silent_but_with_a_floor_smokes() {
        assert!(tick_of(51, 0).is_empty());
        let mut world = World::default();
        world.set(0, 63, 0, BlockState::new(1, 0));
        world.set(0, 64, 0, BlockState::new(51, 0));
        let mut out = Vec::new();
        display_tick(
            &world,
            &Dry,
            BlockPos::new(0, 64, 0),
            &mut JavaRandom::new(2),
            &mut out,
        );
        assert_eq!(kinds(&out), [ParticleKind::LargeSmoke; 3]);
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the game thread tells the renderer about the fog (P7g): the medium at
//! the eye, the potion effects that change the fog, and the eye light that
//! the render thread smooths over ticks.
//!
//! Sources: MCP `ActiveRenderInfo.getBlockAtEntityViewpoint` (the medium),
//! `EntityRenderer.updateFogColor` / `setupFog` (the effects) and
//! `EntityRenderer.updateRenderer` (the `fogColor1` smoothing).

use glam::Vec3;
use umicraft_game::server::effects::Effects;
use umicraft_render::{light_brightness, FogContext, FogMedium};
use umicraft_world::fluid::{height_percent, Liquid};
use umicraft_world::{BlockPos, World};

/// Potion id of Water Breathing.
const WATER_BREATHING: u8 = 13;
/// Potion id of Blindness.
const BLINDNESS: u8 = 15;
/// Potion id of Night Vision.
const NIGHT_VISION: u8 = 16;
/// Share of the gap to the eye light that one tick closes (`fogColor1`).
const LIGHT_EASING: f32 = 0.1;
/// `BlockLiquid.getLiquidHeightPercent` is lowered by this much for the
/// surface test of the eye (`ActiveRenderInfo.getBlockAtEntityViewpoint`).
const SURFACE_SINK: f32 = 0.111_111_11;

/// The fog inputs of one tick for an eye at `eye`; the light is the
/// instantaneous value, which [`FogLight`] smooths.
#[must_use]
pub(crate) fn fog_context(
    world: Option<&World>,
    effects: &Effects,
    eye: Vec3,
    dimension: i8,
    world_time: i64,
) -> FogContext {
    let remaining = |id: u8| {
        effects
            .get(id)
            .map(|effect| u32::try_from(effect.duration).unwrap_or(0))
    };
    let block = BlockPos::new(
        eye.x.floor() as i32,
        eye.y.floor() as i32,
        eye.z.floor() as i32,
    );
    let light = world.map_or(umicraft_world::LightLevel::FULL_SKY, |world| {
        world.light(block)
    });
    FogContext {
        medium: world.map_or(FogMedium::Air, |world| medium_at(world, eye)),
        blindness_ticks: remaining(BLINDNESS),
        night_vision_ticks: remaining(NIGHT_VISION),
        water_breathing: effects.get(WATER_BREATHING).is_some(),
        eye_y: eye.y,
        light_brightness: light_brightness(light.block, light.sky, dimension, world_time),
    }
}

/// The liquid the eye is in. A liquid block whose surface lies below the eye
/// counts as the block above it (a pool surface does not tint the air).
pub(crate) fn medium_at(world: &World, eye: Vec3) -> FogMedium {
    let pos = BlockPos::new(
        eye.x.floor() as i32,
        eye.y.floor() as i32,
        eye.z.floor() as i32,
    );
    let state = world.block(pos);
    let Some(liquid) = Liquid::of(state) else {
        return FogMedium::Air;
    };
    let surface = (pos.y + 1) as f32 - (height_percent(state.metadata()) - SURFACE_SINK);
    let inside = if eye.y >= surface {
        Liquid::of(world.block(BlockPos::new(pos.x, pos.y + 1, pos.z)))
    } else {
        Some(liquid)
    };
    match inside {
        Some(Liquid::Water) => FogMedium::Water,
        Some(Liquid::Lava) => FogMedium::Lava,
        None => FogMedium::Air,
    }
}

/// The eye light brightness eased once per tick, interpolated between ticks.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct FogLight {
    /// The eased value of the previous tick and of the newest one; `None`
    /// before the first tick, which snaps to its target.
    eased: Option<(f32, f32)>,
}

impl FogLight {
    /// Advances one tick toward `target` (`fogColor1 += (f2 - fogColor1) * 0.1`).
    pub(crate) fn tick(&mut self, target: f32) {
        self.eased = Some(match self.eased {
            None => (target, target),
            Some((_, current)) => (current, current + (target - current) * LIGHT_EASING),
        });
    }

    /// The value `partial` of the way between the last two ticks; full light
    /// before the first tick.
    pub(crate) fn at(&self, partial: f32) -> f32 {
        self.eased.map_or(1.0, |(previous, current)| {
            previous + (current - previous) * partial
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::server::effects::Effects;
    use umicraft_protocol::packets::play::clientbound::entity_effect::EntityEffect;
    use umicraft_world::{BlockState, ChunkPayload, ChunkPos};

    const WATER: BlockState = BlockState::new(9, 0);
    const LAVA: BlockState = BlockState::new(11, 0);

    /// One loaded column with the given blocks.
    fn world_with(blocks: &[(BlockPos, BlockState)]) -> World {
        let mut world = World::new(0);
        let data = vec![0u8; 8192 + 2048 + 2048 + 256];
        let payload = ChunkPayload {
            primary_bit_mask: 1,
            ground_up: true,
            data: &data,
        };
        world.apply_chunk(ChunkPos::new(0, 0), payload).unwrap();
        for (pos, state) in blocks {
            world.set_block(*pos, *state);
        }
        world
    }

    fn effect(id: i8, duration: i32) -> EntityEffect {
        EntityEffect {
            entity_id: 1,
            effect_id: id,
            amplifier: 0,
            duration,
            hide_particles: 0,
        }
    }

    fn context(world: &World, effects: &Effects, y: f32) -> FogContext {
        fog_context(Some(world), effects, Vec3::new(8.5, y, 8.5), 0, 6_000)
    }

    #[test]
    fn the_eye_in_a_flowing_block_clears_the_surface_but_a_source_never_does() {
        const FLOWING: BlockState = BlockState::new(8, 4);
        let world = world_with(&[
            (BlockPos::new(8, 5, 8), FLOWING),
            (BlockPos::new(8, 9, 8), WATER),
        ]);
        let none = Effects::default();
        // Level 4 reaches `5/9 - 1/9` of the block under its top.
        assert_eq!(context(&world, &none, 5.2).medium, FogMedium::Water);
        assert_eq!(context(&world, &none, 5.7).medium, FogMedium::Air);
        // A source block counts as full to the top.
        assert_eq!(context(&world, &none, 9.95).medium, FogMedium::Water);
    }

    #[test]
    fn water_above_the_eye_block_keeps_it_submerged_and_lava_is_lava() {
        let world = world_with(&[
            (BlockPos::new(8, 5, 8), WATER),
            (BlockPos::new(8, 6, 8), WATER),
            (BlockPos::new(8, 9, 8), LAVA),
        ]);
        let none = Effects::default();
        assert_eq!(context(&world, &none, 5.99).medium, FogMedium::Water);
        assert_eq!(context(&world, &none, 9.2).medium, FogMedium::Lava);
    }

    #[test]
    fn no_world_means_air_at_full_light() {
        let fog = fog_context(
            None,
            &Effects::default(),
            Vec3::new(0.0, 70.0, 0.0),
            0,
            6_000,
        );
        assert_eq!(fog.medium, FogMedium::Air);
        assert_eq!(fog.light_brightness, 1.0);
        assert_eq!(fog.eye_y, 70.0);
    }

    #[test]
    fn the_potion_effects_reach_the_fog() {
        let world = world_with(&[]);
        let mut effects = Effects::default();
        effects.apply(&effect(15, 100));
        effects.apply(&effect(16, 5_000));
        effects.apply(&effect(13, 400));
        let fog = context(&world, &effects, 70.0);
        assert_eq!(fog.blindness_ticks, Some(100));
        assert_eq!(fog.night_vision_ticks, Some(5_000));
        assert!(fog.water_breathing);
        let plain = context(&world, &Effects::default(), 70.0);
        assert_eq!(plain.blindness_ticks, None);
        assert!(!plain.water_breathing);
    }

    #[test]
    fn the_light_eases_by_a_tenth_per_tick_and_interpolates() {
        let mut light = FogLight::default();
        assert_eq!(light.at(0.5), 1.0, "full light before the first tick");
        light.tick(1.0);
        light.tick(0.0);
        assert!((light.at(1.0) - 0.9).abs() < 1e-6);
        assert!((light.at(0.0) - 1.0).abs() < 1e-6);
        assert!((light.at(0.5) - 0.95).abs() < 1e-6);
    }
}

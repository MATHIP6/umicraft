// SPDX-License-Identifier: MIT OR Apache-2.0

//! The world light an entity is drawn with.
//!
//! MCP `Entity.getBrightnessForRender` samples the combined light at the block
//! holding `(x, y + eyeHeight, z)`, and a position outside the loaded chunks
//! reads black. Dropped items, item frames and items in a hand take their light
//! from that block, so the game thread samples it once per tick and ships it
//! with the entity ([`EntityFrameData::light`](super::EntityFrameData)).

use glam::Vec3;
use umicraft_world::{BlockPos, LightLevel, World};

use super::kind::EntityKind;
use super::snapshot::EntityFrameData;

/// `EntityPlayer.getEyeHeight`; every other entity has an eye height of zero
/// (`Entity.getEyeHeight`).
const PLAYER_EYE_HEIGHT: f32 = 1.62;

/// The block whose light lights an entity of `kind` at `position`.
#[must_use]
pub fn light_block(kind: EntityKind, position: Vec3) -> BlockPos {
    let eye = if kind == EntityKind::Player {
        PLAYER_EYE_HEIGHT
    } else {
        0.0
    };
    block_containing(position + Vec3::new(0.0, eye, 0.0))
}

/// The block containing `point`.
fn block_containing(point: Vec3) -> BlockPos {
    BlockPos::new(
        point.x.floor() as i32,
        point.y.floor() as i32,
        point.z.floor() as i32,
    )
}

/// The light at the eye of the local player, which lights the first-person
/// item (MCP `ItemRenderer.func_178109_a`).
#[must_use]
pub fn eye_light(world: &World, eye: Vec3) -> LightLevel {
    world.light(block_containing(eye))
}

/// The light an entity of `kind` at `position` is drawn with; black outside
/// the loaded chunks.
#[must_use]
pub fn light_at(world: &World, kind: EntityKind, position: Vec3) -> LightLevel {
    world.light(light_block(kind, position))
}

/// Samples the light of every entity in `entities` from `world`, at the
/// position of the current tick.
pub fn light_entities(world: &World, entities: &mut [EntityFrameData]) {
    for entity in entities {
        entity.light = light_at(world, entity.kind, entity.position);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dropped_item_is_lit_by_the_block_it_is_in() {
        let block = light_block(EntityKind::Item, Vec3::new(3.7, 64.2, -0.5));
        assert_eq!(block, BlockPos::new(3, 64, -1));
    }

    #[test]
    fn a_player_is_lit_at_eye_height() {
        let block = light_block(EntityKind::Player, Vec3::new(0.5, 64.0, 0.5));
        assert_eq!(block, BlockPos::new(0, 65, 0));
    }

    #[test]
    fn sampling_replaces_the_default_full_sky_light() {
        let mut entities = [crate::entity::EntityRecord::new(
            super::super::kind::EntityId(1),
            EntityKind::Item,
            glam::DVec3::new(1.0, 64.0, 1.0),
            0.0,
            0.0,
        )
        .frame_data()];
        assert_eq!(entities[0].light, LightLevel::FULL_SKY);
        light_entities(&World::default(), &mut entities);
        assert_eq!(entities[0].light, LightLevel::DARK);
    }

    #[test]
    fn an_unloaded_position_is_dark() {
        let light = light_at(
            &World::default(),
            EntityKind::Item,
            Vec3::new(1.0, 64.0, 1.0),
        );
        assert_eq!(light, LightLevel::DARK);
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Which entities are not meshed because the player cannot see them (perf 2, C1).
//!
//! Meshing an entity costs 3 to 7 us a frame, and the reference draws only what is
//! in range (`RenderManager.shouldRender`). The rule here is cautious: an entity
//! is skipped when it is far, or when it lies behind the camera by more than its
//! own size. The list of entities that the meshing receives stays whole, because
//! a bobber, a leash or the beam of a crystal reads another entity from it.

use glam::Vec3;
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};

/// Past this distance, in blocks, nothing is meshed (the reference stops at 64 for
/// an entity of ordinary size).
const MAX_DISTANCE: f32 = 128.0;
/// Inside this distance every entity is meshed.
const NEAR: f32 = 8.0;
/// How far behind the camera plane an ordinary entity must be to be skipped, in
/// blocks: more than its size.
const MARGIN: f32 = 3.0;
/// The same for the very large ones and the bolt, which reaches far.
const BIG_MARGIN: f32 = 64.0;

/// Where the player looks, in the space of the entity snapshots (relative to the
/// render origin).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CullView {
    /// The eye.
    pub eye: Vec3,
    /// The unit look direction.
    pub forward: Vec3,
}

/// Whether the entity is large enough to keep far behind the camera.
fn is_big(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Dragon
            | EntityKind::Giant
            | EntityKind::Ghast
            | EntityKind::Wither
            | EntityKind::Slime
            | EntityKind::MagmaCube
            | EntityKind::Lightning
    )
}

/// Whether `entity` is left out of the mesh for `view`.
#[must_use]
pub fn out_of_view(entity: &EntityRenderSnapshot, view: CullView) -> bool {
    let offset = entity.position - view.eye;
    let distance_squared = offset.length_squared();
    if distance_squared > MAX_DISTANCE * MAX_DISTANCE && !is_big(entity.kind) {
        return true;
    }
    if distance_squared < NEAR * NEAR {
        return false;
    }
    let margin = if is_big(entity.kind) {
        BIG_MARGIN
    } else {
        MARGIN
    };
    offset.dot(view.forward) < -margin
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::EntityId;

    const VIEW: CullView = CullView {
        eye: Vec3::ZERO,
        forward: Vec3::NEG_Z,
    };

    fn at(kind: EntityKind, position: Vec3) -> EntityRenderSnapshot {
        EntityRenderSnapshot::new(EntityId(1), kind, position)
    }

    #[test]
    fn what_is_in_front_or_close_is_meshed() {
        let ahead = at(EntityKind::Cow, Vec3::new(0.0, 0.0, -30.0));
        let side = at(EntityKind::Cow, Vec3::new(20.0, 0.0, -5.0));
        let close_behind = at(EntityKind::Cow, Vec3::new(0.0, 0.0, 5.0));
        assert!(!out_of_view(&ahead, VIEW));
        assert!(!out_of_view(&side, VIEW), "to the side");
        assert!(!out_of_view(&close_behind, VIEW), "close behind");
    }

    #[test]
    fn what_is_far_behind_is_skipped_but_not_a_giant() {
        let behind = Vec3::new(0.0, 0.0, 30.0);
        assert!(out_of_view(&at(EntityKind::Cow, behind), VIEW));
        assert!(!out_of_view(&at(EntityKind::Dragon, behind), VIEW));
        assert!(!out_of_view(&at(EntityKind::Lightning, behind), VIEW));
    }

    #[test]
    fn what_is_beyond_the_range_is_skipped_unless_it_is_big() {
        let far = Vec3::new(0.0, 0.0, -200.0);
        assert!(out_of_view(&at(EntityKind::Zombie, far), VIEW));
        assert!(!out_of_view(&at(EntityKind::Dragon, far), VIEW));
    }

    #[test]
    fn the_margin_keeps_an_entity_just_behind_the_plane() {
        // 2 blocks behind the camera plane but 20 to the side: its body may reach in.
        let edge = Vec3::new(20.0, 0.0, 2.0);
        assert!(!out_of_view(&at(EntityKind::Cow, edge), VIEW));
    }
}

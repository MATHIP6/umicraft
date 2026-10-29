// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the crosshair points at: a block, an entity or nothing.
//!
//! [`pick_target`] ports MCP `EntityRenderer.getMouseOver`. The block ray
//! comes first; the entity ray is then cast up to the entity reach, and the
//! nearest entity wins when it is closer than the block. Two quirks of the
//! original are kept: in survival an entity farther than 3 blocks does not
//! count and also hides the block behind it (the target becomes a miss), and
//! the vehicle of the player is only targeted when the player stands inside
//! another entity's box.

use glam::DVec3;

use super::hitbox::hitbox;
use crate::entity::{EntityId, EntityRecord};
use crate::raycast::RayHit;
use crate::shapes::Aabb;

/// The entity reach in survival (MCP `EntityRenderer.getMouseOver`, `3.0D`).
const SURVIVAL_ENTITY_REACH: f64 = 3.0;

/// The ray length in creative mode (MCP `extendedReach`, `6.0D`).
const CREATIVE_ENTITY_REACH: f64 = 6.0;

/// An entity the ray hit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntityHit {
    /// The entity.
    pub id: EntityId,
    /// Where the ray entered its box (the eye when it starts inside).
    pub point: DVec3,
    /// The feet position of the entity, to turn `point` into the offset
    /// `Use Entity` sends.
    pub origin: DVec3,
}

impl EntityHit {
    /// The hit point relative to the entity (MCP `func_178894_a`).
    #[must_use]
    pub fn offset(&self) -> DVec3 {
        self.point - self.origin
    }
}

/// The result of [`pick_target`] (MCP `Minecraft.objectMouseOver`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pointed {
    /// Nothing within reach.
    Miss,
    /// A block.
    Block(RayHit),
    /// An entity.
    Entity(EntityHit),
}

impl Pointed {
    /// The block hit, if the crosshair is on a block.
    #[must_use]
    pub fn block(self) -> Option<RayHit> {
        match self {
            Self::Block(hit) => Some(hit),
            _ => None,
        }
    }

    /// The entity hit, if the crosshair is on an entity.
    #[must_use]
    pub fn entity(self) -> Option<EntityHit> {
        match self {
            Self::Entity(hit) => Some(hit),
            _ => None,
        }
    }
}

/// The inputs of [`pick_target`].
#[derive(Debug, Clone, Copy)]
pub struct TargetQuery {
    /// The eye of the player.
    pub eye: DVec3,
    /// The unit look vector.
    pub look: DVec3,
    /// The block reach of the game mode (4.5, or 5 in creative).
    pub block_reach: f64,
    /// True in creative mode (MCP `extendedReach`).
    pub extended_reach: bool,
    /// The block under the crosshair within `block_reach`.
    pub block: Option<RayHit>,
    /// The entity the player rides, which the ray skips.
    pub vehicle: Option<EntityId>,
}

/// Casts the crosshair at the blocks and at `entities`.
#[must_use]
pub fn pick_target<'a>(
    query: &TargetQuery,
    entities: impl IntoIterator<Item = &'a EntityRecord>,
) -> Pointed {
    let TargetQuery {
        eye,
        look,
        block_reach,
        extended_reach,
        block,
        vehicle,
    } = *query;
    let (reach, limited) = if extended_reach {
        (CREATIVE_ENTITY_REACH, false)
    } else {
        (block_reach, block_reach > SURVIVAL_ENTITY_REACH)
    };
    let end = eye + look * reach;
    let block_distance = block.map_or(block_reach, |hit| hit.distance);
    let mut nearest = block_distance;
    let mut best: Option<EntityHit> = None;
    for entity in entities {
        let Some(bounds) = hitbox(entity) else {
            continue;
        };
        let hit = intercept(&bounds, eye, end);
        if contains(&bounds, eye) {
            best = Some(entity_hit(entity, hit.unwrap_or(eye)));
            nearest = 0.0;
        } else if let Some(point) = hit {
            let distance = eye.distance(point);
            if distance < nearest || nearest == 0.0 {
                if Some(entity.id) == vehicle {
                    if nearest == 0.0 {
                        best = Some(entity_hit(entity, point));
                    }
                } else {
                    best = Some(entity_hit(entity, point));
                    nearest = distance;
                }
            }
        }
    }
    match best {
        Some(hit) if limited && eye.distance(hit.point) > SURVIVAL_ENTITY_REACH => Pointed::Miss,
        Some(hit) if nearest < block_distance || block.is_none() => Pointed::Entity(hit),
        _ => block.map_or(Pointed::Miss, Pointed::Block),
    }
}

fn entity_hit(entity: &EntityRecord, point: DVec3) -> EntityHit {
    EntityHit {
        id: entity.id,
        point,
        origin: entity.position,
    }
}

/// Whether `point` lies strictly inside the box (MCP `isVecInside`).
fn contains(bounds: &Aabb, point: DVec3) -> bool {
    point.cmpgt(bounds.min).all() && point.cmplt(bounds.max).all()
}

/// Where the segment `from..to` first touches the box (MCP
/// `AxisAlignedBB.calculateIntercept`), the exit point when it starts
/// inside, `None` when it misses.
fn intercept(bounds: &Aabb, from: DVec3, to: DVec3) -> Option<DVec3> {
    let delta = to - from;
    let mut enter = 0.0_f64;
    let mut exit = 1.0_f64;
    for axis in 0..3 {
        let (start, step) = (from[axis], delta[axis]);
        if step.abs() < f64::EPSILON {
            if start < bounds.min[axis] || start > bounds.max[axis] {
                return None;
            }
            continue;
        }
        let t1 = (bounds.min[axis] - start) / step;
        let t2 = (bounds.max[axis] - start) / step;
        enter = enter.max(t1.min(t2));
        exit = exit.min(t1.max(t2));
        if enter > exit {
            return None;
        }
    }
    let inside = contains(bounds, from);
    Some(from + delta * if inside { exit } else { enter })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::EntityKind;
    use crate::raycast::Face;
    use umicraft_world::BlockPos;

    const LOOK: DVec3 = DVec3::new(0.0, 0.0, 1.0);

    fn zombie(id: i32, z: f64) -> EntityRecord {
        EntityRecord::new(
            EntityId(id),
            EntityKind::Zombie,
            DVec3::new(0.0, 0.0, z),
            0.0,
            0.0,
        )
    }

    fn block_at(distance: i32) -> RayHit {
        RayHit {
            block: BlockPos::new(0, 1, distance),
            face: Face::North,
            position: DVec3::new(0.0, 1.0, f64::from(distance)),
            distance: f64::from(distance),
        }
    }

    fn pick(entities: &[EntityRecord], block: Option<RayHit>, creative: bool) -> Pointed {
        pick_target(
            &TargetQuery {
                eye: DVec3::new(0.0, 1.62, 0.0),
                look: LOOK,
                block_reach: if creative { 5.0 } else { 4.5 },
                extended_reach: creative,
                block,
                vehicle: None,
            },
            entities,
        )
    }

    #[test]
    fn an_entity_in_front_of_a_block_wins() {
        let target = pick(&[zombie(7, 2.0)], Some(block_at(4)), false);
        assert_eq!(target.entity().unwrap().id, EntityId(7));
    }

    #[test]
    fn a_block_in_front_of_an_entity_wins() {
        let target = pick(&[zombie(7, 3.0)], Some(block_at(2)), false);
        assert_eq!(target.block(), Some(block_at(2)));
    }

    #[test]
    fn the_entity_hit_is_where_the_ray_enters_the_box() {
        let hit = pick(&[zombie(7, 2.0)], None, false).entity().unwrap();
        assert!((hit.point.z - (2.0 - 0.4)).abs() < 1e-6);
        assert!((hit.offset().z + 0.4).abs() < 1e-6);
    }

    #[test]
    fn a_survival_entity_beyond_three_blocks_hides_the_block_behind_it() {
        // MCP getMouseOver: the far entity is dropped and the target is a miss.
        let target = pick(&[zombie(7, 4.0)], Some(block_at(4)), false);
        assert_eq!(target, Pointed::Miss);
        let in_the_open = pick(&[zombie(7, 4.0)], None, false);
        assert_eq!(in_the_open, Pointed::Miss);
    }

    #[test]
    fn a_creative_player_is_not_limited_to_three_blocks() {
        // The ray is 6 long but, without a block, only entities nearer than
        // the block reach (5) count, like `d1` in MCP getMouseOver.
        let target = pick(&[zombie(7, 4.8)], None, true);
        assert_eq!(target.entity().unwrap().id, EntityId(7));
        assert_eq!(pick(&[zombie(7, 5.8)], None, true), Pointed::Miss);
    }

    #[test]
    fn the_nearest_of_two_entities_wins() {
        let target = pick(&[zombie(1, 2.8), zombie(2, 1.5)], None, false);
        assert_eq!(target.entity().unwrap().id, EntityId(2));
    }

    #[test]
    fn an_entity_beside_the_ray_is_missed() {
        let mut off = zombie(7, 2.0);
        off.position.x = 2.0;
        assert_eq!(pick(&[off], None, false), Pointed::Miss);
    }

    #[test]
    fn standing_inside_a_box_targets_it_at_the_eye() {
        let hit = pick(&[zombie(7, 0.0)], Some(block_at(3)), false)
            .entity()
            .unwrap();
        assert!(hit.point.z > 0.0, "the exit point of the box");
    }

    #[test]
    fn the_vehicle_is_skipped_by_the_ray() {
        let boat = zombie(9, 1.0);
        let target = pick_target(
            &TargetQuery {
                eye: DVec3::new(0.0, 1.62, -1.0),
                look: LOOK,
                block_reach: 4.5,
                extended_reach: false,
                block: None,
                vehicle: Some(EntityId(9)),
            },
            &[boat],
        );
        assert_eq!(target, Pointed::Miss);
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! A minecart on its rails (MCP `EntityMinecart.func_70489_a`,
//! `func_70495_a`, `RenderMinecart.doRender`).
//!
//! The server sends a cart's position a few times a second; the reference
//! renderer pulls the interpolated position onto the rail under it, and turns
//! and tilts the cart along the two points a third of a block either side. The
//! renderer has no world, so the game thread does it once per tick, on both
//! ends of the interpolation.

use glam::Vec3;
use umicraft_world::{BlockPos, World};

use super::kind::EntityKind;
use super::snapshot::EntityFrameData;

/// The two ends of each rail shape, `[from, to]` as `[x, y, z]` offsets in
/// blocks (MCP `EntityMinecart.matrix`, indexed by the shape number).
const MATRIX: [[[i32; 3]; 2]; 10] = [
    [[0, 0, -1], [0, 0, 1]],
    [[-1, 0, 0], [1, 0, 0]],
    [[-1, -1, 0], [1, 0, 0]],
    [[-1, 0, 0], [1, -1, 0]],
    [[0, 0, -1], [0, -1, 1]],
    [[0, -1, -1], [0, 0, 1]],
    [[0, 0, 1], [1, 0, 0]],
    [[0, 0, 1], [-1, 0, 0]],
    [[0, 0, -1], [-1, 0, 0]],
    [[0, 0, -1], [1, 0, 0]],
];
/// How far either side of the cart the slope is measured, in blocks.
const SPAN: f64 = 0.300_000_011_920_929;

/// The shape of the rail at `pos`, or `None` when it is not a rail
/// (`BlockRailBase.isRailBlock`).
fn shape(world: &World, pos: BlockPos) -> Option<usize> {
    let block = world.block(pos);
    let meta = block.metadata();
    let shape = match block.id() {
        66 => meta & 15,
        27 | 28 | 157 => meta & 7,
        _ => return None,
    };
    // Shapes 0 to 9 only; a plain rail stores 10 to 15 for nothing.
    (shape < 10).then_some(usize::from(shape))
}

/// The rail the cart at `(x, y, z)` stands on: a rail one block down wins
/// over the one at the cart.
fn find_rail(world: &World, x: f64, y: f64, z: f64) -> Option<(BlockPos, usize)> {
    let (i, mut j, k) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    if shape(world, BlockPos::new(i, j - 1, k)).is_some() {
        j -= 1;
    }
    let pos = BlockPos::new(i, j, k);
    shape(world, pos).map(|shape| (pos, shape))
}

/// Where the cart at `(x, y, z)` lies on its rail (`func_70489_a`).
fn snap(world: &World, x: f64, y: f64, z: f64) -> Option<[f64; 3]> {
    let (pos, shape) = find_rail(world, x, y, z)?;
    let [from, to] = MATRIX[shape];
    let (i, j, k) = (f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
    let end = |end: [i32; 3]| {
        [
            i + 0.5 + f64::from(end[0]) * 0.5,
            j + 0.0625 + f64::from(end[1]) * 0.5,
            k + 0.5 + f64::from(end[2]) * 0.5,
        ]
    };
    let (a, b) = (end(from), end(to));
    let (dx, dy, dz) = (b[0] - a[0], (b[1] - a[1]) * 2.0, b[2] - a[2]);
    let along = if dx == 0.0 {
        z - k
    } else if dz == 0.0 {
        x - i
    } else {
        ((x - a[0]) * dx + (z - a[2]) * dz) * 2.0
    };
    let mut height = a[1] + dy * along;
    if dy < 0.0 {
        height += 1.0;
    }
    if dy > 0.0 {
        height += 0.5;
    }
    Some([a[0] + dx * along, height, a[2] + dz * along])
}

/// The point `span` blocks further along the rail (`func_70495_a`).
fn ahead(world: &World, x: f64, y: f64, z: f64, span: f64) -> Option<[f64; 3]> {
    let (pos, shape) = find_rail(world, x, y, z)?;
    let [from, to] = MATRIX[shape];
    let (dx, dz) = (f64::from(to[0] - from[0]), f64::from(to[2] - from[2]));
    let length = dx.hypot(dz);
    let (x, z) = (x + dx / length * span, z + dz / length * span);
    let mut y = f64::from(pos.y);
    if (2..=5).contains(&shape) {
        y += 1.0;
    }
    let reaches = |end: [i32; 3]| {
        end[1] != 0 && x.floor() as i32 - pos.x == end[0] && z.floor() as i32 - pos.z == end[2]
    };
    if reaches(from) {
        y += f64::from(from[1]);
    } else if reaches(to) {
        y += f64::from(to[1]);
    }
    snap(world, x, y, z)
}

/// A cart laid on its rail: where it is, and its yaw and pitch in degrees when
/// the track gives it a direction.
type Laid = (Vec3, Option<(f32, f32)>);

/// The cart at `position` laid on its rail, or `None` when it is not on one.
fn on_rail(world: &World, position: Vec3) -> Option<Laid> {
    let (x, y, z) = (
        f64::from(position.x),
        f64::from(position.y),
        f64::from(position.z),
    );
    let here = snap(world, x, y, z)?;
    let front = ahead(world, x, y, z, SPAN).unwrap_or(here);
    let back = ahead(world, x, y, z, -SPAN).unwrap_or(here);
    let laid = Vec3::new(
        here[0] as f32,
        ((front[1] + back[1]) / 2.0) as f32,
        here[2] as f32,
    );
    // `RenderMinecart` turns the cart along `vec32 - vec31`, back minus front.
    let along = [back[0] - front[0], back[1] - front[1], back[2] - front[2]];
    let length = along.iter().map(|d| d * d).sum::<f64>().sqrt();
    if length == 0.0 {
        return Some((laid, None));
    }
    let (dx, dy, dz) = (along[0] / length, along[1] / length, along[2] / length);
    Some((
        laid,
        Some((dz.atan2(dx).to_degrees() as f32, (dy.atan() * 73.0) as f32)),
    ))
}

/// Lays every minecart of `entities` on the rail under it, at both ends of its
/// interpolation.
pub fn rail_entities(world: &World, entities: &mut [EntityFrameData]) {
    for entity in entities
        .iter_mut()
        .filter(|entity| entity.kind == EntityKind::Minecart)
    {
        let Some((position, angles)) = on_rail(world, entity.position) else {
            continue;
        };
        entity.position = position;
        if let Some((yaw, pitch)) = angles {
            entity.yaw = yaw;
            entity.pitch = pitch;
        }
        // A cart that was off the track a tick ago starts where it ends.
        match on_rail(world, entity.prev_position) {
            Some((position, angles)) => {
                entity.prev_position = position;
                if let Some((yaw, pitch)) = angles {
                    entity.prev_yaw = yaw;
                    entity.prev_pitch = pitch;
                }
            }
            None => {
                entity.prev_position = entity.position;
                entity.prev_yaw = entity.yaw;
                entity.prev_pitch = entity.pitch;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{EntityId, EntityRecord};
    use umicraft_world::{BlockState, ChunkPayload};

    /// An empty overworld chunk at the origin with a block at (0, 5, 0).
    fn world_with(id: u16, meta: u8) -> World {
        let mut world = World::new(0);
        // Blocks, block light, sky light, biomes.
        let data = vec![0u8; 8192 + 2048 + 2048 + 256];
        let payload = ChunkPayload {
            primary_bit_mask: 1,
            ground_up: true,
            data: &data,
        };
        world
            .apply_chunk(umicraft_world::ChunkPos::new(0, 0), payload)
            .expect("an empty chunk");
        world.set_block(BlockPos::new(0, 5, 0), BlockState::new(id, meta));
        world
    }

    fn cart(position: Vec3) -> EntityFrameData {
        EntityRecord::new(
            EntityId(1),
            EntityKind::Minecart,
            glam::DVec3::new(
                f64::from(position.x),
                f64::from(position.y),
                f64::from(position.z),
            ),
            0.0,
            0.0,
        )
        .frame_data()
    }

    #[test]
    fn a_cart_between_rails_is_not_moved() {
        let world = world_with(0, 0);
        let mut frame = [cart(Vec3::new(0.3, 5.0, 0.8))];
        rail_entities(&world, &mut frame);
        assert_eq!(frame[0].position, Vec3::new(0.3, 5.0, 0.8));
    }

    #[test]
    fn a_cart_on_a_straight_rail_is_centred_and_lowered_onto_it() {
        // Shape 1 runs east-west.
        let world = world_with(66, 1);
        let (position, angles) = on_rail(&world, Vec3::new(0.3, 5.0, 0.8)).expect("a rail");
        assert!((position.z - 0.5).abs() < 1e-6, "{position}");
        assert!((position.x - 0.3).abs() < 1e-6, "{position}");
        assert!((position.y - 5.0625).abs() < 1e-5, "{position}");
        let (yaw, pitch) = angles.expect("a direction");
        assert!(
            yaw.abs() < 1e-3 || (yaw.abs() - 180.0).abs() < 1e-3,
            "{yaw}"
        );
        assert!(pitch.abs() < 1e-3, "{pitch}");
    }

    #[test]
    fn a_cart_on_a_slope_tilts_along_it() {
        // Shape 2 rises to the east.
        let world = world_with(66, 2);
        let (position, angles) = on_rail(&world, Vec3::new(0.5, 5.0, 0.5)).expect("a rail");
        let (_, pitch) = angles.expect("a direction");
        assert!(pitch.abs() > 10.0, "{pitch}");
        assert!(position.y > 5.4, "{position}");
    }

    #[test]
    fn the_powered_bit_of_a_golden_rail_is_not_a_shape() {
        let world = world_with(27, 8 | 1);
        assert!(on_rail(&world, Vec3::new(0.5, 5.0, 0.5)).is_some());
    }

    #[test]
    fn only_carts_are_moved() {
        let world = world_with(66, 1);
        let mut frame = [cart(Vec3::new(0.3, 5.0, 0.8))];
        frame[0].kind = EntityKind::Pig;
        rail_entities(&world, &mut frame);
        assert_eq!(frame[0].position, Vec3::new(0.3, 5.0, 0.8));
    }
}

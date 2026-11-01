// SPDX-License-Identifier: MIT OR Apache-2.0

//! `BlockEnchantmentTable.randomDisplayTick`: the glyphs that fly from the
//! bookshelves around a table.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;
use umicraft_world::BlockPos;

use super::emit::{emit_with, Requests};
use crate::particles::ParticleKind;
use crate::shapes::BlockSource;

/// The block id of a bookshelf.
const BOOKSHELF: u16 = 47;

/// One tick in sixteen per shelf cell of the 5 x 5 ring, a glyph leaves the
/// shelf (one or two high) towards the table, provided the cell between them
/// is empty (MCP `BlockEnchantmentTable.java:34`). The particle is created above
/// the table with the shelf's offset as velocity, and `EntityEnchantmentTable
/// ParticleFX` flies it back.
pub(super) fn glyphs<S: BlockSource + ?Sized>(
    world: &S,
    pos: BlockPos,
    rng: &mut JavaRandom,
    out: &mut Requests,
) {
    for i in -2..=2_i32 {
        let mut j = -2_i32;
        while j <= 2 {
            // The reference skips the inner rows by jumping `j` to 2.
            if i > -2 && i < 2 && j == -1 {
                j = 2;
            }
            if rng.next_int(16) == 0 {
                glyphs_from_column(world, pos, (i, j), rng, out);
            }
            j += 1;
        }
    }
}

/// The glyph of the first shelf (at height 0, then 1) in the column `(i, j)`
/// away from the table, if the way to the table is clear.
fn glyphs_from_column<S: BlockSource + ?Sized>(
    world: &S,
    pos: BlockPos,
    (i, j): (i32, i32),
    rng: &mut JavaRandom,
    out: &mut Requests,
) {
    for k in 0..=1_i32 {
        if world.block(pos.x + i, pos.y + k, pos.z + j).id() != BOOKSHELF {
            continue;
        }
        // `i / 2` truncates towards zero, so this is the cell next to the table.
        if !world.block(pos.x + i / 2, pos.y, pos.z + j / 2).is_air() {
            break;
        }
        let position = DVec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 2.0,
            f64::from(pos.z) + 0.5,
        );
        let velocity = DVec3::new(
            f64::from(i as f32 + rng.next_float()) - 0.5,
            f64::from(k as f32 - rng.next_float() - 1.0),
            f64::from(j as f32 + rng.next_float()) - 0.5,
        );
        emit_with(out, ParticleKind::EnchantmentTable, position, velocity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::ambient::testing::World;
    use umicraft_world::BlockState;

    fn run(world: &World, ticks: u64) -> Requests {
        let mut rng = JavaRandom::new(21);
        let mut out = Vec::new();
        for _ in 0..ticks {
            glyphs(world, BlockPos::new(0, 64, 0), &mut rng, &mut out);
        }
        out
    }

    #[test]
    fn a_table_without_shelves_makes_nothing() {
        assert!(run(&World::default(), 500).is_empty());
    }

    #[test]
    fn a_shelf_two_blocks_away_throws_glyphs_with_its_offset_as_velocity() {
        let mut world = World::default();
        world.set(2, 64, 0, BlockState::new(BOOKSHELF, 0));
        let out = run(&world, 3200);
        // One cell in sixteen per tick: about 200 glyphs.
        assert!((140..270).contains(&out.len()), "{}", out.len());
        for request in &out {
            assert_eq!(request.kind, ParticleKind::EnchantmentTable);
            assert_eq!(request.position, DVec3::new(0.5, 66.0, 0.5));
            assert!((1.5..2.5).contains(&request.velocity.x));
            assert!((-2.0..-1.0).contains(&request.velocity.y));
            assert!((-0.5..0.5).contains(&request.velocity.z));
        }
    }

    #[test]
    fn a_blocked_way_or_an_inner_ring_shelf_makes_nothing() {
        let mut world = World::default();
        world.set(2, 64, 0, BlockState::new(BOOKSHELF, 0));
        world.set(1, 64, 0, BlockState::new(1, 0));
        // (1, 1) is on the inner ring, which the loop jumps over.
        world.set(1, 64, 1, BlockState::new(BOOKSHELF, 0));
        assert!(run(&world, 3200).is_empty());
    }

    #[test]
    fn a_shelf_one_block_higher_flies_a_level_lower() {
        let mut world = World::default();
        world.set(-2, 65, 0, BlockState::new(BOOKSHELF, 0));
        let out = run(&world, 1600);
        assert!(!out.is_empty());
        assert!(out.iter().all(|r| (-1.0..0.0).contains(&r.velocity.y)));
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The book above an enchanting table (MCP
//! `TileEntityEnchantmentTableRenderer`).
//!
//! The table itself is a block model; the renderer adds the book. Its motion
//! is a game-tick state ([`BookState`]) that the reference updates in
//! `TileEntityEnchantmentTable.update`. The client keeps no tile entities, so
//! the states live here, one per table position, and are stepped from the
//! wall clock of the scene (20 ticks a second) when the book is drawn. A
//! thread-local holds them: the render thread is the only caller, and the
//! scene is shared by reference with renderers that need no state.
//!
//! The model is single sheets, and the entity pass does not cull, so the
//! sheets that face away from the viewer are dropped here ([`facing`]).

use std::cell::RefCell;
use std::collections::HashMap;

use glam::Vec3;
use umicraft_game::server::block_entities::Pos;
use umicraft_mesh::BlockEntityRef;
use umicraft_model::entity::{BookModel, BookPose, ModelVertex, TextureSlot};

use super::book_state::{wrap, BookState};
use super::{push_at_block, Batches, BlockEntityScene};

/// Game ticks per second.
const TICKS_PER_SECOND: f32 = 20.0;
/// Most ticks stepped at once: after a longer stall the book jumps, so a
/// table that comes back into view is not simulated for minutes.
const MAX_CATCH_UP: u64 = 40;
/// States kept before the ones not drawn for a while are dropped.
const MAX_STATES: usize = 256;
/// Ticks a book may go undrawn before its state is dropped.
const STALE_TICKS: u64 = 1200;

/// The state of a table and the tick it was last advanced to.
struct Tracked {
    state: BookState,
    tick: u64,
}

thread_local! {
    static BOOKS: RefCell<HashMap<Pos, Tracked>> = RefCell::new(HashMap::new());
}

/// Draws the book of the enchanting table at `entry`.
pub(super) fn add_book(
    batches: &mut Batches,
    entry: &BlockEntityRef,
    scene: &BlockEntityScene<'_>,
) {
    let [x, y, z] = entry.position;
    let origin = Vec3::new(x as f32, y as f32, z as f32);
    // The state of a table is keyed by its world position.
    let key = scene.world_position(entry);
    let clock = f64::from(scene.seconds.max(0.0)) * f64::from(TICKS_PER_SECOND);
    let tick = clock as u64;
    let partial = (clock - tick as f64) as f32;
    let pose = BOOKS.with(|books| {
        let mut books = books.borrow_mut();
        if books.len() >= MAX_STATES {
            books.retain(|_, tracked| tracked.tick + STALE_TICKS > tick);
        }
        let tracked = books
            .entry((key[0], key[1], key[2]))
            .or_insert_with(|| Tracked {
                state: BookState::new(seed(key)),
                tick,
            });
        advance(tracked, tick, origin + Vec3::splat(0.5), scene.camera);
        pose_of(&tracked.state, partial)
    });
    let eye = scene.camera - origin;
    let vertices = facing(BookModel::emit(pose), eye);
    if !vertices.is_empty() {
        push_at_block(
            batches.entry(TextureSlot::EnchantingBook).or_default(),
            vertices,
            entry,
        );
    }
}

/// Steps `tracked` to `tick`.
fn advance(tracked: &mut Tracked, tick: u64, table: Vec3, viewer: Vec3) {
    tracked.tick = tracked.tick.max(tick.saturating_sub(MAX_CATCH_UP));
    while tracked.tick < tick {
        tracked.state.update(table, viewer);
        tracked.tick += 1;
    }
}

/// The pose between the last two ticks (`renderTileEntityAt`).
fn pose_of(state: &BookState, partial: f32) -> BookPose {
    let lerp = |from: f32, to: f32| from + (to - from) * partial;
    let flip = lerp(state.page_flip_prev, state.page_flip);
    let flipped = |offset: f32| {
        let shifted = flip + offset;
        ((shifted - shifted.trunc()) * 1.6 - 0.3).clamp(0.0, 1.0)
    };
    BookPose {
        age: state.tick_count as f32 + partial,
        // The turn is interpolated along the short way.
        rotation: state.rotation_prev + wrap(state.rotation - state.rotation_prev) * partial,
        flip_right: flipped(0.25),
        flip_left: flipped(0.75),
        spread: lerp(state.spread_prev, state.spread),
    }
}

/// A seed that differs between tables (the reference shares one generator).
fn seed(position: [i32; 3]) -> u64 {
    let [x, y, z] = position.map(|c| i64::from(c) as u64);
    x.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ y.wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ z.wrapping_mul(0x1656_67B1_9E37_79F9)
}

/// The triangles of `vertices` whose front side looks at `eye` (the winding
/// is counter-clockwise seen from outside); degenerate ones go too.
fn facing(vertices: Vec<ModelVertex>, eye: Vec3) -> Vec<ModelVertex> {
    vertices
        .chunks_exact(3)
        .filter(|triangle| {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(triangle[i].position));
            (b - a).cross(c - a).dot(eye - a) > 0.0
        })
        .flatten()
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use umicraft_game::server::block_entities::{EntityTable, SignTable};
    use umicraft_game::server::chest_lids::ChestLids;
    use umicraft_world::{BlockState, LightLevel};

    use super::*;

    fn entry() -> BlockEntityRef {
        BlockEntityRef {
            position: [4, 70, 9],
            state: BlockState::new(116, 0),
            same_neighbour: umicraft_mesh::SameNeighbours::default(),
            light: LightLevel { block: 0, sky: 15 },
        }
    }

    fn draw(seconds: f32, camera: Vec3) -> Batches {
        let (lids, signs, data) = (ChestLids::default(), SignTable::new(), EntityTable::new());
        let scene = BlockEntityScene {
            lids: &lids,
            signs: &signs,
            data: &data,
            seconds,
            font: None,
            skins: &crate::entity_render::NoSkins,
            partial_tick: 0.0,
            camera,
            origin: [0; 3],
        };
        let mut batches = Batches::new();
        add_book(&mut batches, &entry(), &scene);
        batches
    }

    #[test]
    fn a_book_is_drawn_whole_sheets_facing_the_viewer() {
        let batches = draw(0.0, Vec3::new(4.5, 71.6, 12.0));
        let vertices = &batches[&TextureSlot::EnchantingBook];
        assert!(!vertices.is_empty());
        assert_eq!(vertices.len() % 3, 0);
        // Seven boxes of 12 triangles; the viewer sees fewer than all of them.
        assert!(vertices.len() < 7 * 36);
    }

    #[test]
    fn the_book_opens_while_the_viewer_stays_near() {
        let eye = Vec3::new(6.5, 70.0 + 1.62, 9.5);
        let shut = draw(0.01, eye)[&TextureSlot::EnchantingBook].clone();
        let open = draw(2.0, eye)[&TextureSlot::EnchantingBook].clone();
        let width = |vs: &[umicraft_render::EntityVertex]| {
            vs.iter().map(|v| v.position[2]).fold(f32::MIN, f32::max)
                - vs.iter().map(|v| v.position[2]).fold(f32::MAX, f32::min)
        };
        assert!(width(&open) > width(&shut));
    }

    #[test]
    fn a_stall_catches_up_by_a_bounded_number_of_ticks() {
        let mut tracked = Tracked {
            state: BookState::new(1),
            tick: 0,
        };
        advance(&mut tracked, 100_000, Vec3::ZERO, Vec3::ZERO);
        assert_eq!(tracked.tick, 100_000);
        assert_eq!(u64::from(tracked.state.tick_count), MAX_CATCH_UP);
    }

    #[test]
    fn the_flip_and_the_turn_are_interpolated_within_their_ranges() {
        let mut state = BookState::new(3);
        state.rotation_prev = 3.0;
        state.rotation = -3.0;
        state.page_flip_prev = 1.0;
        state.page_flip = 1.2;
        let pose = pose_of(&state, 0.5);
        // The short way from 3.0 to -3.0 passes through PI, not 0.
        assert!(pose.rotation.abs() > 3.0, "{}", pose.rotation);
        assert!((0.0..=1.0).contains(&pose.flip_right));
        assert!((0.0..=1.0).contains(&pose.flip_left));
    }
}

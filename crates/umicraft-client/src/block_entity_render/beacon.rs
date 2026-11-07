// SPDX-License-Identifier: MIT OR Apache-2.0

//! The beacon beam (MCP `TileEntityBeaconRenderer`, `TileEntityBeacon`).
//!
//! In the reference the beam is two nested prisms from the beacon to the build
//! limit: an inner opaque one of half-width 0.2 that turns around the beacon
//! and an outer translucent one (alpha 0.125) from 0.2 to 0.8, both with
//! `entity/beacon_beam.png` scrolling upwards. It exists once the pyramid is
//! complete (`Levels > 0`) and nothing opaque is above the beacon
//! (`TileEntityBeacon.updateSegmentColors`).
//!
//! Differences, forced by what the client knows and by the additive pipeline
//! (the only entity pipeline that does not light, shade or cut the texture):
//!
//! - the pyramid is the server's `Levels`; the open sky is read from the sky
//!   light of the beacon's own cell, which stained glass does not reduce;
//! - stained glass above the beacon does not tint or split the beam (it is
//!   white), because the client has no block access here;
//! - the inner prism is added instead of replacing the frame, and the outer one
//!   is added with its tint scaled by the alpha 0.125 instead of blended;
//! - the fade-in (`shouldBeamRender`, 0.025 per tick) is not tracked: the beam
//!   is always full length, and the fog is not disabled for it.

use std::collections::HashMap;
use std::f64::consts::PI;

use umicraft_game::nbt::Nbt;
use umicraft_mesh::BlockEntityRef;
use umicraft_model::entity::TextureSlot;
use umicraft_render::EntityVertex;

use super::additive::{tint_word, vertex};
use super::BlockEntityScene;

/// The `action` of an Update Block Entity packet for a beacon.
const BEACON_ACTION: u8 = 3;
/// The top of the beam: the build limit.
const BUILD_LIMIT: f64 = 256.0;
/// Sky light of a cell open to the sky.
const OPEN_SKY: u8 = 15;
/// Ticks per second, to turn the client clock into the world time of the
/// reference (`getTotalWorldTime`).
const TICKS_PER_SECOND: f64 = 20.0;
/// Alpha of the outer prism.
const OUTER_ALPHA: f32 = 0.125;
/// Half-width of the rotating inner prism.
const INNER_RADIUS: f64 = 0.2;

/// Draws the beam of the beacon at `entry` into `batches`, if it has one.
pub(super) fn add_beam(
    batches: &mut HashMap<TextureSlot, Vec<EntityVertex>>,
    entry: &BlockEntityRef,
    scene: &BlockEntityScene<'_>,
) {
    // The table is keyed by world position; the beam is drawn at the local one.
    let [wx, wy, wz] = scene.world_position(entry);
    let [x, y, z] = entry.position;
    let pyramid = scene
        .data
        .get(&(wx, wy, wz))
        .filter(|data| data.action == BEACON_ACTION)
        .map_or(0, |data| levels(data.nbt.as_deref()));
    if pyramid <= 0 || entry.light.sky < OPEN_SKY {
        return;
    }
    let height = BUILD_LIMIT - f64::from(wy);
    let ticks = f64::from(scene.seconds) * TICKS_PER_SECOND;
    batches
        .entry(TextureSlot::BeaconBeam)
        .or_default()
        .extend(beam_vertices([x, y, z], height, ticks));
}

/// `Levels` of a beacon update; anything unreadable is 0, no beam (the bytes
/// come from the network).
fn levels(bytes: Option<&[u8]>) -> i32 {
    bytes
        .and_then(|bytes| Nbt::parse(bytes).ok())
        .and_then(|root| root.get("Levels").and_then(Nbt::as_int))
        .unwrap_or(0)
}

/// The two prisms of a beam of `height` blocks above the block at `block`,
/// `ticks` after the world started.
fn beam_vertices(block: [i32; 3], height: f64, ticks: f64) -> Vec<EntityVertex> {
    let origin = [
        f64::from(block[0]),
        f64::from(block[1]),
        f64::from(block[2]),
    ];
    // `-1 + frac(-t * 0.2 - floor(-t * 0.1))`: the texture runs upwards.
    let phase = -ticks * 0.2 - (-ticks * 0.1).floor();
    let scroll = -1.0 + (phase - phase.floor());
    let turn = ticks * 0.025 * -1.5;
    let mut out = Vec::with_capacity(32);

    // Inner prism: a square of corners at 45, 135, 225 and 315 degrees.
    let corner = |angle: f64| {
        [
            0.5 + (turn + angle).cos() * INNER_RADIUS,
            0.5 + (turn + angle).sin() * INNER_RADIUS,
        ]
    };
    let corners = [
        corner(PI * 0.75),
        corner(PI * 0.25),
        corner(PI * 1.25),
        corner(PI * 1.75),
    ];
    // The faces of the reference, as corner pairs (a, b).
    for (a, b) in [(0, 1), (3, 2), (1, 3), (2, 0)] {
        push_face(
            &mut out,
            origin,
            [corners[a], corners[b]],
            height,
            (scroll, height * 2.5 + scroll),
            tint_word([1.0; 3]),
        );
    }

    // Outer prism: the square from 0.2 to 0.8.
    let outer = tint_word([OUTER_ALPHA; 3]);
    for pair in [
        [[0.2, 0.2], [0.8, 0.2]],
        [[0.8, 0.2], [0.8, 0.8]],
        [[0.8, 0.8], [0.2, 0.8]],
        [[0.2, 0.8], [0.2, 0.2]],
    ] {
        push_face(
            &mut out,
            origin,
            pair,
            height,
            (scroll, height + scroll),
            outer,
        );
    }
    out
}

/// One vertical face of the beam between two `[x, z]` corners (offsets in the
/// block), `v` being the texture rows at its bottom and top. The texture runs
/// across the face from `u = 1` at the first corner to `0` at the second.
fn push_face(
    out: &mut Vec<EntityVertex>,
    origin: [f64; 3],
    [first, second]: [[f64; 2]; 2],
    height: f64,
    (v_bottom, v_top): (f64, f64),
    tint: u32,
) {
    let point = |corner: [f64; 2], up: f64, u: f32, v: f64| {
        vertex(
            [
                (origin[0] + corner[0]) as f32,
                (origin[1] + up) as f32,
                (origin[2] + corner[1]) as f32,
            ],
            [u, v as f32],
            tint,
        )
    };
    out.extend([
        point(first, height, 1.0, v_top),
        point(first, 0.0, 1.0, v_bottom),
        point(second, 0.0, 0.0, v_bottom),
        point(second, height, 0.0, v_top),
    ]);
}

#[cfg(test)]
mod tests {
    use umicraft_game::server::block_entities::{BlockEntityData, EntityTable};
    use umicraft_game::server::chest_lids::ChestLids;
    use umicraft_mesh::SameNeighbours;
    use umicraft_world::{BlockState, LightLevel};

    use super::*;

    /// `{Levels: 4}` as an NBT compound with an empty root name.
    fn levels_nbt(levels: i32) -> Vec<u8> {
        let mut bytes = vec![10, 0, 0, 3, 0, 6];
        bytes.extend(b"Levels");
        bytes.extend(levels.to_be_bytes());
        bytes.push(0);
        bytes
    }

    fn beacon(sky: u8) -> BlockEntityRef {
        BlockEntityRef {
            position: [4, 70, 9],
            state: BlockState::new(138, 0),
            same_neighbour: SameNeighbours::default(),
            light: LightLevel { block: 0, sky },
        }
    }

    fn draw(data: &EntityTable, entry: &BlockEntityRef) -> Vec<EntityVertex> {
        let lids = ChestLids::default();
        let scene = BlockEntityScene {
            data,
            seconds: 1.0,
            lids: &lids,
            signs: &Default::default(),
            skins: &crate::entity_render::NoSkins,
            font: None,
            partial_tick: 0.0,
            camera: glam::Vec3::ZERO,
            origin: [0; 3],
        };
        let mut batches = HashMap::new();
        add_beam(&mut batches, entry, &scene);
        batches.remove(&TextureSlot::BeaconBeam).unwrap_or_default()
    }

    fn table(action: u8, nbt: Option<Vec<u8>>) -> EntityTable {
        EntityTable::from([((4, 70, 9), BlockEntityData { action, nbt })])
    }

    #[test]
    fn a_complete_beacon_draws_eight_faces_up_to_the_build_limit() {
        let vertices = draw(&table(3, Some(levels_nbt(4))), &beacon(15));
        assert_eq!(vertices.len(), 8 * 4);
        let top = vertices.iter().map(|v| v.position[1]).fold(0.0, f32::max);
        let bottom = vertices
            .iter()
            .map(|v| v.position[1])
            .fold(f32::MAX, f32::min);
        assert_eq!((bottom, top), (70.0, 256.0));
    }

    #[test]
    fn the_prisms_have_the_reference_radii() {
        let vertices = beam_vertices([0, 0, 0], 10.0, 7.0);
        for vertex in &vertices[..16] {
            let [x, _, z] = vertex.position;
            let radius = f64::from(x - 0.5).hypot(f64::from(z - 0.5));
            assert!((radius - 0.2).abs() < 1e-5, "{radius}");
        }
        for vertex in &vertices[16..] {
            let [x, _, z] = vertex.position;
            assert!((0.2..=0.8).contains(&x) && (0.2..=0.8).contains(&z));
        }
        // The outer prism is added at the alpha of the reference.
        assert_eq!(vertices[16].light & 0xFF, 32);
    }

    #[test]
    fn the_texture_scrolls_with_time_and_repeats_with_the_height() {
        let still = beam_vertices([0, 0, 0], 10.0, 0.0);
        let later = beam_vertices([0, 0, 0], 10.0, 2.5);
        assert_ne!(still[0].uv[1], later[0].uv[1]);
        // The inner prism repeats every 2.5 rows per block, the outer one once.
        assert!((still[0].uv[1] - still[1].uv[1] - 25.0).abs() < 1e-4);
        assert!((still[16].uv[1] - still[17].uv[1] - 10.0).abs() < 1e-4);
    }

    #[test]
    fn no_beam_without_levels_data_sky_or_valid_nbt() {
        assert!(draw(&table(3, Some(levels_nbt(0))), &beacon(15)).is_empty());
        assert!(draw(&EntityTable::new(), &beacon(15)).is_empty());
        assert!(draw(&table(3, Some(levels_nbt(4))), &beacon(14)).is_empty());
        assert!(draw(&table(1, Some(levels_nbt(4))), &beacon(15)).is_empty());
        assert!(draw(&table(3, None), &beacon(15)).is_empty());
        assert!(draw(&table(3, Some(vec![10, 0, 0, 3, 0, 9])), &beacon(15)).is_empty());
    }
}

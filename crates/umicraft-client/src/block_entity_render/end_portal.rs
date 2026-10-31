// SPDX-License-Identifier: MIT OR Apache-2.0

//! The surface of an end portal (MCP `TileEntityEndPortalRenderer`).
//!
//! The reference draws sixteen copies of the top face of the block at
//! `y + 0.75`. Layer 0 shows `environment/end_sky.png`, the others
//! `entity/end_portal.png`, each tinted by a colour ramp from a fixed random
//! seed. Their texture coordinates are not stored: OpenGL generates them per
//! vertex (`texGen`) so that every layer looks like a window onto a plane
//! `L` blocks below the surface, which parallaxes as the camera moves, and the
//! texture matrix then scales, turns and scrolls them.
//!
//! The generator and the matrix collapse, for a vertex `P` and the camera `E`
//! (both horizontal), at height `h` of the camera over the surface, to
//!
//! ```text
//! g  = -E - (h + L) / h * (P - E)
//! uv = scale * (rotate(angle) * (g - 0.5) + 0.5) + (0, scroll)
//! ```
//!
//! with `L = 16 - i` (65 for layer 0), `scale` 0.125, 0.5 or 0.0625,
//! `angle = 2 * (4321 i^2 + 9 i)` degrees and `scroll` cycling every 700 s
//! (derived from the `texGen` planes, the `q` division and the matrix stack of
//! the reference).
//!
//! Differences:
//!
//! - every layer is added to the frame. The reference replaces the frame with
//!   layer 0 (`blendFunc(SRC_ALPHA, ONE_MINUS_SRC_ALPHA)` at alpha 1), so the
//!   world below the frame shows through here, brightened by the sky layer;
//! - the faces are drawn only from above (the reference culls the back);
//! - the offset of the camera from the player's eye (`ActiveRenderInfo`, a few
//!   centimetres) is ignored, and the scroll follows the client clock instead
//!   of the system clock.

use std::collections::HashMap;

use umicraft_mesh::BlockEntityRef;
use umicraft_model::entity::TextureSlot;
use umicraft_render::EntityVertex;

use super::additive::{tint_word, vertex};
use super::BlockEntityScene;

/// Number of stacked layers.
const LAYERS: u32 = 16;
/// Height of the surface in the block (`BlockEndPortal` bounds are 0..0.75).
const SURFACE: f32 = 0.75;
/// The seed of the colour ramp (`field_147527_e`).
const COLOUR_SEED: u64 = 31_100;
/// Milliseconds after which the scroll wraps.
const SCROLL_PERIOD_MS: f64 = 700_000.0;
/// Smallest camera height over the surface used in the division, in blocks.
const MIN_HEIGHT: f64 = 1e-3;

/// The sixteen layers of the portal at `entry`.
pub(super) fn add_end_portal(
    batches: &mut HashMap<TextureSlot, Vec<EntityVertex>>,
    entry: &BlockEntityRef,
    scene: &BlockEntityScene<'_>,
) {
    let [x, y, z] = entry.position;
    let height = f64::from(scene.camera.y) - (f64::from(y) + f64::from(SURFACE));
    // Seen from below the faces are culled in the reference.
    if height <= 0.0 {
        return;
    }
    let camera = [f64::from(scene.camera.x), f64::from(scene.camera.z)];
    let scroll = (f64::from(scene.seconds) * 1000.0 % SCROLL_PERIOD_MS) / SCROLL_PERIOD_MS;
    let mut random = JavaRandom::new(COLOUR_SEED);
    for layer in 0..LAYERS {
        let tint = layer_tint(layer, &mut random);
        let slot = if layer == 0 {
            TextureSlot::EndSky
        } else {
            TextureSlot::EndPortal
        };
        let uv = |corner: [f64; 2]| layer_uv(layer, corner, camera, height, scroll);
        let (x, z) = (f64::from(x), f64::from(z));
        let corners = [[x, z], [x, z + 1.0], [x + 1.0, z + 1.0], [x + 1.0, z]];
        let quad = layer_quad(corners, f64::from(y) + f64::from(SURFACE), tint, uv);
        batches.entry(slot).or_default().extend(quad);
    }
}

/// The corners of one layer at height `top`.
fn layer_quad(
    corners: [[f64; 2]; 4],
    top: f64,
    tint: u32,
    uv: impl Fn([f64; 2]) -> [f32; 2],
) -> [EntityVertex; 4] {
    // The integer part of the first corner's coordinates is removed from all
    // four: the texture repeats, and it keeps the f32 precise far from the
    // origin.
    let base = uv(corners[0]).map(f32::floor);
    corners.map(|corner| {
        let [u, v] = uv(corner);
        vertex(
            [corner[0] as f32, top as f32, corner[1] as f32],
            [u - base[0], v - base[1]],
            tint,
        )
    })
}

/// The colour a layer adds. `random` must be asked for the layers in order
/// (the reference consumes three numbers per layer, the sky included).
fn layer_tint(layer: u32, random: &mut JavaRandom) -> u32 {
    let fade = layer_fade(layer);
    let rgb = [
        (random.next_float() * 0.5 + 0.1) * fade,
        (random.next_float() * 0.5 + 0.4) * fade,
        (random.next_float() * 0.5 + 0.5) * fade,
    ];
    tint_word(if layer == 0 { [fade; 3] } else { rgb })
}

/// The brightness factor `f6` of a layer: the sky is dim, the others fade
/// towards the deepest.
fn layer_fade(layer: u32) -> f32 {
    if layer == 0 {
        0.1
    } else {
        1.0 / ((LAYERS - layer) as f32 + 1.0)
    }
}

/// The depth `L` of the plane a layer looks at (`f4`).
fn layer_depth(layer: u32) -> f64 {
    if layer == 0 {
        65.0
    } else {
        f64::from(LAYERS - layer)
    }
}

/// The scale of the texture of a layer (`f5`).
fn layer_scale(layer: u32) -> f64 {
    match layer {
        0 => 0.125,
        1 => 0.5,
        _ => 0.0625,
    }
}

/// The turn of a layer in degrees, reduced to a circle.
fn layer_degrees(layer: u32) -> f64 {
    f64::from((layer * layer * 4321 + layer * 9) * 2 % 360)
}

/// The texture coordinate of the surface point `corner` (`[x, z]`) for a
/// camera at horizontal `camera`, `height` blocks above the surface.
fn layer_uv(layer: u32, corner: [f64; 2], camera: [f64; 2], height: f64, scroll: f64) -> [f32; 2] {
    let height = height.max(MIN_HEIGHT);
    let depth = layer_depth(layer);
    let zoom = (height + depth) / height;
    let generated = [0, 1].map(|axis| -camera[axis] - zoom * (corner[axis] - camera[axis]));
    let (sin, cos) = layer_degrees(layer).to_radians().sin_cos();
    let (u, v) = (generated[0] - 0.5, generated[1] - 0.5);
    let scale = layer_scale(layer);
    [
        (scale * (u * cos - v * sin + 0.5)) as f32,
        (scale * (u * sin + v * cos + 0.5) + scroll) as f32,
    ]
}

/// `java.util.Random`, for the colour ramp of the reference.
struct JavaRandom(u64);

impl JavaRandom {
    const MULTIPLIER: u64 = 0x5_DEEC_E66D;
    const MASK: u64 = (1 << 48) - 1;

    fn new(seed: u64) -> Self {
        Self((seed ^ Self::MULTIPLIER) & Self::MASK)
    }

    /// `nextFloat`: 24 random bits over 2^24.
    fn next_float(&mut self) -> f32 {
        self.0 = (self.0.wrapping_mul(Self::MULTIPLIER).wrapping_add(0xB)) & Self::MASK;
        (self.0 >> 24) as f32 / (1u32 << 24) as f32
    }
}

#[cfg(test)]
mod tests {
    use umicraft_game::server::block_entities::EntityTable;
    use umicraft_game::server::chest_lids::ChestLids;
    use umicraft_mesh::SameNeighbours;
    use umicraft_world::{BlockState, LightLevel};

    use super::*;

    fn portal() -> BlockEntityRef {
        BlockEntityRef {
            position: [4, 30, 9],
            state: BlockState::new(119, 0),
            same_neighbour: SameNeighbours::default(),
            light: LightLevel::default(),
        }
    }

    fn draw(camera: glam::Vec3, seconds: f32) -> HashMap<TextureSlot, Vec<EntityVertex>> {
        let lids = ChestLids::default();
        let scene = BlockEntityScene {
            data: &EntityTable::new(),
            seconds,
            lids: &lids,
            signs: &Default::default(),
            skins: &crate::entity_render::NoSkins,
            font: None,
            partial_tick: 0.0,
            camera,
        };
        let mut batches = HashMap::new();
        add_end_portal(&mut batches, &portal(), &scene);
        batches
    }

    #[test]
    fn sixteen_quads_at_three_quarters_of_the_block() {
        let batches = draw(glam::Vec3::new(4.5, 33.0, 9.5), 0.0);
        assert_eq!(batches[&TextureSlot::EndSky].len(), 4);
        assert_eq!(batches[&TextureSlot::EndPortal].len(), 15 * 4);
        for vertex in batches.values().flatten() {
            assert_eq!(vertex.position[1], 30.75);
            assert!((4.0..=5.0).contains(&vertex.position[0]));
            assert!((9.0..=10.0).contains(&vertex.position[2]));
        }
    }

    #[test]
    fn nothing_is_drawn_from_below_the_surface() {
        assert!(draw(glam::Vec3::new(4.5, 30.5, 9.5), 0.0).is_empty());
    }

    #[test]
    fn the_coordinates_are_deterministic_and_follow_the_camera() {
        let here = glam::Vec3::new(4.5, 33.0, 9.5);
        let a = draw(here, 3.0);
        let b = draw(here, 3.0);
        assert_eq!(
            a[&TextureSlot::EndPortal][5].uv,
            b[&TextureSlot::EndPortal][5].uv
        );
        let moved = draw(here + glam::Vec3::X, 3.0);
        assert_ne!(
            a[&TextureSlot::EndPortal][5].uv,
            moved[&TextureSlot::EndPortal][5].uv
        );
    }

    #[test]
    fn a_layer_follows_the_texgen_planes_of_the_reference() {
        // Camera straight above the corner: the point under it does not move
        // with the depth, only the generated plane is flipped by the sign.
        let uv = layer_uv(1, [10.0, 20.0], [10.0, 20.0], 3.0, 0.0);
        let g = [-10.0_f64, -20.0];
        let (sin, cos) = layer_degrees(1).to_radians().sin_cos();
        let (u, v) = (g[0] - 0.5, g[1] - 0.5);
        let expected = [
            0.5 * (u * cos - v * sin + 0.5),
            0.5 * (u * sin + v * cos + 0.5),
        ];
        assert!((f64::from(uv[0]) - expected[0]).abs() < 1e-4);
        assert!((f64::from(uv[1]) - expected[1]).abs() < 1e-4);
    }

    #[test]
    fn the_scroll_moves_only_v() {
        let a = layer_uv(3, [1.0, 2.0], [0.0, 0.0], 2.0, 0.0);
        let b = layer_uv(3, [1.0, 2.0], [0.0, 0.0], 2.0, 0.25);
        assert_eq!(a[0], b[0]);
        assert!((b[1] - a[1] - 0.25).abs() < 1e-6);
    }

    #[test]
    fn the_java_random_matches_the_known_first_float() {
        // new Random(0).nextFloat() == 0.73096776
        let first = JavaRandom::new(0).next_float();
        assert!((first - 0.730_967_76).abs() < 1e-7, "{first}");
    }

    #[test]
    fn the_sky_layer_is_dim_and_white_and_the_others_fade() {
        let mut random = JavaRandom::new(COLOUR_SEED);
        let sky = layer_tint(0, &mut random);
        assert_eq!(sky & 0xFF, sky >> 8 & 0xFF);
        assert_eq!(sky & 0xFF, 26);
        assert_eq!(layer_fade(1), 1.0 / 16.0);
        assert_eq!(layer_fade(15), 0.5);
    }
}

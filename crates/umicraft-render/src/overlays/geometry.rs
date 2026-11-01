// SPDX-License-Identifier: MIT OR Apache-2.0

//! The quads of the overlays, built like the reference builds them.
//!
//! Sources: MCP `ItemRenderer.renderOverlays` (in-wall, water, fire) and
//! `GuiIngame.renderGameOverlay` (vignette, pumpkin, portal). The first three
//! are quads in front of the eye at `z = -0.5` seen through the projection of
//! the hand; the others are full-screen rectangles.

use std::ops::Range;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

use super::data::{Portal, ScreenOverlays, Underwater};

/// One vertex of the overlay pass.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(super) struct ScreenVertex {
    /// Position in clip space.
    pub position: [f32; 2],
    /// Texture coordinate; the water picture goes past `0..1` and repeats.
    pub uv: [f32; 2],
    /// A [`Source`] code.
    pub source: f32,
    /// `1.0` when the texture's alpha is ignored (the in-wall quad is drawn
    /// without blending), else `0.0`.
    pub opaque: f32,
    /// Factor applied to the texel (`GlStateManager.color`).
    pub color: [f32; 4],
}

impl ScreenVertex {
    /// Vertex attributes (locations 0..4).
    pub(super) const ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
        0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32x4
    ];
}

/// The picture a quad samples.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Source {
    /// A layer of the block atlas.
    Atlas(u32),
    /// `misc/underwater.png`.
    Underwater,
    /// `misc/pumpkinblur.png`.
    Pumpkin,
    /// `misc/vignette.png`.
    Vignette,
}

impl Source {
    /// The code the shader reads: the layer, or a negative picture number.
    fn code(self) -> f32 {
        match self {
            Self::Atlas(layer) => layer as f32,
            Self::Underwater => -1.0,
            Self::Pumpkin => -2.0,
            Self::Vignette => -3.0,
        }
    }
}

/// How a run of vertices is blended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Blend {
    /// `tryBlendFuncSeparate(770, 771, 1, 0)`.
    Alpha,
    /// `tryBlendFuncSeparate(0, 769, 1, 0)`: the screen is multiplied by
    /// `1 - texel * colour`.
    Vignette,
}

/// A run of vertices drawn with one blend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Draw {
    /// The blend of the run.
    pub blend: Blend,
    /// The vertices of the run.
    pub range: Range<u32>,
}

/// The vertices and the runs of one frame, reused between frames.
#[derive(Debug, Default)]
pub(super) struct Batch {
    /// All the vertices, in drawing order.
    pub vertices: Vec<ScreenVertex>,
    /// The runs, in drawing order.
    pub draws: Vec<Draw>,
}

/// The most vertices a frame needs: seven quads of six.
pub(super) const MAX_VERTICES: usize = 7 * 6;

/// The depth the quads of the eye sit at, in front of it.
const QUAD_DEPTH: f32 = -0.5;
/// The tint of the in-wall quad (`GlStateManager.color(0.1, 0.1, 0.1, 0.5)`).
const WALL_SHADE: f32 = 0.1;
/// Opacity of the underwater picture.
const WATER_ALPHA: f32 = 0.5;
/// Opacity of the flames.
const FIRE_ALPHA: f32 = 0.9;
/// Degrees of yaw or pitch that scroll the water picture by one texture.
const WATER_SCROLL_DEGREES: f32 = 64.0;
/// How often the water picture repeats over the screen.
const WATER_REPEAT: f32 = 4.0;

/// The corners of a quad, bottom-left first and counter-clockwise.
const UNIT_CORNERS: [(f32, f32); 4] = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
/// The texture corners of the quads in front of the eye: u runs from its
/// maximum to its minimum (`func_178108_a`, `renderFireInFirstPerson`).
const MIRRORED_UVS: [[f32; 2]; 4] = [[1.0, 1.0], [0.0, 1.0], [0.0, 0.0], [1.0, 0.0]];
/// The texture corners of the full-screen rectangles (`GuiIngame`).
const UPRIGHT_UVS: [[f32; 2]; 4] = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];

impl Batch {
    /// Rebuilds the batch for `overlays`. `hand_projection` is the projection
    /// the reference has set when it draws the overlays (`renderHand`).
    pub(super) fn rebuild(&mut self, overlays: &ScreenOverlays, hand_projection: Mat4) {
        self.vertices.clear();
        self.draws.clear();
        if let Some(layer) = overlays.in_wall {
            self.in_wall(hand_projection, layer);
        }
        if let Some(water) = overlays.underwater {
            self.underwater(hand_projection, water);
        }
        if let Some(layer) = overlays.fire {
            self.flames(hand_projection, layer);
        }
        if let Some([red, green, blue]) = overlays.vignette {
            self.screen_quad(Blend::Vignette, Source::Vignette, [red, green, blue, 1.0]);
        }
        if overlays.pumpkin {
            self.screen_quad(Blend::Alpha, Source::Pumpkin, [1.0; 4]);
        }
        if let Some(Portal { layer, alpha }) = overlays.portal {
            self.screen_quad(Blend::Alpha, Source::Atlas(layer), [1.0, 1.0, 1.0, alpha]);
        }
    }

    /// The block sprite over the whole screen (`func_178108_a`), drawn without
    /// blending, so the sprite's alpha is ignored.
    fn in_wall(&mut self, projection: Mat4, layer: u32) {
        let color = [WALL_SHADE, WALL_SHADE, WALL_SHADE, 1.0];
        self.quad(Blend::Alpha, Source::Atlas(layer), true, color, |index| {
            let (x, y) = UNIT_CORNERS[index];
            (
                project(projection, Mat4::IDENTITY, x, y),
                MIRRORED_UVS[index],
            )
        });
    }

    /// The water picture, scrolled by where the player looks
    /// (`renderWaterOverlayTexture`).
    fn underwater(&mut self, projection: Mat4, water: Underwater) {
        let scroll_u = -water.yaw_degrees / WATER_SCROLL_DEGREES;
        let scroll_v = water.pitch_degrees / WATER_SCROLL_DEGREES;
        let shade = water.brightness;
        let color = [shade, shade, shade, WATER_ALPHA];
        self.quad(Blend::Alpha, Source::Underwater, false, color, |index| {
            let (x, y) = UNIT_CORNERS[index];
            let [u, v] = MIRRORED_UVS[index];
            (
                project(projection, Mat4::IDENTITY, x, y),
                [u * WATER_REPEAT + scroll_u, v * WATER_REPEAT + scroll_v],
            )
        });
    }

    /// The two flames at the lower corners (`renderFireInFirstPerson`): unit
    /// squares moved 0.24 sideways and 0.3 down, turned 10 degrees towards
    /// each other.
    fn flames(&mut self, projection: Mat4, layer: u32) {
        let color = [1.0, 1.0, 1.0, FIRE_ALPHA];
        for side in [-1.0_f32, 1.0] {
            let model = Mat4::from_translation(Vec3::new(-side * 0.24, -0.3, 0.0))
                * Mat4::from_rotation_y((side * 10.0).to_radians());
            self.quad(Blend::Alpha, Source::Atlas(layer), false, color, |index| {
                let (x, y) = UNIT_CORNERS[index];
                (
                    project(projection, model, x * 0.5, y * 0.5),
                    MIRRORED_UVS[index],
                )
            });
        }
    }

    /// A rectangle over the whole screen with an upright texture.
    fn screen_quad(&mut self, blend: Blend, source: Source, color: [f32; 4]) {
        self.quad(blend, source, false, color, |index| {
            let (x, y) = UNIT_CORNERS[index];
            ([x, y], UPRIGHT_UVS[index])
        });
    }

    /// Appends the two triangles of a quad whose corner `index` (0..4) is
    /// `corner(index)`, merging the run with the previous one of the same
    /// blend.
    fn quad(
        &mut self,
        blend: Blend,
        source: Source,
        opaque: bool,
        color: [f32; 4],
        corner: impl Fn(usize) -> ([f32; 2], [f32; 2]),
    ) {
        let first = self.vertices.len() as u32;
        for index in [0, 1, 2, 0, 2, 3] {
            let (position, uv) = corner(index);
            self.vertices.push(ScreenVertex {
                position,
                uv,
                source: source.code(),
                opaque: f32::from(u8::from(opaque)),
                color,
            });
        }
        let end = self.vertices.len() as u32;
        match self.draws.last_mut() {
            Some(last) if last.blend == blend => last.range.end = end,
            _ => self.draws.push(Draw {
                blend,
                range: first..end,
            }),
        }
    }
}

/// The clip-space position of the point `(x, y, -0.5)` moved by `model`.
fn project(projection: Mat4, model: Mat4, x: f32, y: f32) -> [f32; 2] {
    let point = projection.project_point3(model.transform_point3(Vec3::new(x, y, QUAD_DEPTH)));
    [point.x, point.y]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The projection of a 70 degree hand on a square screen.
    fn projection() -> Mat4 {
        glam::camera::rh::proj::directx::perspective(70f32.to_radians(), 1.0, 0.05, 100.0)
    }

    fn batch(overlays: ScreenOverlays) -> Batch {
        let mut batch = Batch::default();
        batch.rebuild(&overlays, projection());
        batch
    }

    #[test]
    fn nothing_to_show_builds_nothing() {
        let batch = batch(ScreenOverlays::default());
        assert!(batch.vertices.is_empty() && batch.draws.is_empty());
    }

    #[test]
    fn the_biggest_frame_fits_the_buffer() {
        let batch = batch(ScreenOverlays {
            in_wall: Some(1),
            underwater: Some(Underwater {
                brightness: 1.0,
                yaw_degrees: 0.0,
                pitch_degrees: 0.0,
            }),
            fire: Some(2),
            pumpkin: true,
            portal: Some(Portal {
                layer: 3,
                alpha: 1.0,
            }),
            vignette: Some([1.0; 3]),
        });
        assert_eq!(batch.vertices.len(), MAX_VERTICES);
    }

    #[test]
    fn a_quad_half_a_block_ahead_covers_the_screen() {
        // At 0.5 in front of a 70 degree eye the unit half-extent reaches past
        // the edge of the view, so the in-wall sprite fills every pixel.
        let batch = batch(ScreenOverlays {
            in_wall: Some(7),
            ..ScreenOverlays::default()
        });
        assert!(batch
            .vertices
            .iter()
            .all(|v| v.position[0].abs() > 1.0 && v.position[1].abs() > 1.0));
        assert!(batch.vertices.iter().all(|v| v.opaque == 1.0));
        assert_eq!(batch.vertices[0].source, 7.0);
        assert_eq!(batch.vertices[0].color, [0.1, 0.1, 0.1, 1.0]);
    }

    #[test]
    fn the_water_scrolls_with_the_look_and_repeats_four_times() {
        let water = |yaw, pitch| Underwater {
            brightness: 0.8,
            yaw_degrees: yaw,
            pitch_degrees: pitch,
        };
        let still = batch(ScreenOverlays {
            underwater: Some(water(0.0, 0.0)),
            ..ScreenOverlays::default()
        });
        let turned = batch(ScreenOverlays {
            underwater: Some(water(32.0, 64.0)),
            ..ScreenOverlays::default()
        });
        assert_eq!(still.vertices[0].uv, [4.0, 4.0]);
        assert_eq!(still.vertices[0].color, [0.8, 0.8, 0.8, 0.5]);
        assert_eq!(turned.vertices[0].uv, [3.5, 5.0]);
    }

    #[test]
    fn the_flames_are_two_quads_on_either_side_of_the_middle() {
        let batch = batch(ScreenOverlays {
            fire: Some(5),
            ..ScreenOverlays::default()
        });
        assert_eq!(batch.vertices.len(), 12);
        let centre = |quad: &[ScreenVertex]| quad.iter().map(|v| v.position[0]).sum::<f32>() / 6.0;
        let (first, second) = batch.vertices.split_at(6);
        assert!(centre(first) > 0.0 && centre(second) < 0.0);
        // Both sit below the middle of the screen.
        assert!(batch.vertices.iter().map(|v| v.position[1]).sum::<f32>() < 0.0);
        assert!(batch.vertices.iter().all(|v| v.color[3] == 0.9));
    }

    #[test]
    fn the_screen_overlays_run_in_the_order_of_the_reference() {
        let batch = batch(ScreenOverlays {
            pumpkin: true,
            vignette: Some([0.5, 0.5, 0.5]),
            portal: Some(Portal {
                layer: 4,
                alpha: 0.3,
            }),
            ..ScreenOverlays::default()
        });
        let sources: Vec<f32> = batch.vertices.iter().step_by(6).map(|v| v.source).collect();
        assert_eq!(sources, [-3.0, -2.0, 4.0]);
        // The vignette multiplies, the other two blend: two runs.
        let blends: Vec<Blend> = batch.draws.iter().map(|d| d.blend).collect();
        assert_eq!(blends, [Blend::Vignette, Blend::Alpha]);
        assert_eq!(batch.draws[1].range, 6..18);
    }

    #[test]
    fn a_rebuild_replaces_the_previous_frame() {
        let mut batch = batch(ScreenOverlays {
            pumpkin: true,
            ..ScreenOverlays::default()
        });
        batch.rebuild(&ScreenOverlays::default(), projection());
        assert!(batch.vertices.is_empty() && batch.draws.is_empty());
    }
}

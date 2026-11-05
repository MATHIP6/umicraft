// SPDX-License-Identifier: MIT OR Apache-2.0

//! Smooth lighting and ambient occlusion sampled around a face (ADR-0008).

use umicraft_world::LightLevel;

use crate::{BlockSource, VertexLight};

/// How the light of a quad is sampled: the `Smooth Lighting` video option.
///
/// 1.8.8 only tests `ambientOcclusion != 0` (`Minecraft.isAmbientOcclusionEnabled`),
/// so the options "Minimum" and "Maximum" mesh identically and map to
/// [`Self::Smooth`]; "Off" is [`Self::Flat`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lighting {
    /// Per-corner light and ambient occlusion (`renderModelAmbientOcclusion`).
    #[default]
    Smooth,
    /// One light per quad, sampled outside the face (`renderModelStandard`).
    Flat,
}

impl Lighting {
    /// The lighting of the `Smooth Lighting` option value (`0` off, `1` and
    /// `2` on).
    #[must_use]
    pub const fn from_option(level: u8) -> Self {
        if level == 0 {
            Self::Flat
        } else {
            Self::Smooth
        }
    }
}

/// Axis of a face normal.
#[derive(Debug, Clone, Copy)]
enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    /// Component of a block-local position on this axis.
    fn component(self, position: [f32; 3]) -> f32 {
        match self {
            Self::X => position[0],
            Self::Y => position[1],
            Self::Z => position[2],
        }
    }

    /// Child cell one step along this axis.
    fn offset(self, position: [i32; 3], delta: i32) -> [i32; 3] {
        let mut offset = position;
        match self {
            Self::X => offset[0] += delta,
            Self::Y => offset[1] += delta,
            Self::Z => offset[2] += delta,
        }
        offset
    }
}

/// Outward plane of one face: the dominant normal axis plus the two tangent
/// axes the corner cells live in.
#[derive(Debug, Clone, Copy)]
struct FaceFrame {
    /// Outward axis of the face normal.
    axis: Axis,
    /// `+1` when the normal points along the positive axis, else `-1`.
    sign: i32,
    /// First tangent axis.
    u: Axis,
    /// Second tangent axis.
    v: Axis,
}

impl FaceFrame {
    /// Snaps a quad normal to the face frame.
    ///
    /// Legacy cubes and unrotated model elements have axis normals. A model
    /// element rotated by 22.5°/45° (for example a cross) has a diagonal
    /// normal; the dominant component is the face the 1.8 baker would have
    /// assigned, which keeps the sample plane stable and allocation-free.
    fn from_normal(normal: [f32; 3]) -> Self {
        let [x, y, z] = normal.map(f32::abs);
        let (axis, value) = if x >= y && x >= z {
            (Axis::X, normal[0])
        } else if y >= z {
            (Axis::Y, normal[1])
        } else {
            (Axis::Z, normal[2])
        };
        let sign = if value < 0.0 { -1 } else { 1 };
        let (u, v) = match axis {
            Axis::X => (Axis::Y, Axis::Z),
            Axis::Y => (Axis::X, Axis::Z),
            Axis::Z => (Axis::X, Axis::Y),
        };
        Self { axis, sign, u, v }
    }
}

/// Index in the 3×3 tangent-plane grid; `du` and `dv` are `-1..=1`.
const fn grid_index(du: i32, dv: i32) -> usize {
    ((dv + 1) * 3 + (du + 1)) as usize
}

/// The nine cells on the outward side of one face, sampled once.
///
/// A full-cube face has four corners whose four-cell light footprints cover
/// exactly this 3×3 grid (plus the cell directly outward), so sampling it once
/// costs nine light and eight occlusion lookups per emitted face instead of
/// sixteen light and twelve block lookups.
struct FaceSamples {
    light: [LightLevel; 9],
    opaque: [bool; 9],
}

impl FaceSamples {
    /// Samples the outward plane of the face on `block`.
    ///
    /// The occupancy lookups are skipped when AO is off for this quad, so a
    /// cross or plant costs only the nine light samples.
    fn new<S: BlockSource + ?Sized>(
        source: &S,
        block: [i32; 3],
        frame: FaceFrame,
        ao_enabled: bool,
    ) -> Self {
        let base = frame.axis.offset(block, frame.sign);
        let mut samples = Self {
            light: [LightLevel::DARK; 9],
            opaque: [false; 9],
        };
        for dv in -1..=1 {
            for du in -1..=1 {
                let cell = frame.v.offset(frame.u.offset(base, du), dv);
                let index = grid_index(du, dv);
                samples.light[index] = source.light(cell[0], cell[1], cell[2]);
                // AO only ever reads the edge and diagonal cells; the centre
                // is needed for smooth light but never for occlusion.
                if ao_enabled && (du != 0 || dv != 0) {
                    samples.opaque[index] = source.block(cell[0], cell[1], cell[2]).is_opaque();
                }
            }
        }
        samples
    }
}

/// Samples smooth light and corner AO for the four corners of one quad.
///
/// `block` is the world position of the emitting block and `corners` are its
/// block-local corner positions. For each corner the tangent direction is the
/// side of the block centre the corner lies on; the four cells around it are
/// the outward cell, the two edge neighbours and the diagonal. Block and sky
/// light are averaged separately and rounded to the nearest nibble.
///
/// AO uses the 1.8 `AmbientOcclusionFace` layout: the two edge neighbours and
/// the diagonal on the outward side. Both edges occluding means three
/// occluders even when the diagonal is open, which is the darkest corner.
/// `ambient_occlusion == false` (model flag) or `shade == false` (cross and
/// plant quads) keeps the corner open at [`VertexLight::AO_OPEN`]; the shade
/// flag is still carried to the shader (ADR-0008 §2).
///
/// The returned array lives on the stack; the sampler performs no allocation.
///
/// When `ambient_occlusion` is `false` (cross and plant models) or `lighting`
/// is [`Lighting::Flat`], the function returns flat lighting: all four corners
/// share one light. This matches `renderModelStandardQuads` in the 1.8
/// reference, which uses `getMixedBrightnessForBlock` uniformly: the cell
/// outside the face for a quad on the cube boundary, the block's own cell
/// otherwise. Without the own cell, cross-model faces pick up dark samples
/// from the ground block below and show a visible shadow at their base.
pub(crate) fn quad_light<S: BlockSource + ?Sized>(
    source: &S,
    block: [i32; 3],
    normal: [f32; 3],
    corners: &[[f32; 3]; 4],
    ambient_occlusion: bool,
    shade: bool,
    lighting: Lighting,
) -> [VertexLight; 4] {
    let frame = FaceFrame::from_normal(normal);
    if !ambient_occlusion || lighting == Lighting::Flat {
        let cell = if lies_on_face_plane(frame, corners) {
            frame.axis.offset(block, frame.sign)
        } else {
            block
        };
        let light = source.light(cell[0], cell[1], cell[2]);
        let flat = VertexLight::new(light.block, light.sky, VertexLight::AO_OPEN, shade);
        return [flat; 4];
    }

    let ao_enabled = shade;
    let samples = FaceSamples::new(source, block, frame, ao_enabled);

    std::array::from_fn(|index| {
        let corner = corners[index];
        let du = if frame.u.component(corner) >= 0.5 {
            1
        } else {
            -1
        };
        let dv = if frame.v.component(corner) >= 0.5 {
            1
        } else {
            -1
        };

        let base = samples.light[grid_index(0, 0)];
        let edge_u_light = or_base(samples.light[grid_index(du, 0)], base);
        let edge_v_light = or_base(samples.light[grid_index(0, dv)], base);
        // `renderModelAmbientOcclusion`: when both edge neighbours are solid
        // the diagonal is hidden behind them and reuses an edge sample.
        let diagonal_light =
            if samples.opaque[grid_index(du, 0)] && samples.opaque[grid_index(0, dv)] {
                edge_u_light
            } else {
                or_base(samples.light[grid_index(du, dv)], base)
            };
        let block_light = average_nibble([
            base.block,
            edge_u_light.block,
            edge_v_light.block,
            diagonal_light.block,
        ]);
        let sky_light = average_nibble([
            base.sky,
            edge_u_light.sky,
            edge_v_light.sky,
            diagonal_light.sky,
        ]);

        let ao = if ao_enabled {
            let edge_u = samples.opaque[grid_index(du, 0)];
            let edge_v = samples.opaque[grid_index(0, dv)];
            let diagonal = samples.opaque[grid_index(du, dv)];
            let occlusion = if edge_u && edge_v {
                3
            } else {
                u8::from(edge_u) + u8::from(edge_v) + u8::from(diagonal)
            };
            15 - 5 * occlusion
        } else {
            VertexLight::AO_OPEN
        };

        VertexLight::new(block_light, sky_light, ao, shade)
    })
}

/// Whether every corner of the quad sits on the cube face the normal points
/// to (`fillQuadBounds` flag 0): such a quad is lit by the cell outside the
/// face, any other (a cross, an inset torch) by the block's own cell.
fn lies_on_face_plane(frame: FaceFrame, corners: &[[f32; 3]; 4]) -> bool {
    let first = frame.axis.component(corners[0]);
    let on_boundary = !(1.0e-4..=0.9999).contains(&first);
    on_boundary
        && corners
            .iter()
            .all(|corner| (frame.axis.component(*corner) - first).abs() < 1.0e-4)
}

/// `getAoBrightness`: a corner sample with no light at all (a solid block's
/// own cell, which the server stores as dark) takes the light of the face's
/// outward cell instead of pulling the corner toward black. Without this
/// every edge next to a solid block was a dark crease, much darker than the
/// reference.
fn or_base(sample: LightLevel, base: LightLevel) -> LightLevel {
    if sample == LightLevel::DARK {
        base
    } else {
        sample
    }
}

/// Rounds the average of four nibbles to the nearest integer.
fn average_nibble(values: [u8; 4]) -> u8 {
    let sum: u16 = values.iter().map(|value| u16::from(*value)).sum();
    ((sum + 2) / 4) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dark_sample_falls_back_to_the_outward_cell() {
        let base = LightLevel::new(0, 12);
        assert_eq!(or_base(LightLevel::DARK, base), base);
        assert_eq!(or_base(LightLevel::new(3, 0), base), LightLevel::new(3, 0));
    }

    #[test]
    fn a_diagonal_normal_snaps_to_its_dominant_axis() {
        // A cross element rotated by 45° has a diagonal normal; the sample
        // plane must still be a stable axis pair.
        let frame = FaceFrame::from_normal([0.707, 0.0, 0.707]);
        assert!(matches!(frame.axis, Axis::X));
        assert_eq!(frame.sign, 1);
        let frame = FaceFrame::from_normal([0.0, -0.4, 0.9]);
        assert!(matches!(frame.axis, Axis::Z));
        assert_eq!(frame.sign, 1);
        let frame = FaceFrame::from_normal([0.0, -1.0, 0.0]);
        assert!(matches!(frame.axis, Axis::Y));
        assert_eq!(frame.sign, -1);
    }
}

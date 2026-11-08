// SPDX-License-Identifier: MIT OR Apache-2.0

//! The quads of one frame of rain and snow, built like
//! `EntityRenderer.renderRainSnow`.
//!
//! Every column within 5 (fast) or 10 (fancy) blocks of the camera that has
//! precipitation gets one vertical quad, turned to face the camera by a
//! per-offset direction (`rainXCoords`, `rainYCoords`), from the precipitation
//! height (or `y - radius`) to `y + radius`. Rain scrolls down by a per-column
//! phase; snow drifts with a random wobble. The random draws are seeded by the
//! column, so each column keeps its look from frame to frame.
//!
//! Approximations: the temperature noise of `getFloatTemperature` is omitted
//! (at most 0.007 of temperature); the light is the one at the precipitation
//! height even when the camera is above it; the camera stands for the render
//! view entity.

use std::ops::Range;

use bytemuck::{Pod, Zeroable};
use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::field::{PrecipitationCell, PrecipitationField};

/// Snow below this temperature at the quad's bottom (`getTemperatureAtHeight >= 0.15` rains).
const SNOW_LINE: f32 = 0.15;
/// Blocks of precipitation radius with fast graphics.
pub(super) const FAST_RADIUS: i32 = 5;
/// Blocks of precipitation radius with fancy graphics.
pub(super) const FANCY_RADIUS: i32 = 10;
/// Vertices a frame can need at most: 6 per column of the fancy square.
pub(super) const MAX_VERTICES: usize = 6 * (2 * FANCY_RADIUS as usize + 1).pow(2);

/// One vertex of the weather pass.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(super) struct WeatherVertex {
    /// Position relative to the render origin.
    pub position: [f32; 3],
    /// Texture coordinate; the sampler repeats.
    pub uv: [f32; 2],
    /// Opacity multiplied into the texel (`color(1, 1, 1, alpha)`).
    pub alpha: f32,
    /// Sky and block light, each `0.0..=15.0`.
    pub light: [f32; 2],
}

impl WeatherVertex {
    /// Vertex attributes (locations 0..4).
    pub(super) const ATTRIBUTES: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32, 3 => Float32x2];
}

/// Which picture a run of vertices uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Precipitation {
    /// `rain.png`.
    Rain,
    /// `snow.png`.
    Snow,
}

/// A run of consecutive vertices drawn with one picture, in the order the
/// reference draws them (it switches the texture when the kind changes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Run {
    pub kind: Precipitation,
    pub vertices: Range<u32>,
}

/// The vertices of a frame and their runs.
#[derive(Debug, Default)]
pub(super) struct WeatherMesh {
    pub vertices: Vec<WeatherVertex>,
    pub runs: Vec<Run>,
}

impl WeatherMesh {
    fn clear(&mut self) {
        self.vertices.clear();
        self.runs.clear();
    }

    /// Appends a quad, opening a new run when the picture changes.
    fn push_quad(&mut self, kind: Precipitation, corners: [WeatherVertex; 4]) {
        let start = self.vertices.len() as u32;
        for index in [0, 1, 2, 0, 2, 3] {
            self.vertices.push(corners[index]);
        }
        let end = self.vertices.len() as u32;
        match self.runs.last_mut() {
            Some(run) if run.kind == kind => run.vertices.end = end,
            _ => self.runs.push(Run {
                kind,
                vertices: start..end,
            }),
        }
    }
}

/// What the frame knows about the viewer.
#[derive(Debug, Clone, Copy)]
pub(super) struct FrameInput {
    /// The camera position.
    pub eye: DVec3,
    /// The render origin the quads are made relative to, in f64 before the
    /// cast to f32 (F2-1b).
    pub origin: DVec3,
    /// `getRainStrength`, the opacity scale.
    pub strength: f32,
    /// Fancy graphics reach 10 blocks, fast ones 5.
    pub fancy: bool,
    /// The client tick count with its fraction (`rendererUpdateCount + partial`).
    pub ticks: f64,
}

/// The direction of the quad of the column at offset `dx`, `dz` from the
/// viewer's column (`rainXCoords`, `rainYCoords`, already halved); `None` for
/// the viewer's own column, where the reference divides zero by zero and
/// draws nothing.
fn quad_offset(dx: i32, dz: i32) -> Option<(f64, f64)> {
    if dx == 0 && dz == 0 {
        return None;
    }
    let (fx, fz) = (dx as f32, dz as f32);
    let length = (fx * fx + fz * fz).sqrt();
    Some((f64::from(-fz / length) * 0.5, f64::from(fx / length) * 0.5))
}

/// `BiomeGenBase.getFloatTemperature` without its noise term: it falls by
/// 0.05 per 30 blocks above y = 64.
pub(super) fn temperature_at(base: f32, y: i32) -> f32 {
    if y > 64 {
        base - (y - 64) as f32 * 0.05 / 30.0
    } else {
        base
    }
}

/// The per-column seed of the reference: Java `int` arithmetic, then widened.
fn column_hash(x: i32, z: i32) -> (i32, i32) {
    let x_term = x
        .wrapping_mul(x)
        .wrapping_mul(3121)
        .wrapping_add(x.wrapping_mul(45_238_971));
    let z_term = z
        .wrapping_mul(z)
        .wrapping_mul(418_711)
        .wrapping_add(z.wrapping_mul(13_761));
    (x_term, z_term)
}

/// Builds the quads of a frame into `mesh`, replacing its content.
pub(super) fn build(field: &PrecipitationField, input: &FrameInput, mesh: &mut WeatherMesh) {
    mesh.clear();
    if input.strength <= 0.0 || field.is_empty() {
        return;
    }
    let radius = if input.fancy {
        FANCY_RADIUS
    } else {
        FAST_RADIUS
    };
    let (column_x, column_z) = (input.eye.x.floor() as i32, input.eye.z.floor() as i32);
    let eye_y = input.eye.y.floor() as i32;
    let count = input.ticks.floor() as i32;
    let partial = input.ticks.fract() as f32;
    let wobble_clock = count as f32 + partial;
    for z in column_z - radius..=column_z + radius {
        for x in column_x - radius..=column_x + radius {
            let Some(cell) = field.get(x, z) else {
                continue;
            };
            let Some(offset) = quad_offset(x - column_x, z - column_z) else {
                continue;
            };
            let column = Column {
                x,
                z,
                bottom: (eye_y - radius).max(cell.height),
                top: (eye_y + radius).max(cell.height),
                offset,
                distance: column_distance(input.eye, x, z, radius),
                origin: input.origin,
            };
            if column.bottom == column.top {
                continue;
            }
            if temperature_at(cell.temperature, column.bottom) >= SNOW_LINE {
                push_rain(mesh, cell, &column, input, count, partial);
            } else {
                push_snow(mesh, cell, &column, input, (count, partial), wobble_clock);
            }
        }
    }
}

/// The geometry of one column of the grid.
struct Column {
    x: i32,
    z: i32,
    /// `k2`: the lower end of the quad.
    bottom: i32,
    /// `l2`: the upper end of the quad.
    top: i32,
    offset: (f64, f64),
    /// Distance to the viewer in units of the radius (`f3`).
    distance: f32,
    /// The render origin of the vertices.
    origin: DVec3,
}

fn column_distance(eye: DVec3, x: i32, z: i32, radius: i32) -> f32 {
    let dx = f64::from(x as f32 + 0.5) - eye.x;
    let dz = f64::from(z as f32 + 0.5) - eye.z;
    (dx * dx + dz * dz).sqrt() as f32 / radius as f32
}

/// The four corners of the quad: `(u, v)` pairs for the bottom and the top.
fn corners(
    column: &Column,
    alpha: f32,
    light: [f32; 2],
    uv: [(f64, f64); 4],
) -> [WeatherVertex; 4] {
    let (dx, dz) = column.offset;
    let (cx, cz) = (f64::from(column.x) + 0.5, f64::from(column.z) + 0.5);
    let (bottom, top) = (f64::from(column.bottom), f64::from(column.top));
    let at = |x: f64, y: f64, z: f64, uv: (f64, f64)| WeatherVertex {
        position: [
            (x - column.origin.x) as f32,
            (y - column.origin.y) as f32,
            (z - column.origin.z) as f32,
        ],
        uv: [uv.0 as f32, uv.1 as f32],
        alpha,
        light,
    };
    [
        at(cx - dx, bottom, cz - dz, uv[0]),
        at(cx + dx, bottom, cz + dz, uv[1]),
        at(cx + dx, top, cz + dz, uv[2]),
        at(cx - dx, top, cz - dz, uv[3]),
    ]
}

fn push_rain(
    mesh: &mut WeatherMesh,
    cell: &PrecipitationCell,
    column: &Column,
    input: &FrameInput,
    count: i32,
    partial: f32,
) {
    let (x_term, z_term) = column_hash(column.x, column.z);
    let mut random = JavaRandom::new(i64::from(x_term ^ z_term) as u64);
    let phase = f64::from(count.wrapping_add(x_term).wrapping_add(z_term) & 31);
    let scroll = (phase + f64::from(partial)) / 32.0 * (3.0 + random.next_double());
    let alpha = ((1.0 - column.distance * column.distance) * 0.5 + 0.5) * input.strength;
    let light = [f32::from(cell.sky_light), f32::from(cell.block_light)];
    let (low, high) = (
        f64::from(column.bottom) * 0.25,
        f64::from(column.top) * 0.25,
    );
    let uv = [
        (0.0, low + scroll),
        (1.0, low + scroll),
        (1.0, high + scroll),
        (0.0, high + scroll),
    ];
    mesh.push_quad(Precipitation::Rain, corners(column, alpha, light, uv));
}

fn push_snow(
    mesh: &mut WeatherMesh,
    cell: &PrecipitationCell,
    column: &Column,
    input: &FrameInput,
    (count, partial): (i32, f32),
    wobble_clock: f32,
) {
    let (x_term, z_term) = column_hash(column.x, column.z);
    let mut random = JavaRandom::new(i64::from(x_term ^ z_term) as u64);
    let fall = f64::from((count & 511) as f32 + partial) / 512.0;
    let drift_u = random.next_double()
        + f64::from(wobble_clock) * 0.01 * f64::from(random.next_gaussian() as f32);
    let drift_v =
        random.next_double() + f64::from(wobble_clock * random.next_gaussian() as f32) * 0.001;
    let alpha = ((1.0 - column.distance * column.distance) * 0.3 + 0.5) * input.strength;
    // `(light * 3 + 0xF000F0) / 4` per channel: snow is lit towards full light.
    let light = [
        (f32::from(cell.sky_light) * 3.0 + 15.0) / 4.0,
        (f32::from(cell.block_light) * 3.0 + 15.0) / 4.0,
    ];
    let (low, high) = (
        f64::from(column.bottom) * 0.25,
        f64::from(column.top) * 0.25,
    );
    let uv = [
        (drift_u, low + fall + drift_v),
        (1.0 + drift_u, low + fall + drift_v),
        (1.0 + drift_u, high + fall + drift_v),
        (drift_u, high + fall + drift_v),
    ];
    mesh.push_quad(Precipitation::Snow, corners(column, alpha, light, uv));
}

#[cfg(test)]
mod tests {
    use super::*;

    const EYE: DVec3 = DVec3::new(0.5, 70.0, 0.5);

    fn cell(temperature: f32) -> PrecipitationCell {
        PrecipitationCell {
            height: 64,
            temperature,
            sky_light: 15,
            block_light: 0,
        }
    }

    fn field(temperature: f32) -> PrecipitationField {
        let mut field = PrecipitationField::new(0, 0);
        for z in -12..=12 {
            for x in -12..=12 {
                field.set(x, z, cell(temperature));
            }
        }
        field
    }

    fn input(fancy: bool) -> FrameInput {
        FrameInput {
            eye: EYE,
            origin: DVec3::ZERO,
            strength: 1.0,
            fancy,
            ticks: 100.25,
        }
    }

    #[test]
    fn quads_are_relative_to_the_render_origin() {
        // F2-1b: far from the world origin an absolute f32 vertex has a 6 cm
        // grain; relative ones stay within the radius of the viewer.
        let mut field = PrecipitationField::new(1_000_000, 1_000_000);
        for z in 999_988..=1_000_012 {
            for x in 999_988..=1_000_012 {
                field.set(x, z, cell(0.8));
            }
        }
        let far = FrameInput {
            eye: DVec3::new(1_000_000.5, 70.0, 1_000_000.5),
            origin: DVec3::new(1_000_000.0, 70.0, 1_000_000.0),
            ..input(true)
        };
        let mesh = mesh_of(&field, &far);
        assert!(!mesh.vertices.is_empty());
        assert!(mesh
            .vertices
            .iter()
            .all(|v| v.position[0].abs() < 16.0 && v.position[2].abs() < 16.0));
    }

    fn mesh_of(field: &PrecipitationField, input: &FrameInput) -> WeatherMesh {
        let mut mesh = WeatherMesh::default();
        build(field, input, &mut mesh);
        mesh
    }

    #[test]
    fn fancy_covers_a_21_square_without_the_viewer_column() {
        let mesh = mesh_of(&field(0.8), &input(true));
        assert_eq!(mesh.vertices.len(), 6 * (21 * 21 - 1));
        assert!(mesh.vertices.len() <= MAX_VERTICES);
    }

    #[test]
    fn fast_covers_an_11_square() {
        let mesh = mesh_of(&field(0.8), &input(false));
        assert_eq!(mesh.vertices.len(), 6 * (11 * 11 - 1));
    }

    #[test]
    fn warm_columns_rain_and_cold_ones_snow() {
        let rain = mesh_of(&field(0.8), &input(false));
        assert_eq!(rain.runs.len(), 1);
        assert_eq!(rain.runs[0].kind, Precipitation::Rain);
        let snow = mesh_of(&field(0.0), &input(false));
        assert_eq!(snow.runs[0].kind, Precipitation::Snow);
    }

    #[test]
    fn height_turns_rain_into_snow() {
        // 0.25 (taiga) at the ground; the height term brings it below 0.15
        // from y = 124 up.
        let mut high = field(0.25);
        for z in -12..=12 {
            for x in -12..=12 {
                high.set(
                    x,
                    z,
                    PrecipitationCell {
                        height: 130,
                        ..cell(0.25)
                    },
                );
            }
        }
        let mut input = input(false);
        input.eye.y = 135.0;
        assert_eq!(mesh_of(&high, &input).runs[0].kind, Precipitation::Snow);
        assert_eq!(
            mesh_of(&field(0.25), &self::input(false)).runs[0].kind,
            Precipitation::Rain
        );
    }

    #[test]
    fn the_runs_follow_the_order_of_the_columns_and_split_on_a_change() {
        let mut field = field(0.8);
        field.set(0, -1, cell(0.0));
        let mesh = mesh_of(&field, &input(false));
        let kinds: Vec<_> = mesh.runs.iter().map(|run| run.kind).collect();
        assert_eq!(
            kinds,
            [
                Precipitation::Rain,
                Precipitation::Snow,
                Precipitation::Rain
            ]
        );
        let covered: u32 = mesh.runs.iter().map(|run| run.vertices.len() as u32).sum();
        assert_eq!(covered as usize, mesh.vertices.len());
        assert_eq!(mesh.runs[0].vertices.start, 0);
        assert_eq!(mesh.runs[1].vertices.start, mesh.runs[0].vertices.end);
    }

    #[test]
    fn a_column_reaches_from_the_height_or_below_the_viewer_to_above_it() {
        let mesh = mesh_of(&field(0.8), &input(false));
        let ys: Vec<f32> = mesh.vertices.iter().map(|v| v.position[1]).collect();
        // The viewer at 70 with radius 5 spans 65 to 75, above the ground at 64.
        assert!(ys.iter().all(|&y| y == 65.0 || y == 75.0));
    }

    #[test]
    fn no_quad_is_drawn_without_strength_or_cells() {
        let mut dry = input(true);
        dry.strength = 0.0;
        assert!(mesh_of(&field(0.8), &dry).vertices.is_empty());
        assert!(mesh_of(&PrecipitationField::default(), &input(true))
            .vertices
            .is_empty());
    }

    #[test]
    fn opacity_falls_with_distance_and_scales_with_strength() {
        let full = mesh_of(&field(0.8), &input(false));
        let alphas: Vec<f32> = full.vertices.iter().map(|v| v.alpha).collect();
        let max = alphas.iter().copied().fold(0.0, f32::max);
        let min = alphas.iter().copied().fold(1.0, f32::min);
        assert!(max <= 1.0 && max > 0.9, "{max}");
        // The corners of the square lie beyond the radius: no opacity left.
        assert!(min.abs() < 1e-3, "{min}");
        let mut drizzle = input(false);
        drizzle.strength = 0.5;
        let half = mesh_of(&field(0.8), &drizzle);
        assert!((half.vertices[0].alpha - full.vertices[0].alpha * 0.5).abs() < 1e-6);
    }

    #[test]
    fn rain_scrolls_down_with_time_and_snow_is_lit_towards_full_light() {
        let a = mesh_of(&field(0.8), &input(false));
        let mut later = input(false);
        later.ticks += 1.0;
        let b = mesh_of(&field(0.8), &later);
        assert_ne!(a.vertices[0].uv[1], b.vertices[0].uv[1]);
        let mut dark = field(0.0);
        dark.set(
            1,
            0,
            PrecipitationCell {
                sky_light: 0,
                block_light: 0,
                ..cell(0.0)
            },
        );
        let snow = mesh_of(&dark, &input(false));
        assert!(snow.vertices.iter().any(|v| v.light == [3.75, 3.75]));
    }

    #[test]
    fn the_quad_faces_the_viewer_by_the_offset_table() {
        let (x, z) = quad_offset(3, 0).expect("offset");
        assert_eq!((x, z), (0.0, 0.5));
        let (x, z) = quad_offset(0, 4).expect("offset");
        assert_eq!((x, z), (-0.5, 0.0));
        assert_eq!(quad_offset(0, 0), None);
    }

    #[test]
    fn the_height_term_of_the_temperature_starts_above_64() {
        assert_eq!(temperature_at(0.25, 64), 0.25);
        assert!((temperature_at(0.25, 124) - 0.15).abs() < 1e-6);
    }

    #[test]
    fn the_same_frame_builds_the_same_quads() {
        let a = mesh_of(&field(0.0), &input(true));
        let b = mesh_of(&field(0.0), &input(true));
        assert_eq!(a.vertices, b.vertices);
    }
}

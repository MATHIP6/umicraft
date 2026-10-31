// SPDX-License-Identifier: MIT OR Apache-2.0

//! Where the cloud layer sits relative to the eye in one frame.
//!
//! The reference rebuilds the clouds every frame from the camera position, but
//! the geometry never changes: only a shift of the grid and an offset into
//! the texture do. This module computes those two from the camera and the
//! drift clock, in `f64` as the reference does, so the layer neither jitters
//! far from the origin nor loses the drift to rounding.

use super::CloudMode;

/// The texture covers 256 cells; the reference wraps its scroll every 2048
/// units so single-precision texture coordinates stay exact.
const WRAP: f64 = 2048.0;
/// Blocks the layer drifts per tick (`d2 * 0.029999999329447746`).
const DRIFT_PER_TICK: f64 = 0.03;
/// Texture coordinate of one block in the fast version (`4.8828125E-4`).
const FAST_UV_PER_BLOCK: f64 = 1.0 / 2048.0;
/// Blocks per cell of the fancy version (`GlStateManager.scale(12, 1, 12)`).
pub(super) const FANCY_CELL: f64 = 12.0;
/// Texture coordinate of one texel (`0.00390625`).
pub(super) const UV_PER_TEXEL: f32 = 1.0 / 256.0;
/// Lift above the nominal height that keeps the layer off the exact plane
/// (`getCloudHeight() - y + 0.33`).
const HEIGHT_BIAS: f64 = 0.33;
/// The `+ 0.33` the fancy version adds to the Z coordinate.
const FANCY_Z_BIAS: f64 = 0.330_000_013_113_021_85;
/// Height of the cloud layer in the overworld (`WorldProvider.getCloudHeight`).
pub(super) const CLOUD_HEIGHT: f64 = 128.0;

/// The per-frame placement of a cloud mesh.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Placement {
    /// Added to the mesh position before `scale`; the Y is the height of the
    /// cloud base above the eye.
    pub shift: [f32; 3],
    /// Multiplies X and Z of the shifted position.
    pub scale: [f32; 2],
    /// Added to every texture coordinate.
    pub uv_offset: [f32; 2],
}

/// `value` wrapped into `0..2048` the way `d - floor(d / 2048) * 2048` is.
pub(super) fn wrap(value: f64) -> f64 {
    value - (value / WRAP).floor() * WRAP
}

/// The drift in blocks after `ticks` client ticks.
pub(super) fn drift(ticks: f64) -> f64 {
    ticks * DRIFT_PER_TICK
}

/// Placement of the mesh of `mode` for an eye at `eye` after `ticks` ticks.
/// `Off` has no placement.
pub(super) fn placement(mode: CloudMode, eye: [f64; 3], ticks: f64) -> Option<Placement> {
    let height = CLOUD_HEIGHT - eye[1] + HEIGHT_BIAS;
    match mode {
        CloudMode::Off => None,
        CloudMode::Fast => {
            let u = wrap(eye[0] + drift(ticks));
            let v = wrap(eye[2]);
            Some(Placement {
                shift: [0.0, height as f32, 0.0],
                scale: [1.0, 1.0],
                uv_offset: [
                    (u * FAST_UV_PER_BLOCK) as f32,
                    (v * FAST_UV_PER_BLOCK) as f32,
                ],
            })
        }
        CloudMode::Fancy => {
            let u = wrap((eye[0] + drift(ticks)) / FANCY_CELL);
            let v = wrap(eye[2] / FANCY_CELL + FANCY_Z_BIAS);
            Some(Placement {
                // The grid slides by the fractional cell; the whole cells
                // move into the texture offset.
                shift: [
                    -(u - u.floor()) as f32,
                    height as f32,
                    -(v - v.floor()) as f32,
                ],
                scale: [FANCY_CELL as f32, FANCY_CELL as f32],
                uv_offset: [
                    u.floor() as f32 * UV_PER_TEXEL,
                    v.floor() as f32 * UV_PER_TEXEL,
                ],
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scroll_wraps_every_2048_units() {
        assert_eq!(wrap(0.0), 0.0);
        assert_eq!(wrap(2048.0), 0.0);
        assert_eq!(wrap(2049.5), 1.5);
        assert_eq!(wrap(-1.0), 2047.0);
    }

    #[test]
    fn the_drift_is_three_hundredths_of_a_block_per_tick() {
        assert!((drift(100.0) - 3.0).abs() < 1e-9);
    }

    #[test]
    fn off_has_no_placement() {
        assert_eq!(placement(CloudMode::Off, [0.0; 3], 0.0), None);
    }

    #[test]
    fn the_fast_layer_sits_above_the_eye_and_scrolls_with_x_and_the_drift() {
        let still = placement(CloudMode::Fast, [0.0, 64.0, 0.0], 0.0).unwrap();
        assert!((still.shift[1] - 64.33).abs() < 1e-4);
        assert_eq!(still.uv_offset, [0.0, 0.0]);
        // 1024 blocks east: half the texture.
        let moved = placement(CloudMode::Fast, [1024.0, 64.0, 0.0], 0.0).unwrap();
        assert!((moved.uv_offset[0] - 0.5).abs() < 1e-6);
        // The drift alone moves it the same way.
        let drifting = placement(CloudMode::Fast, [0.0, 64.0, 0.0], 100.0).unwrap();
        assert!((drifting.uv_offset[0] - 3.0 / 2048.0).abs() < 1e-7);
    }

    #[test]
    fn the_fancy_grid_slides_by_the_fraction_of_a_cell() {
        // 18 blocks = 1.5 cells: whole cell 1 in the texture, half a cell of shift.
        let placed = placement(CloudMode::Fancy, [18.0, 64.0, 0.0], 0.0).unwrap();
        assert!((placed.shift[0] + 0.5).abs() < 1e-6);
        assert!((placed.uv_offset[0] - UV_PER_TEXEL).abs() < 1e-9);
        assert_eq!(placed.scale, [12.0, 12.0]);
    }

    #[test]
    fn the_layer_height_follows_the_eye() {
        let low = placement(CloudMode::Fancy, [0.0, 0.0, 0.0], 0.0).unwrap();
        let high = placement(CloudMode::Fancy, [0.0, 130.0, 0.0], 0.0).unwrap();
        assert!((low.shift[1] - 128.33).abs() < 1e-4);
        assert!((high.shift[1] + 1.67).abs() < 1e-4);
    }

    #[test]
    fn a_far_position_keeps_the_texture_offset_in_range() {
        let far = placement(CloudMode::Fast, [30_000_000.0, 64.0, -30_000_000.0], 5e6).unwrap();
        assert!(far.uv_offset.iter().all(|c| (0.0..1.0).contains(c)));
    }
}

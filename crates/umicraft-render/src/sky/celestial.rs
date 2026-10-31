// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sun, the moon and the sunrise / sunset glow
//! (`RenderGlobal.renderSky`, bodies and fan).
//!
//! The sun is a 30 x 30 quad and the moon a 20 x 20 quad, both 100 blocks
//! from the eye; they sit on opposite sides of the same turn, which is
//! `rotate(-90, y)` then `rotate(angle * 360, x)`. The moon texture is a 4 x 2
//! sheet of phases picked by `phase % 4` (column) and `phase / 4 % 2` (row).

use glam::Mat4;

use super::vertex::{quad, SkyVertex};

/// Distance of the bodies from the eye.
const DISTANCE: f32 = 100.0;
/// Half the sun quad's side (`f17 = 30`).
const SUN_HALF: f32 = 30.0;
/// Half the moon quad's side (`f17 = 20`).
const MOON_HALF: f32 = 20.0;
/// Number of moon phases in the sheet.
pub(super) const MOON_PHASES: usize = 8;
/// Segments of the sunrise fan (`for l = 0; l <= 16`).
const FAN_SEGMENTS: usize = 16;
/// Triangle-list vertices of the sunrise fan.
pub(super) const FAN_VERTICES: usize = FAN_SEGMENTS * 3;

/// Model shared by the sun, moon and stars at `celestial_angle`.
pub(super) fn sky_turn(celestial_angle: f32) -> Mat4 {
    Mat4::from_rotation_y(-std::f32::consts::FRAC_PI_2)
        * Mat4::from_rotation_x(celestial_angle * std::f32::consts::TAU)
}

/// The sun quad (`sun.png`, full texture).
pub(super) fn sun() -> [SkyVertex; 6] {
    let v = SkyVertex::textured;
    quad([
        v([-SUN_HALF, DISTANCE, -SUN_HALF], [0.0, 0.0]),
        v([SUN_HALF, DISTANCE, -SUN_HALF], [1.0, 0.0]),
        v([SUN_HALF, DISTANCE, SUN_HALF], [1.0, 1.0]),
        v([-SUN_HALF, DISTANCE, SUN_HALF], [0.0, 1.0]),
    ])
}

/// The texture rectangle `[u0, v0, u1, v1]` of `phase` in `moon_phases.png`.
pub(super) fn moon_uv(phase: u8) -> [f32; 4] {
    let column = f32::from(phase % 4);
    let row = f32::from(phase / 4 % 2);
    [
        column / 4.0,
        row / 2.0,
        (column + 1.0) / 4.0,
        (row + 1.0) / 2.0,
    ]
}

/// The moon quads of all eight phases, `MOON_PHASES * 6` vertices; phase
/// `p` starts at `p * 6`. The corner order mirrors the reference, which
/// draws the moon from below.
pub(super) fn moons() -> Vec<SkyVertex> {
    (0..MOON_PHASES as u8)
        .flat_map(|phase| {
            let [u0, v0, u1, v1] = moon_uv(phase);
            let v = SkyVertex::textured;
            quad([
                v([-MOON_HALF, -DISTANCE, MOON_HALF], [u1, v1]),
                v([MOON_HALF, -DISTANCE, MOON_HALF], [u0, v1]),
                v([MOON_HALF, -DISTANCE, -MOON_HALF], [u0, v0]),
                v([-MOON_HALF, -DISTANCE, -MOON_HALF], [u1, v0]),
            ])
        })
        .collect()
}

/// Model of the sunrise fan: `rotate(90, x)`, a half turn about `z` while
/// `sin(angle)` is negative (the sun is on the rising, eastern side), then
/// `rotate(90, z)`.
pub(super) fn sunrise_model(celestial_angle: f32) -> Mat4 {
    let eastern = (celestial_angle * std::f32::consts::TAU).sin() < 0.0;
    let half_turn = if eastern { std::f32::consts::PI } else { 0.0 };
    Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2)
        * Mat4::from_rotation_z(half_turn)
        * Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2)
}

/// The glow fan for `colors = [r, g, b, alpha]`: a coloured centre at
/// `(0, 100, 0)` and a transparent ring of radius 120 whose depth bulges with
/// the alpha, as a triangle list of [`FAN_VERTICES`] vertices.
pub(super) fn sunrise_fan(colors: [f32; 4]) -> [SkyVertex; FAN_VERTICES] {
    let [r, g, b, alpha] = colors;
    let centre = SkyVertex::coloured([0.0, DISTANCE, 0.0], [0.0; 2], [r, g, b, alpha]);
    let ring = |index: usize| {
        let angle = index as f32 * std::f32::consts::TAU / FAN_SEGMENTS as f32;
        let (sin, cos) = angle.sin_cos();
        SkyVertex::coloured(
            [sin * 120.0, cos * 120.0, -cos * 40.0 * alpha],
            [0.0; 2],
            [r, g, b, 0.0],
        )
    };
    std::array::from_fn(|i| match i % 3 {
        0 => centre,
        1 => ring(i / 3),
        _ => ring(i / 3 + 1),
    })
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::*;

    fn sun_direction(celestial_angle: f32) -> Vec3 {
        sky_turn(celestial_angle).transform_point3(Vec3::Y)
    }

    #[test]
    fn the_sun_rises_in_the_east_and_the_moon_is_opposite() {
        let sunrise = sun_direction(0.8536);
        assert!(sunrise.x > 0.5 && sunrise.y > 0.5 && sunrise.z.abs() < 1e-5);
        assert!(sun_direction(0.0).y > 0.999);
        let sunset = sun_direction(0.1464);
        assert!(sunset.x < -0.5 && sunset.y > 0.5);
        let moon = sky_turn(0.3).transform_point3(Vec3::NEG_Y);
        let sun = sun_direction(0.3);
        assert!((moon + sun).length() < 1e-5);
    }

    #[test]
    fn the_moon_phases_pick_column_and_row_like_the_reference() {
        assert_eq!(moon_uv(0), [0.0, 0.0, 0.25, 0.5]);
        assert_eq!(moon_uv(3), [0.75, 0.0, 1.0, 0.5]);
        assert_eq!(moon_uv(4), [0.0, 0.5, 0.25, 1.0]);
        assert_eq!(moon_uv(7), [0.75, 0.5, 1.0, 1.0]);
        // The reference's `i / 4 % 2` wraps past phase 7.
        assert_eq!(moon_uv(8), moon_uv(0));
    }

    #[test]
    fn the_moon_quads_carry_their_phase_rectangle() {
        let moons = moons();
        assert_eq!(moons.len(), MOON_PHASES * 6);
        let first = &moons[6 * 5..6 * 6];
        let [u0, v0, u1, v1] = moon_uv(5);
        assert_eq!(first[0].uv, [u1, v1]);
        assert_eq!(first[1].uv, [u0, v1]);
        assert_eq!(first[2].uv, [u0, v0]);
        assert!(first.iter().all(|v| v.position[1] == -100.0));
    }

    #[test]
    fn the_sun_quad_is_30_wide_at_100_blocks() {
        let sun = sun();
        assert!(sun.iter().all(|v| v.position[1] == 100.0));
        assert_eq!(sun[0].position, [-30.0, 100.0, -30.0]);
        assert_eq!(sun[2].uv, [1.0, 1.0]);
    }

    #[test]
    fn the_fan_has_a_coloured_centre_and_a_transparent_ring() {
        let fan = sunrise_fan([0.85, 0.375, 0.2, 1.0]);
        assert_eq!(fan.len(), FAN_VERTICES);
        assert_eq!(fan[0].color, [0.85, 0.375, 0.2, 1.0]);
        assert_eq!(fan[1].color[3], 0.0);
        // Ring point 0 is (0, 120, -40 * alpha).
        assert_eq!(fan[1].position, [0.0, 120.0, -40.0]);
        // The fan closes on itself: the last ring point is the first.
        let last = fan[FAN_VERTICES - 1].position;
        assert!((last[1] - 120.0).abs() < 1e-3 && last[0].abs() < 1e-3);
    }

    #[test]
    fn the_fan_faces_the_sun_side() {
        // At sunrise (east, +X) the fan's apex (0, 100, 0) must land on +X;
        // at sunset on -X.
        let apex = |angle| sunrise_model(angle).transform_point3(Vec3::new(0.0, 100.0, 0.0));
        assert!(apex(0.8536).x > 99.0);
        assert!(apex(0.1464).x < -99.0);
    }
}

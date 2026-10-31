// SPDX-License-Identifier: MIT OR Apache-2.0

//! The cloud tint of `World.getCloudColour`.
//!
//! It lives beside the sky colours because it reads the same celestial angle
//! and weather inputs, but only the cloud pass consumes it.

/// `World.cloudColour`, `0xFFFFFF`: white, before the time and weather tint.
const BASE: [f32; 3] = [1.0; 3];

/// `World.getCloudColour`: white clouds darkened toward grey by rain and
/// thunder and dimmed by the time of day.
///
/// `cos_angle` is `cos(celestial_angle * 2π)`; `rain` and `thunder` are the
/// interpolated strengths in `0.0..=1.0`. Blue keeps a little more light at
/// night than red and green (`0.15` against `0.1`), as in the reference.
pub(super) fn cloud_color(cos_angle: f32, rain: f32, thunder: f32) -> [f32; 3] {
    let daylight = (cos_angle * 2.0 + 0.5).clamp(0.0, 1.0);
    let mut color = BASE;
    if rain > 0.0 {
        color = toward_grey(color, 1.0 - rain * 0.95, 0.6);
    }
    color = [
        color[0] * (daylight * 0.9 + 0.1),
        color[1] * (daylight * 0.9 + 0.1),
        color[2] * (daylight * 0.85 + 0.15),
    ];
    if thunder > 0.0 {
        color = toward_grey(color, 1.0 - thunder * 0.95, 0.2);
    }
    color
}

/// Mixes `color` with its luma times `grey_scale`, keeping `keep` of the
/// colour: the shared step of the rain and thunder darkening.
fn toward_grey(color: [f32; 3], keep: f32, grey_scale: f32) -> [f32; 3] {
    let grey = (color[0] * 0.3 + color[1] * 0.59 + color[2] * 0.11) * grey_scale;
    color.map(|channel| channel * keep + grey * (1.0 - keep))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5)
    }

    #[test]
    fn clouds_are_white_at_noon_and_dim_at_midnight() {
        assert!(near(cloud_color(1.0, 0.0, 0.0), [1.0; 3]));
        // cos = -1: daylight 0, so 0.1 / 0.1 / 0.15.
        assert!(near(cloud_color(-1.0, 0.0, 0.0), [0.1, 0.1, 0.15]));
    }

    #[test]
    fn rain_pulls_the_clouds_toward_grey() {
        let rain = cloud_color(1.0, 1.0, 0.0);
        // keep 0.05, grey 0.6: 0.05 + 0.6 * 0.95 = 0.62 on every channel.
        assert!(near(rain, [0.62; 3]));
    }

    #[test]
    fn thunder_darkens_after_the_time_factor() {
        let storm = cloud_color(1.0, 1.0, 1.0);
        // The grey of the rained cloud (0.62) is scaled by 0.2 at keep 0.05.
        let expected = 0.62 * 0.05 + 0.62 * 0.2 * 0.95;
        assert!(near(storm, [expected; 3]));
    }
}

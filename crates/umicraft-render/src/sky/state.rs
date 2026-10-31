// SPDX-License-Identifier: MIT OR Apache-2.0

//! The pure sky model: every colour and factor the sky pass, the terrain
//! shader and the fog read for one frame.
//!
//! Each formula is a line-by-line port of the 1.8 reference, so the rain,
//! thunder and lightning inputs darken the sky exactly where it does:
//!
//! - `WorldProvider.calculateCelestialAngle` (Nether `0.5`, End `0.0`),
//!   `getMoonPhase`, `calcSunriseSunsetColors`, `getFogColor`;
//! - `World.getSkyColor` (with `BiomeGenBase.getSkyColorByTemp`),
//!   `getSunBrightness`, `getStarBrightness`;
//! - `EntityRenderer.updateFogColor` for the final fog colour.
//!
//! # Celestial convention
//!
//! `celestial_angle` is `0` at noon, `0.5` at midnight and about `0.85` /
//! `0.15` at sunrise / sunset. The sun sits at `(-sin(2π·angle), cos(2π·angle),
//! 0)` times the distance, so it rises in the east (`+X`).

use crate::RenderEnvironment;

/// Length of one day/night cycle in ticks (`WorldProvider`).
const DAY_TICKS: i64 = 24_000;
/// `0xA080A0`, the base colour of the End fog (`WorldProviderEnd.getFogColor`).
const END_FOG_BASE: [f32; 3] = [160.0 / 255.0, 128.0 / 255.0, 160.0 / 255.0];
/// The Nether fog (`WorldProviderHell.getFogColor`).
const NETHER_FOG: [f32; 3] = [0.2, 0.03, 0.03];

/// Which sky a dimension draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkyKind {
    /// Sun, moon, stars and the dome (`WorldProviderSurface`).
    Overworld,
    /// No sky, only the clear colour (`isSurfaceWorld() == false`).
    Nether,
    /// The six-face cube of `end_sky.png` (`renderSkyEnd`).
    End,
}

impl SkyKind {
    /// The sky of a Join Game dimension. An unknown dimension falls back to
    /// the overworld, as a server that invents one still sends a day clock.
    pub(crate) fn of(dimension: i8) -> Self {
        match dimension {
            -1 => Self::Nether,
            1 => Self::End,
            _ => Self::Overworld,
        }
    }
}

/// Colours and celestial factors for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyState {
    /// Celestial angle in `0.0..1.0` (see the module convention).
    pub celestial_angle: f32,
    /// Linear sky colour overhead (`World.getSkyColor`), rain-darkened.
    pub sky_color: [f32; 3],
    /// Fog colour, which is also the clear colour (`updateFogColor`).
    pub fog_color: [f32; 3],
    /// Sun-light factor in `0.2..=1.0` applied to sky light in the terrain
    /// shader; `0` in the Nether and End, which have no sky light.
    pub sun_brightness: f32,
    /// Star opacity in `0.0..=0.5` before the rain dimming
    /// (`World.getStarBrightness`).
    pub star_brightness: f32,
    /// The factor `1 - rainStrength` that dims the sun, moon and stars.
    pub rain_dim: f32,
    /// Moon phase index in `0..=7`; 0 is a full moon.
    pub moon_phase: u8,
    /// Sunrise / sunset glow `[r, g, b, alpha]` while the sun is near the
    /// horizon (`calcSunriseSunsetColors`), `None` otherwise.
    pub sunrise: Option<[f32; 4]>,
}

impl SkyState {
    /// Computes the sky state for `environment`.
    ///
    /// Not modelled here (they need the camera or the block at the eye): the
    /// sunrise tint of the fog toward the sun, the void darkening below
    /// `y = 32` and the water / lava fog colours (P7g).
    #[must_use]
    pub fn at(environment: RenderEnvironment) -> Self {
        let kind = SkyKind::of(environment.dimension);
        let celestial_angle = match kind {
            SkyKind::Overworld => celestial_angle(environment.world_time, environment.partial_tick),
            SkyKind::Nether => 0.5,
            SkyKind::End => 0.0,
        };
        let sky_color = sky_color(celestial_angle, &environment);
        Self {
            celestial_angle,
            sky_color,
            fog_color: final_fog_color(kind, celestial_angle, sky_color, &environment),
            sun_brightness: match kind {
                SkyKind::Overworld => sun_brightness(celestial_angle, &environment),
                SkyKind::Nether | SkyKind::End => 0.0,
            },
            star_brightness: star_brightness(celestial_angle),
            rain_dim: 1.0 - environment.rain_strength.clamp(0.0, 1.0),
            moon_phase: moon_phase(environment.world_time),
            sunrise: match kind {
                SkyKind::Overworld => sunrise_colors(celestial_angle),
                SkyKind::Nether | SkyKind::End => None,
            },
        }
    }
}

/// `cos(angle * 2π)`, the cosine every curve of the reference is built on.
fn cos_cycle(angle: f32) -> f32 {
    (angle * std::f32::consts::TAU).cos()
}

/// `WorldProvider.calculateCelestialAngle`.
///
/// Rust's `%` truncates like Java's, so a negative (frozen) clock takes the
/// same branch of the wrap as the reference.
fn celestial_angle(world_time: i64, partial_tick: f32) -> f32 {
    let ticks = (world_time % DAY_TICKS) as f32;
    let mut phase = (ticks + partial_tick.clamp(0.0, 1.0)) / DAY_TICKS as f32 - 0.25;
    if phase < 0.0 {
        phase += 1.0;
    }
    if phase > 1.0 {
        phase -= 1.0;
    }
    1.0 - ((f64::from(phase) * std::f64::consts::PI).cos() + 1.0) as f32 / 2.0
}

/// `WorldProvider.getMoonPhase` in `0..=7`, 0 being the full moon.
fn moon_phase(world_time: i64) -> u8 {
    // The reference mixes Java's truncating `/` and `%`, which Rust shares.
    // `(x % 8 + 8) % 8` is in `0..=7`, so the narrowing conversion is exact.
    ((world_time / DAY_TICKS % 8 + 8) % 8) as u8
}

/// `World.getStarBrightness`.
fn star_brightness(angle: f32) -> f32 {
    let brightness = (1.0 - (cos_cycle(angle) * 2.0 + 0.25)).clamp(0.0, 1.0);
    brightness * brightness * 0.5
}

/// `1 - strength * 5 / 16`, the factor rain and thunder apply to daylight.
fn weather_light(strength: f32) -> f32 {
    1.0 - strength * 5.0 / 16.0
}

/// `World.getSunBrightness`.
fn sun_brightness(angle: f32, environment: &RenderEnvironment) -> f32 {
    // The reference inverts the clamped value twice; the result is the
    // clamped ramp itself.
    let day = (cos_cycle(angle) * 2.0 + 0.2).clamp(0.0, 1.0);
    let weathered = day
        * weather_light(environment.rain_strength)
        * weather_light(environment.thunder_strength);
    weathered * 0.8 + 0.2
}

/// `MathHelper.func_181758_c`: HSB to RGB with the reference's truncation to
/// 8 bits per channel.
fn hsb_to_rgb(hue: f32, saturation: f32, brightness: f32) -> [f32; 3] {
    let sector = (hue * 6.0) as i32 % 6;
    let fraction = hue * 6.0 - sector as f32;
    let low = brightness * (1.0 - saturation);
    let falling = brightness * (1.0 - fraction * saturation);
    let rising = brightness * (1.0 - (1.0 - fraction) * saturation);
    let (r, g, b) = match sector {
        0 => (brightness, rising, low),
        1 => (falling, brightness, low),
        2 => (low, brightness, rising),
        3 => (low, falling, brightness),
        4 => (rising, low, brightness),
        _ => (brightness, low, falling),
    };
    [r, g, b].map(|channel| f32::from(((channel * 255.0) as i32).clamp(0, 255) as u8) / 255.0)
}

/// `BiomeGenBase.getSkyColorByTemp`.
fn biome_sky_color(temperature: f32) -> [f32; 3] {
    let scaled = (temperature / 3.0).clamp(-1.0, 1.0);
    hsb_to_rgb(0.622_222_24 - scaled * 0.05, 0.5 + scaled * 0.1, 1.0)
}

/// Pulls a colour toward the grey `luma * target` by `1 - keep`, the
/// desaturation rain and thunder apply in `World.getSkyColor`.
fn darken_toward_grey(color: [f32; 3], strength: f32, grey_scale: f32) -> [f32; 3] {
    if strength <= 0.0 {
        return color;
    }
    let grey = (color[0] * 0.3 + color[1] * 0.59 + color[2] * 0.11) * grey_scale;
    let keep = 1.0 - strength * 0.75;
    color.map(|channel| channel * keep + grey * (1.0 - keep))
}

/// `World.getSkyColor`: the biome colour scaled by daylight, then darkened by
/// rain, thunder and lightning.
fn sky_color(angle: f32, environment: &RenderEnvironment) -> [f32; 3] {
    let daylight = (cos_cycle(angle) * 2.0 + 0.5).clamp(0.0, 1.0);
    let base = biome_sky_color(environment.sky_temperature).map(|channel| channel * daylight);
    let rained = darken_toward_grey(base, environment.rain_strength, 0.6);
    let stormed = darken_toward_grey(rained, environment.thunder_strength, 0.2);
    apply_lightning(stormed, environment)
}

/// The lightning flash of `World.getSkyColor`: up to 45 % toward a pale blue.
fn apply_lightning(color: [f32; 3], environment: &RenderEnvironment) -> [f32; 3] {
    if environment.lightning_ticks == 0 {
        return color;
    }
    let remaining = f32::from(environment.lightning_ticks) - environment.partial_tick;
    let flash = remaining.min(1.0) * 0.45;
    let flash_color = [0.8, 0.8, 1.0];
    [0, 1, 2].map(|i| color[i] * (1.0 - flash) + flash_color[i] * flash)
}

/// `WorldProvider.getFogColor` of the dimension.
fn provider_fog_color(kind: SkyKind, angle: f32) -> [f32; 3] {
    let daylight = (cos_cycle(angle) * 2.0 + 0.5).clamp(0.0, 1.0);
    match kind {
        SkyKind::Overworld => {
            let base = [0.752_941_2, 0.847_058_83, 1.0];
            [
                base[0] * (daylight * 0.94 + 0.06),
                base[1] * (daylight * 0.94 + 0.06),
                base[2] * (daylight * 0.91 + 0.09),
            ]
        }
        SkyKind::Nether => NETHER_FOG,
        // `daylight * 0 + 0.15`: the End fog does not follow the clock.
        SkyKind::End => END_FOG_BASE.map(|channel| channel * 0.15),
    }
}

/// `EntityRenderer.updateFogColor` without the camera-dependent parts: the
/// provider fog drawn toward the sky colour by the view distance, then dimmed
/// by rain and thunder.
fn final_fog_color(
    kind: SkyKind,
    angle: f32,
    sky: [f32; 3],
    environment: &RenderEnvironment,
) -> [f32; 3] {
    let fog = provider_fog_color(kind, angle);
    // Far views see more of the sky colour at the horizon.
    let view = 0.25 + 0.75 * f32::from(environment.view_distance) / 32.0;
    let sky_mix = 1.0 - view.powf(0.25);
    let mut color = [0, 1, 2].map(|i| fog[i] + (sky[i] - fog[i]) * sky_mix);

    let rain = environment.rain_strength;
    if rain > 0.0 {
        let red_green = 1.0 - rain * 0.5;
        let blue = 1.0 - rain * 0.4;
        color = [color[0] * red_green, color[1] * red_green, color[2] * blue];
    }
    let thunder = environment.thunder_strength;
    if thunder > 0.0 {
        color = color.map(|channel| channel * (1.0 - thunder * 0.5));
    }
    color
}

/// `WorldProvider.calcSunriseSunsetColors`: the glow `[r, g, b, alpha]` while
/// the sun is within 0.4 of the horizon on the cosine curve.
fn sunrise_colors(angle: f32) -> Option<[f32; 4]> {
    const RANGE: f32 = 0.4;
    let height = cos_cycle(angle);
    if !(-RANGE..=RANGE).contains(&height) {
        return None;
    }
    let t = height / RANGE * 0.5 + 0.5;
    let alpha = 1.0 - (1.0 - (t * std::f32::consts::PI).sin()) * 0.99;
    Some([t * 0.3 + 0.7, t * t * 0.7 + 0.2, 0.2, alpha * alpha])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(time: i64) -> SkyState {
        SkyState::at(RenderEnvironment::new(0, time, 0.0, 8))
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn the_celestial_angle_follows_the_reference_curve() {
        assert!(near(state(6_000).celestial_angle, 0.0));
        assert!(near(state(18_000).celestial_angle, 0.5));
        assert!(near(state(0).celestial_angle, 0.8536));
        assert!(near(state(12_000).celestial_angle, 0.1464));
    }

    #[test]
    fn a_frozen_negative_clock_wraps_like_java() {
        // -6000 % 24000 is -6000 in Java: (-0.25 - 0.25) + 1 = 0.5.
        assert!(near(
            SkyState::at(RenderEnvironment::new(0, -6_000, 0.0, 8)).celestial_angle,
            state(18_000).celestial_angle
        ));
        assert!(SkyState::at(RenderEnvironment::new(0, i64::MIN, 0.0, 8))
            .celestial_angle
            .is_finite());
    }

    #[test]
    fn the_moon_phase_counts_days_with_javas_modulo() {
        assert_eq!(state(24_000).moon_phase, 1);
        assert_eq!(state(24_000 * 7).moon_phase, 7);
        assert_eq!(state(24_000 * 8).moon_phase, 0);
        // `-24000 / 24000 = -1`: `(-1 % 8 + 8) % 8 = 7`.
        assert_eq!(state(-24_000).moon_phase, 7);
        // `-6000 / 24000 = 0` in Java, so a frozen first day stays phase 0.
        assert_eq!(state(-6_000).moon_phase, 0);
    }

    #[test]
    fn the_plains_sky_is_the_reference_blue_at_noon() {
        // HSB(0.62222224 - 0.8/3*0.05, 0.5 + 0.8/3*0.1, 1) truncated to bytes.
        let sky = state(6_000).sky_color;
        let bytes = sky.map(|c| (c * 255.0).round() as i32);
        assert_eq!(bytes, [120, 167, 255]);
        let midnight = state(18_000).sky_color;
        assert_eq!(midnight, [0.0; 3], "cos(π)·2 + 0.5 clamps to zero");
    }

    #[test]
    fn colder_biomes_shift_the_sky_hue() {
        let cold = SkyState::at(RenderEnvironment {
            sky_temperature: -0.5,
            ..RenderEnvironment::DEFAULT
        });
        let hot = SkyState::at(RenderEnvironment {
            sky_temperature: 2.0,
            ..RenderEnvironment::DEFAULT
        });
        // Lower saturation (cold) lifts the red channel, higher (hot) drops it.
        let plains = state(6_000).sky_color[0];
        assert!(cold.sky_color[0] > plains && hot.sky_color[0] < plains);
    }

    #[test]
    fn rain_and_thunder_darken_and_desaturate_the_sky() {
        let clear = state(6_000);
        let rain = SkyState::at(RenderEnvironment {
            rain_strength: 1.0,
            ..RenderEnvironment::DEFAULT
        });
        let storm = SkyState::at(RenderEnvironment {
            rain_strength: 1.0,
            thunder_strength: 1.0,
            ..RenderEnvironment::DEFAULT
        });
        let sum = |c: [f32; 3]| c[0] + c[1] + c[2];
        assert!(sum(rain.sky_color) < sum(clear.sky_color));
        assert!(sum(storm.sky_color) < sum(rain.sky_color));
        // Rain 1: keep 0.25 of the colour plus 0.75 of grey = luma * 0.6.
        let luma = (120.0 * 0.3 + 167.0 * 0.59 + 255.0 * 0.11) / 255.0 * 0.6;
        let expected_red = 120.0 / 255.0 * 0.25 + luma * 0.75;
        assert!(near(rain.sky_color[0], expected_red));
        assert!(near(rain.rain_dim, 0.0));
        // Sun brightness: 1 - 5/16 per full strength.
        assert!(near(rain.sun_brightness, (1.0 - 5.0 / 16.0) * 0.8 + 0.2));
    }

    #[test]
    fn lightning_blends_toward_pale_blue_by_at_most_45_percent() {
        let flash = SkyState::at(RenderEnvironment {
            lightning_ticks: 5,
            ..RenderEnvironment::DEFAULT
        });
        let plain = state(6_000);
        assert!(near(
            flash.sky_color[0],
            plain.sky_color[0] * 0.55 + 0.8 * 0.45
        ));
        assert!(near(flash.sky_color[2], plain.sky_color[2] * 0.55 + 0.45));
    }

    #[test]
    fn sun_and_star_brightness_follow_the_day() {
        assert!(near(state(6_000).sun_brightness, 1.0));
        assert!(near(state(18_000).sun_brightness, 0.2));
        assert!(near(state(6_000).star_brightness, 0.0));
        assert!(near(
            state(18_000).star_brightness,
            0.5 * 0.75 * 0.75 * 0.0 + 0.5
        ));
    }

    #[test]
    fn the_sunrise_glow_exists_only_near_the_horizon() {
        assert!(state(6_000).sunrise.is_none());
        assert!(state(18_000).sunrise.is_none());
        let sunrise = state(23_000).sunrise.expect("glow before sunrise");
        // t = 0.5: red 0.85, green 0.375, alpha (1 - 0 * 0.99)^2 = 1 at the
        // horizon crossing; the early-morning sample is somewhere inside.
        assert!(sunrise[0] > 0.7 && sunrise[3] > 0.0 && sunrise[3] <= 1.0);
        let horizon = sunrise_colors(0.25).expect("exact horizon");
        assert!(near(horizon[0], 0.85) && near(horizon[1], 0.375) && near(horizon[3], 1.0));
    }

    #[test]
    fn the_nether_and_end_have_fixed_fog_and_no_sunlight() {
        let nether = SkyState::at(RenderEnvironment::new(-1, 6_000, 0.0, 8));
        let end = SkyState::at(RenderEnvironment::new(1, 6_000, 0.0, 8));
        for sample in [nether, end] {
            assert_eq!(sample.sun_brightness, 0.0);
            assert!(sample.sunrise.is_none());
        }
        assert_eq!(nether.celestial_angle, 0.5);
        assert_eq!(end.celestial_angle, 0.0);
        // Nether sky is black (cosine clamp), so the fog is only the base.
        assert_eq!(nether.sky_color, [0.0; 3]);
        let mixed = 1.0 - (0.25f32 + 0.75 * 8.0 / 32.0).powf(0.25);
        assert!(near(nether.fog_color[0], 0.2 * (1.0 - mixed)));
        assert!(end.fog_color[0] > 0.0 && end.fog_color[0] < 0.3);
    }

    #[test]
    fn the_fog_is_dimmed_by_rain_and_thunder() {
        let clear = state(6_000).fog_color;
        let rain = SkyState::at(RenderEnvironment {
            rain_strength: 1.0,
            ..RenderEnvironment::DEFAULT
        });
        assert!(rain.fog_color[0] < clear[0] * 0.55 + 0.1);
        assert!(rain.fog_color[2] < clear[2]);
    }

    #[test]
    fn an_unknown_dimension_draws_the_overworld() {
        assert_eq!(SkyKind::of(7), SkyKind::Overworld);
        assert_eq!(SkyKind::of(-1), SkyKind::Nether);
        assert_eq!(SkyKind::of(1), SkyKind::End);
    }
}

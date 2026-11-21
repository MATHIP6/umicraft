// SPDX-License-Identifier: MIT OR Apache-2.0

//! Custom sky layers: `sky/world<N>/sky<K>.properties` (F6 phase 4).
//!
//! A resource pack may draw textured layers over the sky of a dimension. Each
//! layer is a cube around the eye with a 3×2 texture, faded in and out by the
//! time of day, the weather, the biome and the eye height, optionally turning
//! with the sun, and blended with one of nine modes.
//!
//! [`SkyPlan::scan`] reads every layer of the packs, and [`SkyLayer::parse`]
//! reads one file. [`SkyPlan::evaluate`] gives the layers to draw this frame
//! with their brightness and rotation. It allocates nothing: the caller keeps
//! a [`SkyState`] for the smoothing and an output `Vec` it reuses.
//!
//! The behaviour follows OptiFine's `CustomSky`, `CustomSkyLayer`, `Blender`,
//! `SmoothFloat` and `TextureUtils.fixResourcePath` (decompiled 1.8.9 build,
//! read for behaviour only, ADR-0088), and the public OptiFineDoc
//! `sky.properties`. Where they differ, the documented rule wins: a source
//! without a `/` is relative to the properties file.

use thiserror::Error;

use crate::{BiomeFilter, Properties, PropertiesError, RangeList, WeatherSet};

/// The dimensions OptiFine scans: `world0` to `world9`. The overworld is 0
/// and the End is 1; the Nether (-1) has no sky layers.
pub const DIMENSIONS: usize = 10;
/// The most layers read per dimension (OptiFine stops at 999).
const MAX_LAYERS: u32 = 999;
/// Ticks in a day.
const DAY: i32 = 24_000;
/// Below this brightness a layer is not drawn.
const MIN_BRIGHTNESS: f32 = 1.0e-4;

/// Why a sky layer file was rejected. The layer is skipped; the others stay.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SkyLayerError {
    /// The file is not a valid `.properties` file.
    #[error(transparent)]
    Properties(#[from] PropertiesError),
    /// `startFadeIn`, `endFadeIn` or `endFadeOut` is missing or not `HH:MM`.
    #[error("startFadeIn, endFadeIn and endFadeOut are required as HH:MM")]
    MissingTimes,
    /// The fade-in, hold, fade-out and off spans do not add up to a day.
    #[error("the fade times cover {0} ticks instead of 24000")]
    NotOneDay(i32),
    /// `speed` is negative.
    #[error("speed must not be negative")]
    NegativeSpeed,
    /// `daysLoop` is zero or negative.
    #[error("daysLoop must be positive")]
    BadDaysLoop,
}

/// How a layer is blended over the sky (OptiFine `Blender`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Blend {
    /// Alpha blending; the brightness scales the alpha.
    Alpha,
    /// Additive, scaled by alpha (the default).
    Add,
    /// The inverted destination times the source.
    Subtract,
    /// The destination times the source.
    Multiply,
    /// Plain addition.
    Dodge,
    /// The destination times the inverted source.
    Burn,
    /// Screen: source plus destination times the inverted source.
    Screen,
    /// Overlay: twice the product of source and destination.
    Overlay,
    /// No blending, with the alpha test.
    Replace,
}

impl Blend {
    /// Every mode, for the renderer to build one pipeline each.
    pub const ALL: [Self; 9] = [
        Self::Alpha,
        Self::Add,
        Self::Subtract,
        Self::Multiply,
        Self::Dodge,
        Self::Burn,
        Self::Screen,
        Self::Overlay,
        Self::Replace,
    ];

    /// The mode named `text`, case-insensitive. An unknown name is `Add`
    /// with a warning, as in OptiFine.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text.trim().to_lowercase().as_str() {
            "alpha" => Self::Alpha,
            "add" => Self::Add,
            "subtract" => Self::Subtract,
            "multiply" => Self::Multiply,
            "dodge" => Self::Dodge,
            "burn" => Self::Burn,
            "screen" => Self::Screen,
            "overlay" => Self::Overlay,
            "replace" => Self::Replace,
            unknown => {
                tracing::warn!(blend = unknown, "unknown sky blend, using add");
                Self::Add
            }
        }
    }

    /// The vertex colour the layer is drawn with for `brightness`, as
    /// `Blender.setupBlend` sets it: in the alpha for the modes that blend by
    /// alpha, in the colour for the others.
    #[must_use]
    pub fn color(self, brightness: f32) -> [f32; 4] {
        let b = brightness;
        match self {
            Self::Alpha | Self::Add | Self::Replace => [1.0, 1.0, 1.0, b],
            Self::Multiply => [b, b, b, b],
            Self::Subtract | Self::Dodge | Self::Burn | Self::Screen | Self::Overlay => {
                [b, b, b, 1.0]
            }
        }
    }
}

/// One sky layer.
#[derive(Debug, Clone, PartialEq)]
pub struct SkyLayer {
    /// The full virtual path of the 3×2 texture (`assets/minecraft/...`).
    pub source: String,
    /// The blend mode.
    pub blend: Blend,
    start_fade_in: i32,
    end_fade_in: i32,
    start_fade_out: i32,
    end_fade_out: i32,
    rotate: bool,
    speed: f32,
    /// The rotation axis, already permuted to `(z, y, -x)` as OptiFine does.
    axis: [f32; 3],
    days: Option<RangeList>,
    days_loop: i64,
    weather: WeatherSet,
    biomes: Option<BiomeFilter>,
    heights: Option<RangeList>,
    transition: f32,
}

impl SkyLayer {
    /// Reads the layer at the full virtual `path` (`.../sky<K>.properties`).
    ///
    /// # Errors
    ///
    /// [`SkyLayerError`] when the file does not parse, a required time is
    /// missing, the times do not cover one day, the speed is negative or
    /// `daysLoop` is not positive. Other bad values fall back to their
    /// defaults with a warning, as in OptiFine.
    pub fn parse(bytes: &[u8], path: &str) -> Result<Self, SkyLayerError> {
        let props = Properties::parse(bytes)?;
        let time = |key| props.get(key).and_then(parse_time);
        let (Some(start_fade_in), Some(end_fade_in), Some(end_fade_out)) =
            (time("startFadeIn"), time("endFadeIn"), time("endFadeOut"))
        else {
            return Err(SkyLayerError::MissingTimes);
        };
        let fade_in = normalize(end_fade_in - start_fade_in);
        let start_fade_out = time("startFadeOut").unwrap_or_else(|| {
            let derived = normalize(end_fade_out - fade_in);
            if between(derived, start_fade_in, end_fade_in) {
                end_fade_in
            } else {
                derived
            }
        });
        let total = fade_in
            + normalize(start_fade_out - end_fade_in)
            + normalize(end_fade_out - start_fade_out)
            + normalize(start_fade_in - end_fade_out);
        if total != DAY {
            return Err(SkyLayerError::NotOneDay(total));
        }
        let speed = parse_float(props.get("speed"), 1.0);
        if speed < 0.0 {
            return Err(SkyLayerError::NegativeSpeed);
        }
        let days_loop = props
            .get("daysLoop")
            .map_or(Ok(8), str::parse::<i64>)
            .unwrap_or(-1);
        if days_loop <= 0 {
            return Err(SkyLayerError::BadDaysLoop);
        }
        let default_source = path.strip_suffix(".properties").unwrap_or(path).to_string() + ".png";
        let source = props
            .get("source")
            .map_or(default_source, |source| resolve_path(source, path));
        Ok(Self {
            source,
            blend: props.get("blend").map_or(Blend::Add, Blend::parse),
            start_fade_in,
            end_fade_in,
            start_fade_out,
            end_fade_out,
            rotate: parse_bool(props.get("rotate"), true),
            speed,
            axis: parse_axis(props.get("axis")),
            days: props.get("days").and_then(|days| range_list(days, "days")),
            days_loop,
            weather: WeatherSet::parse(props.get("weather").unwrap_or("clear")),
            biomes: props.get("biomes").map(BiomeFilter::parse),
            heights: props.get("heights").and_then(|h| range_list(h, "heights")),
            transition: parse_float(props.get("transition"), 1.0),
        })
    }

    /// Whether the layer can show at `world_time` (`isActive`): outside its
    /// off span, and on one of its `days` when it has some.
    #[must_use]
    pub fn is_active(&self, world_time: i64) -> bool {
        let time_of_day = world_time.rem_euclid(i64::from(DAY)) as i32;
        if between(time_of_day, self.end_fade_out, self.start_fade_in) {
            return false;
        }
        let Some(days) = &self.days else {
            return true;
        };
        let cycle = i64::from(DAY) * self.days_loop;
        let since = (world_time - i64::from(self.start_fade_in)).rem_euclid(cycle);
        days.contains(since / i64::from(DAY))
    }

    /// The time-of-day factor (`getFadeBrightness`): ramps up over the fade
    /// in, holds at 1, ramps down over the fade out, 0 otherwise.
    #[must_use]
    pub fn fade_brightness(&self, time_of_day: i32) -> f32 {
        let ramp = |from: i32, to: i32| {
            normalize(time_of_day - from) as f32 / normalize(to - from).max(1) as f32
        };
        if between(time_of_day, self.start_fade_in, self.end_fade_in) {
            ramp(self.start_fade_in, self.end_fade_in)
        } else if between(time_of_day, self.end_fade_in, self.start_fade_out) {
            1.0
        } else if between(time_of_day, self.start_fade_out, self.end_fade_out) {
            1.0 - ramp(self.start_fade_out, self.end_fade_out)
        } else {
            0.0
        }
    }

    /// The weather factor (`getWeatherBrightness`). `thunder` is the thunder
    /// strength already divided by the rain strength, as `CustomSky` passes it.
    #[must_use]
    pub fn weather_brightness(&self, rain: f32, thunder: f32) -> f32 {
        let mut sum = 0.0;
        if self.weather.clear {
            sum += 1.0 - rain;
        }
        if self.weather.rain {
            sum += rain - thunder;
        }
        if self.weather.thunder {
            sum += thunder;
        }
        sum.clamp(0.0, 1.0)
    }

    /// Whether the layer depends on where the eye is (biomes or heights).
    #[must_use]
    pub fn depends_on_position(&self) -> bool {
        self.biomes.is_some() || self.heights.is_some()
    }

    /// The raw position factor (`getPositionBrightnessRaw`): 1 when the eye's
    /// biome and block height match, else 0.
    #[must_use]
    pub fn position_match(&self, biome_name: &str, eye_block_y: i64) -> f32 {
        let biome_ok = self
            .biomes
            .as_ref()
            .is_none_or(|filter| filter.matches(biome_name));
        let height_ok = self
            .heights
            .as_ref()
            .is_none_or(|heights| heights.contains(eye_block_y));
        if biome_ok && height_ok {
            1.0
        } else {
            0.0
        }
    }

    /// The rotation of the layer in degrees about [`SkyLayer::axis`], or
    /// `None` when the layer does not turn. A speed that is not a whole
    /// number adds the day's offset so the layer does not jump at midnight.
    #[must_use]
    pub fn rotation_degrees(&self, world_time: i64, celestial_angle: f32) -> Option<f32> {
        if !self.rotate {
            return None;
        }
        let mut offset = 0.0_f32;
        if self.speed != self.speed.round() {
            let day = (world_time + 18_000).div_euclid(i64::from(DAY));
            offset = ((day as f64 * f64::from(self.speed % 1.0)) % 1.0) as f32;
        }
        Some(360.0 * (offset + celestial_angle * self.speed))
    }

    /// The rotation axis (already in OptiFine's `(z, y, -x)` order).
    #[must_use]
    pub fn axis(&self) -> [f32; 3] {
        self.axis
    }
}

/// What a frame of the sky depends on.
#[derive(Debug, Clone, Copy)]
pub struct SkyInputs<'a> {
    /// The world time in ticks (it grows past 24000 every day).
    pub world_time: i64,
    /// The celestial angle of the frame, `0.0..1.0`.
    pub celestial_angle: f32,
    /// The rain strength, `0.0..=1.0`.
    pub rain: f32,
    /// The thunder strength, `0.0..=1.0`.
    pub thunder: f32,
    /// The display name of the biome at the eye ("Extreme Hills").
    pub biome_name: &'a str,
    /// The block height of the eye.
    pub eye_block_y: i64,
    /// Seconds since the previous frame, for the biome and height fades.
    pub frame_seconds: f32,
}

/// One layer to draw this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerFrame {
    /// The index of the layer in its dimension's list.
    pub index: usize,
    /// The blend mode.
    pub blend: Blend,
    /// The brightness, `1e-4..=1.0`.
    pub brightness: f32,
    /// The rotation in degrees about `axis`, if the layer turns.
    pub rotation: Option<f32>,
    /// The rotation axis.
    pub axis: [f32; 3],
}

/// The smoothed position factor of every layer, kept by the caller between
/// frames. `None` until a layer that depends on the position is first seen
/// (and again after [`SkyState::reset`], as OptiFine does on a world change).
#[derive(Debug, Clone, Default)]
pub struct SkyState {
    smoothed: Vec<Option<f32>>,
}

impl SkyState {
    /// Forgets the smoothing, for a new world or a new plan.
    pub fn reset(&mut self) {
        self.smoothed.clear();
    }
}

/// The layers of every dimension.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkyPlan {
    worlds: Vec<Vec<SkyLayer>>,
}

impl SkyPlan {
    /// Reads the layers of every dimension. `lookup` gives the full virtual
    /// path and the bytes of a pack-feature file from its relative path
    /// (`sky/world0/sky1.properties`), as `PackFiles::feature` does. In each
    /// dimension the scan stops at the first missing file; a file that does
    /// not parse is skipped with a warning.
    pub fn scan<'a>(lookup: impl Fn(&str) -> Option<(&'a str, &'a [u8])>) -> Self {
        let worlds = (0..DIMENSIONS)
            .map(|world| {
                (1..=MAX_LAYERS)
                    .map_while(|k| lookup(&format!("sky/world{world}/sky{k}.properties")))
                    .filter_map(|(path, bytes)| match SkyLayer::parse(bytes, path) {
                        Ok(layer) => Some(layer),
                        Err(error) => {
                            tracing::warn!(%path, %error, "sky layer skipped");
                            None
                        }
                    })
                    .collect()
            })
            .collect();
        Self { worlds }
    }

    /// Builds a plan from already parsed layers, one list per dimension.
    #[must_use]
    pub fn from_worlds(worlds: Vec<Vec<SkyLayer>>) -> Self {
        Self { worlds }
    }

    /// The layers of `dimension`; empty when it has none (the Nether).
    #[must_use]
    pub fn layers(&self, dimension: i32) -> &[SkyLayer] {
        usize::try_from(dimension)
            .ok()
            .and_then(|index| self.worlds.get(index))
            .map_or(&[], Vec::as_slice)
    }

    /// Whether any dimension has a layer.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.worlds.iter().all(Vec::is_empty)
    }

    /// Writes into `out` (cleared first) the layers of `dimension` to draw
    /// this frame, in draw order (`CustomSky.renderSky`). `state` carries the
    /// position smoothing between frames.
    pub fn evaluate(
        &self,
        dimension: i32,
        inputs: &SkyInputs<'_>,
        state: &mut SkyState,
        out: &mut Vec<LayerFrame>,
    ) {
        out.clear();
        let layers = self.layers(dimension);
        state.smoothed.resize(layers.len(), None);
        let time_of_day = inputs.world_time.rem_euclid(i64::from(DAY)) as i32;
        // `CustomSky.renderSky` hands the layers the thunder relative to the rain.
        let thunder = if inputs.rain > 0.0 {
            inputs.thunder / inputs.rain
        } else {
            inputs.thunder
        };
        for (index, (layer, smoothed)) in layers.iter().zip(&mut state.smoothed).enumerate() {
            if !layer.is_active(inputs.world_time) {
                continue;
            }
            let position = if layer.depends_on_position() {
                let raw = layer.position_match(inputs.biome_name, inputs.eye_block_y);
                let value = smoothed.map_or(raw, |previous| {
                    smooth(previous, raw, inputs.frame_seconds, layer.transition)
                });
                *smoothed = Some(value);
                value
            } else {
                1.0
            };
            let brightness = (position
                * layer.weather_brightness(inputs.rain, thunder)
                * layer.fade_brightness(time_of_day))
            .clamp(0.0, 1.0);
            if brightness < MIN_BRIGHTNESS {
                continue;
            }
            out.push(LayerFrame {
                index,
                blend: layer.blend,
                brightness,
                rotation: layer.rotation_degrees(inputs.world_time, inputs.celestial_angle),
                axis: layer.axis,
            });
        }
    }
}

/// One step of OptiFine's `SmoothFloat.getSmoothValue`: an ease toward
/// `target` that takes about `fade_seconds`, whatever the frame rate.
#[must_use]
pub fn smooth(previous: f32, target: f32, frame_seconds: f32, fade_seconds: f32) -> f32 {
    if frame_seconds <= 0.0 {
        return previous;
    }
    let delta = target - previous;
    if fade_seconds > 0.0 && frame_seconds < fade_seconds && delta.abs() > 1.0e-6 {
        let ratio = fade_seconds / frame_seconds;
        let factor = 4.61 - 1.0 / (0.13 + ratio / 10.0);
        previous + delta * (frame_seconds / fade_seconds * factor).clamp(0.0, 1.0)
    } else {
        target
    }
}

/// Resolves a texture path of a sky file against the file's full virtual
/// path, as `TextureUtils.fixResourcePath` does, with OptiFineDoc's rule that
/// a name without `/` sits next to the file.
#[must_use]
pub fn resolve_path(source: &str, properties_path: &str) -> String {
    const NAMESPACE: &str = "assets/minecraft/";
    let relative = properties_path
        .strip_prefix(NAMESPACE)
        .unwrap_or(properties_path);
    let folder = relative.rsplit_once('/').map_or("", |(folder, _)| folder);
    // `~/` names the feature folder the file itself is in.
    let feature_root = relative.split('/').next().unwrap_or("mcpatcher");
    let source = source.trim();
    let resolved = if let Some(rest) = source.strip_prefix(NAMESPACE) {
        rest.to_string()
    } else if let Some(rest) = source.strip_prefix("./") {
        format!("{folder}/{rest}")
    } else if let Some(rest) = source
        .strip_prefix("/~/")
        .or_else(|| source.strip_prefix("~/"))
    {
        format!("{feature_root}/{rest}")
    } else if !source.contains('/') {
        format!("{folder}/{source}")
    } else {
        source.trim_start_matches('/').to_string()
    };
    format!("{NAMESPACE}{resolved}")
}

/// Puts a tick count back into `0..24000`.
fn normalize(ticks: i32) -> i32 {
    ticks.rem_euclid(DAY)
}

/// `timeBetween`: whether `time` lies in `[start, end]`, wrapping past
/// midnight when `start > end`.
fn between(time: i32, start: i32, end: i32) -> bool {
    if start <= end {
        (start..=end).contains(&time)
    } else {
        time >= start || time <= end
    }
}

/// `HH:MM` to ticks: 06:00 is tick 0 (`parseTime`).
fn parse_time(text: &str) -> Option<i32> {
    let (hours, minutes) = text.trim().split_once(':')?;
    let hours: i32 = hours.trim().parse().ok().filter(|h| (0..=23).contains(h))?;
    let minutes: i32 = minutes
        .trim()
        .parse()
        .ok()
        .filter(|m| (0..=59).contains(m))?;
    Some((hours - 6).rem_euclid(24) * 1000 + (f64::from(minutes) / 60.0 * 1000.0) as i32)
}

fn parse_bool(text: Option<&str>, default: bool) -> bool {
    match text.map(str::to_lowercase).as_deref() {
        None => default,
        Some("true") => true,
        Some("false") => false,
        Some(other) => {
            tracing::warn!(value = other, "not a boolean, using the default");
            default
        }
    }
}

fn parse_float(text: Option<&str>, default: f32) -> f32 {
    text.map_or(default, |text| {
        text.trim().parse().unwrap_or_else(|_| {
            tracing::warn!(value = text, "not a number, using the default");
            default
        })
    })
}

/// `parseAxis`: three numbers in `-1..=1`, not all zero, returned as
/// `(z, y, -x)`. Anything else is the default `1 0 0`.
fn parse_axis(text: Option<&str>) -> [f32; 3] {
    const DEFAULT: [f32; 3] = [0.0, 0.0, -1.0];
    let Some(text) = text else {
        return DEFAULT;
    };
    let values: Vec<f32> = text
        .split(' ')
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect();
    match values[..] {
        [x, y, z]
            if [x, y, z].iter().all(|v| (-1.0..=1.0).contains(v))
                && x * x + y * y + z * z >= 1.0e-5 =>
        {
            [z, y, -x]
        }
        _ => {
            tracing::warn!(axis = text, "invalid sky axis, using 1 0 0");
            DEFAULT
        }
    }
}

fn range_list(text: &str, key: &str) -> Option<RangeList> {
    RangeList::parse(text)
        .map_err(|error| tracing::warn!(key, %error, "invalid range list, ignored"))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATH: &str = "assets/minecraft/optifine/sky/world0/sky1.properties";

    fn layer(text: &str) -> SkyLayer {
        SkyLayer::parse(text.as_bytes(), PATH).expect("valid layer")
    }

    /// Night stars: fade in 17:00-18:00, out 05:00-06:00.
    const NIGHT: &str =
        "startFadeIn=17:00\nendFadeIn=18:00\nstartFadeOut=05:00\nendFadeOut=06:00\n";

    fn ticks(hours: i32) -> i32 {
        (hours - 6).rem_euclid(24) * 1000
    }

    #[test]
    fn times_shift_so_six_am_is_tick_zero() {
        assert_eq!(parse_time("06:00"), Some(0));
        assert_eq!(parse_time("18:30"), Some(12_500));
        assert_eq!(parse_time("00:00"), Some(18_000));
        assert_eq!(parse_time("24:00"), None);
        assert_eq!(parse_time("7"), None);
    }

    #[test]
    fn the_fade_ramps_holds_and_wraps_past_midnight() {
        let night = layer(NIGHT);
        assert_eq!(night.fade_brightness(ticks(17)), 0.0);
        assert!((night.fade_brightness(ticks(17) + 500) - 0.5).abs() < 1e-6);
        assert_eq!(night.fade_brightness(ticks(23)), 1.0);
        assert_eq!(night.fade_brightness(ticks(2)), 1.0, "past midnight");
        assert!((night.fade_brightness(ticks(5) + 250) - 0.75).abs() < 1e-6);
        assert_eq!(night.fade_brightness(ticks(12)), 0.0);
        assert!(!night.is_active(i64::from(ticks(12))));
        assert!(night.is_active(i64::from(ticks(23))));
    }

    #[test]
    fn a_missing_fade_out_start_is_derived() {
        let derived = layer("startFadeIn=17:00\nendFadeIn=18:00\nendFadeOut=06:00\n");
        assert_eq!(derived, layer(NIGHT));
    }

    #[test]
    fn invalid_layers_are_rejected() {
        let parse = |text: &str| SkyLayer::parse(text.as_bytes(), PATH);
        assert_eq!(parse("startFadeIn=17:00"), Err(SkyLayerError::MissingTimes));
        assert_eq!(
            parse(&format!("{NIGHT}speed=-1")),
            Err(SkyLayerError::NegativeSpeed)
        );
        assert_eq!(
            parse(&format!("{NIGHT}daysLoop=0")),
            Err(SkyLayerError::BadDaysLoop)
        );
        assert!(matches!(
            parse("startFadeIn=17:00\nendFadeIn=18:00\nstartFadeOut=19:00\nendFadeOut=17:30\n"),
            Err(SkyLayerError::NotOneDay(_))
        ));
    }

    #[test]
    fn the_defaults_and_the_blend_names() {
        let plain = layer(NIGHT);
        assert_eq!(plain.blend, Blend::Add);
        assert_eq!(
            plain.source,
            "assets/minecraft/optifine/sky/world0/sky1.png"
        );
        assert_eq!(plain.axis(), [0.0, 0.0, -1.0]);
        assert_eq!(layer(&format!("{NIGHT}blend=Screen")).blend, Blend::Screen);
        assert_eq!(layer(&format!("{NIGHT}blend=sparkle")).blend, Blend::Add);
        assert_eq!(
            layer(&format!("{NIGHT}axis=0 1 0")).axis(),
            [0.0, 1.0, -0.0]
        );
        assert_eq!(
            layer(&format!("{NIGHT}axis=0 0 0")).axis(),
            [0.0, 0.0, -1.0]
        );
    }

    #[test]
    fn days_select_the_days_of_the_loop() {
        let every_other = layer(&format!("{NIGHT}days=0\ndaysLoop=2\n"));
        let first_night = i64::from(ticks(23));
        assert!(every_other.is_active(first_night));
        assert!(!every_other.is_active(first_night + 24_000));
        assert!(every_other.is_active(first_night + 48_000));
    }

    #[test]
    fn the_weather_factor_mixes_the_strengths() {
        let clear = layer(NIGHT);
        assert_eq!(clear.weather_brightness(0.0, 0.0), 1.0);
        assert!((clear.weather_brightness(0.25, 0.0) - 0.75).abs() < 1e-6);
        let storm = layer(&format!("{NIGHT}weather=rain thunder"));
        assert_eq!(storm.weather_brightness(1.0, 0.5), 1.0);
        let thunder = layer(&format!("{NIGHT}weather=thunder"));
        assert_eq!(thunder.weather_brightness(1.0, 0.5), 0.5);
    }

    #[test]
    fn a_fractional_speed_adds_the_day_offset() {
        let half = layer(&format!("{NIGHT}speed=0.5"));
        let day_one = 24_000 * 2 + 6_000;
        let degrees = half.rotation_degrees(day_one, 0.0).expect("turns");
        assert!((degrees - 180.0).abs() < 1e-3, "{degrees}");
        let whole = layer(NIGHT);
        assert_eq!(whole.rotation_degrees(day_one, 0.25), Some(90.0));
        assert_eq!(
            layer(&format!("{NIGHT}rotate=false")).rotation_degrees(0, 0.5),
            None
        );
    }

    #[test]
    fn paths_resolve_like_optifine() {
        let path = "assets/minecraft/mcpatcher/sky/world0/sky1.properties";
        assert_eq!(
            resolve_path("stars.png", path),
            "assets/minecraft/mcpatcher/sky/world0/stars.png"
        );
        assert_eq!(
            resolve_path("./a/b.png", path),
            "assets/minecraft/mcpatcher/sky/world0/a/b.png"
        );
        assert_eq!(
            resolve_path("~/sky/shared.png", path),
            "assets/minecraft/mcpatcher/sky/shared.png"
        );
        assert_eq!(
            resolve_path("assets/minecraft/textures/x.png", path),
            "assets/minecraft/textures/x.png"
        );
        assert_eq!(
            resolve_path("textures/environment/sun.png", path),
            "assets/minecraft/textures/environment/sun.png"
        );
    }

    #[test]
    fn the_scan_stops_at_the_first_gap_and_skips_bad_files() {
        let files = [
            ("sky/world0/sky1.properties", NIGHT),
            ("sky/world0/sky2.properties", "broken=1"),
            ("sky/world0/sky3.properties", NIGHT),
            ("sky/world0/sky5.properties", NIGHT),
            ("sky/world1/sky1.properties", NIGHT),
        ];
        let lookup = |relative: &str| {
            files
                .iter()
                .find(|(path, _)| *path == relative)
                .map(|(path, text)| (*path, text.as_bytes()))
        };
        let plan = SkyPlan::scan(lookup);
        assert_eq!(plan.layers(0).len(), 2, "sky2 skipped, sky5 after a gap");
        assert_eq!(plan.layers(1).len(), 1);
        assert!(plan.layers(-1).is_empty());
        assert!(!plan.is_empty());
    }

    #[test]
    fn evaluate_reuses_its_buffers_and_smooths_the_position() {
        let plan = SkyPlan::from_worlds(vec![vec![
            layer(NIGHT),
            layer(&format!("{NIGHT}biomes=Desert\ntransition=1\nblend=alpha")),
        ]]);
        let mut state = SkyState::default();
        let mut out = Vec::with_capacity(4);
        let mut inputs = SkyInputs {
            world_time: i64::from(ticks(23)),
            celestial_angle: 0.6,
            rain: 0.0,
            thunder: 0.0,
            biome_name: "Desert",
            eye_block_y: 64,
            frame_seconds: 0.05,
        };
        plan.evaluate(0, &inputs, &mut state, &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].blend, Blend::Alpha);
        inputs.biome_name = "Plains";
        plan.evaluate(0, &inputs, &mut state, &mut out);
        let fading = out
            .iter()
            .find(|frame| frame.index == 1)
            .expect("still fading");
        assert!(fading.brightness > 0.5 && fading.brightness < 1.0);
        let capacity = out.capacity();
        for _ in 0..200 {
            plan.evaluate(0, &inputs, &mut state, &mut out);
        }
        assert_eq!(out.len(), 1, "faded out");
        assert_eq!(out.capacity(), capacity, "no growth");
    }

    #[test]
    fn smoothing_reaches_the_target_and_ignores_zero_steps() {
        assert_eq!(smooth(0.0, 1.0, 0.0, 1.0), 0.0);
        assert_eq!(smooth(0.0, 1.0, 2.0, 1.0), 1.0);
        let step = smooth(0.0, 1.0, 0.1, 1.0);
        assert!(step > 0.0 && step < 1.0);
    }

    #[test]
    fn blend_colours_follow_the_blender() {
        assert_eq!(Blend::Alpha.color(0.5), [1.0, 1.0, 1.0, 0.5]);
        assert_eq!(Blend::Multiply.color(0.5), [0.5; 4]);
        assert_eq!(Blend::Screen.color(0.5), [0.5, 0.5, 0.5, 1.0]);
    }

    proptest::proptest! {
        #[test]
        fn arbitrary_layer_files_never_panic(bytes in proptest::collection::vec(proptest::num::u8::ANY, 0..512)) {
            let _ = SkyLayer::parse(&bytes, PATH);
        }
    }
}

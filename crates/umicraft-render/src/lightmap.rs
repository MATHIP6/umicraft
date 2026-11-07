// SPDX-License-Identifier: MIT OR Apache-2.0

//! The light colour of every (sky, block) light pair, as vanilla builds it
//! once per frame (MCP `EntityRenderer.updateLightmap`).
//!
//! The reference fills a 16 x 16 texture (`i / 16` is the sky level, `i % 16`
//! the block level) and every lit surface samples it with the light levels of
//! its vertices, bilinearly (`GL_LINEAR`, texel centres at `(level + 0.5) / 16`).
//! The shaders do the same with the 256 colours of [`colors`], which sit in the
//! camera uniform. The model is not a scalar: torch light is warm (the blue
//! channel falls faster), block light is brighter than its table value (`1.5`
//! plus the flicker), the night sky is bluish, and nothing is darker than
//! `0.03`.

use crate::environment::RenderEnvironment;
use crate::fog::night_vision_brightness;

/// Levels per axis of the lightmap.
const LEVELS: usize = 16;
/// Entries of the lightmap.
pub(crate) const ENTRIES: usize = LEVELS * LEVELS;

/// What the lightmap depends on, from the world and the options.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LightmapParams {
    /// `World.getSunBrightness`, in `0.2..=1.0`.
    pub sun_brightness: f32,
    /// `EntityRenderer.torchFlickerX`.
    pub torch_flicker: f32,
    /// A lightning flash is on (`World.getLastLightningBolt() > 0`).
    pub lightning: bool,
    /// The dimension (`-1` Nether, `1` End, anything else the overworld).
    pub dimension: i8,
    /// `getNightVisionBrightness` while the effect runs.
    pub night_vision: Option<f32>,
    /// The brightness option, `0..=1`.
    pub gamma: f32,
}

impl LightmapParams {
    /// The parameters of a frame.
    pub(crate) fn of(environment: &RenderEnvironment, sun_brightness: f32, flicker: f32) -> Self {
        Self {
            sun_brightness,
            torch_flicker: flicker,
            lightning: environment.lightning_ticks > 0,
            dimension: environment.dimension,
            night_vision: environment
                .fog
                .night_vision_ticks
                .map(|ticks| night_vision_brightness(ticks, environment.partial_tick)),
            gamma: environment.gamma,
        }
    }
}

/// `WorldProvider.generateLightBrightnessTable`; the Nether raises the floor
/// to `0.1` (`WorldProviderHell`).
fn brightness(level: usize, dimension: i8) -> f32 {
    let floor = if dimension == -1 { 0.1 } else { 0.0 };
    let dark = 1.0 - level as f32 / 15.0;
    (1.0 - dark) / (dark * 3.0 + 1.0) * (1.0 - floor) + floor
}

/// The colour of one entry, RGB in `0..=1`.
fn entry(params: &LightmapParams, sky: usize, block: usize) -> [f32; 3] {
    let sun = params.sun_brightness;
    let mut sky_light = brightness(sky, params.dimension) * (sun * 0.95 + 0.05);
    let block_light = brightness(block, params.dimension) * (params.torch_flicker * 0.1 + 1.5);
    if params.lightning {
        sky_light = brightness(sky, params.dimension);
    }
    let sky_tint = sky_light * (sun * 0.65 + 0.35);
    let warm_green = block_light * ((block_light * 0.6 + 0.4) * 0.6 + 0.4);
    let warm_blue = block_light * (block_light * block_light * 0.6 + 0.4);
    let mut color = [
        sky_tint + block_light,
        sky_tint + warm_green,
        sky_light + warm_blue,
    ]
    .map(|channel| channel * 0.96 + 0.03);
    if params.dimension == 1 {
        color = [
            0.22 + block_light * 0.75,
            0.28 + warm_green * 0.75,
            0.25 + warm_blue * 0.75,
        ];
    }
    if let Some(strength) = params.night_vision {
        // The channel that would reach 1 first sets the lift; a black channel
        // has no finite one and keeps its colour (the reference divides by 0).
        let lift = color
            .iter()
            .filter(|channel| **channel > 0.0)
            .map(|channel| 1.0 / channel)
            .fold(f32::INFINITY, f32::min);
        if lift.is_finite() {
            color = color.map(|channel| channel * (1.0 - strength) + channel * lift * strength);
        }
    }
    color.map(|channel| {
        let channel = channel.min(1.0);
        let inverse = 1.0 - channel;
        let lifted = 1.0 - inverse * inverse * inverse * inverse;
        let gamma = channel * (1.0 - params.gamma) + lifted * params.gamma;
        (gamma * 0.96 + 0.03).clamp(0.0, 1.0)
    })
}

/// The 256 light colours, `[sky * 16 + block]` as RGBA with alpha `1`.
pub(crate) fn colors(params: &LightmapParams) -> [[f32; 4]; ENTRIES] {
    let mut colors = [[0.0; 4]; ENTRIES];
    for sky in 0..LEVELS {
        for block in 0..LEVELS {
            let [r, g, b] = entry(params, sky, block);
            colors[sky * LEVELS + block] = [r, g, b, 1.0];
        }
    }
    colors
}

/// Every entry white: the GUI draws items without the lightmap.
pub(crate) const fn unlit() -> [[f32; 4]; ENTRIES] {
    [[1.0; 4]; ENTRIES]
}

/// `EntityRenderer.updateTorchFlicker`, run once per client tick: a damped
/// random walk that makes torch light waver.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TorchFlicker {
    value: f32,
    velocity: f32,
    random: u64,
    tick: Option<u64>,
}

impl TorchFlicker {
    pub(crate) fn new() -> Self {
        Self {
            value: 0.0,
            velocity: 0.0,
            random: 0x9E37_79B9_7F4A_7C15,
            tick: None,
        }
    }

    /// `Math.random()`: a xorshift64* in `0..1`. The wobble only needs to look
    /// random, not to be unpredictable.
    fn next(&mut self) -> f32 {
        self.random ^= self.random >> 12;
        self.random ^= self.random << 25;
        self.random ^= self.random >> 27;
        let bits = self.random.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40;
        bits as f32 / (1_u64 << 24) as f32
    }

    /// Runs the ticks up to `tick` (at most a few: a stalled frame must not
    /// replay seconds of flicker) and returns the flicker.
    pub(crate) fn at(&mut self, tick: u64) -> f32 {
        const MAX_CATCH_UP: u64 = 4;
        let from = self.tick.replace(tick).unwrap_or(tick);
        for _ in 0..tick.saturating_sub(from).min(MAX_CATCH_UP) {
            let step = (self.next() - self.next()) * self.next() * self.next();
            self.velocity = (self.velocity + step) * 0.9;
            // `torchFlickerX += (torchFlickerDX - torchFlickerX) * 1.0`.
            self.value = self.velocity;
        }
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RenderEnvironment;

    fn day() -> LightmapParams {
        LightmapParams {
            sun_brightness: 1.0,
            torch_flicker: 0.0,
            lightning: false,
            dimension: 0,
            night_vision: None,
            gamma: 0.0,
        }
    }

    fn at(colors: &[[f32; 4]; ENTRIES], sky: usize, block: usize) -> [f32; 3] {
        let [r, g, b, _] = colors[sky * LEVELS + block];
        [r, g, b]
    }

    #[test]
    fn full_light_is_the_brightest_entry_and_just_under_one() {
        let colors = colors(&day());
        // `1 * 0.96 + 0.03`: even full light is not pure white in vanilla.
        for channel in at(&colors, 15, 15) {
            assert!((channel - 0.99).abs() < 1e-4, "{channel}");
        }
    }

    #[test]
    fn darkness_keeps_the_floor_of_three_hundredths() {
        let colors = colors(&day());
        for channel in at(&colors, 0, 0) {
            assert!((channel - (0.03 * 0.96 + 0.03)).abs() < 1e-4, "{channel}");
        }
    }

    #[test]
    fn torch_light_is_warm_and_brighter_than_its_table_value() {
        let colors = colors(&day());
        // Level 7: the table value is 0.18, the 1.5 factor makes it about 0.27
        // and the warm curves drop green and blue below red.
        let [r, g, b] = at(&colors, 0, 7);
        assert!(r > g && g > b, "{r} {g} {b}");
        assert!(r > 0.25, "{r}");
        // A strong torch saturates every channel (`min(1, ..)`).
        let [r, g, b] = at(&colors, 0, 14);
        assert!(r == g && g == b, "{r} {g} {b}");
    }

    #[test]
    fn night_sky_light_is_bluish() {
        let night = LightmapParams {
            sun_brightness: 0.2,
            ..day()
        };
        let [r, g, b] = at(&colors(&night), 15, 0);
        assert!(b > r && (r - g).abs() < 1e-6, "{r} {g} {b}");
    }

    #[test]
    fn the_nether_never_goes_fully_dark() {
        let nether = LightmapParams {
            dimension: -1,
            ..day()
        };
        let floor = at(&colors(&nether), 0, 0)[0];
        let overworld = at(&colors(&day()), 0, 0)[0];
        assert!(floor > overworld + 0.05, "{floor} {overworld}");
    }

    #[test]
    fn lightning_lights_the_sky_at_full_strength() {
        let night = LightmapParams {
            sun_brightness: 0.2,
            ..day()
        };
        let flash = LightmapParams {
            lightning: true,
            ..night
        };
        assert!(at(&colors(&flash), 15, 0)[0] > at(&colors(&night), 15, 0)[0]);
    }

    #[test]
    fn gamma_lifts_the_dark_entries() {
        let bright = LightmapParams {
            gamma: 1.0,
            ..day()
        };
        assert!(at(&colors(&bright), 5, 0)[0] > at(&colors(&day()), 5, 0)[0]);
    }

    #[test]
    fn night_vision_lifts_the_dark_entries_toward_white() {
        let seeing = LightmapParams {
            night_vision: Some(1.0),
            ..day()
        };
        let dark = at(&colors(&seeing), 0, 0);
        assert!(dark.iter().all(|c| *c > 0.9), "{dark:?}");
    }

    /// A lit wall of the chunk (0, -1) five blocks ahead of a camera at
    /// (8, 80, 0), rendered with the light `(block, sky)`; the middle pixel.
    fn wall_pixel(block: u8, sky: u8) -> [u8; 3] {
        use crate::{Camera, Overlay, Renderer};
        use glam::Vec3;
        use umicraft_mesh::{Mesh, Vertex, VertexLight};
        use umicraft_world::ChunkPos;

        let light = VertexLight::new(block, sky, VertexLight::AO_OPEN, false).pack();
        let vertex = |x: f32, y: f32, uv: [f32; 2]| Vertex {
            position: [x, y, -5.0],
            normal: [0.0, 0.0, 1.0],
            uv,
            layer: 0,
            light,
            tint: umicraft_mesh::tint::WHITE,
        };
        let wall = Mesh {
            vertices: vec![
                vertex(0.0, 70.0, [0.0, 1.0]),
                vertex(16.0, 70.0, [1.0, 1.0]),
                vertex(16.0, 90.0, [1.0, 0.0]),
                vertex(0.0, 90.0, [0.0, 0.0]),
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
            ..Mesh::default()
        };
        let mut renderer = Renderer::new_offscreen(64, 48).expect("offscreen renderer");
        renderer.upload_mesh(ChunkPos::new(0, -1), &wall);
        let mut camera = Camera::new(Vec3::new(8.0, 80.0, 0.0), 0.0, 0.0);
        camera.set_aspect(64, 48);
        renderer
            .render(&camera, &Overlay::new(64, 48))
            .expect("render");
        let image = renderer.read_pixels().expect("pixels");
        let at = ((24 * image.width + 32) * 4) as usize;
        [image.rgba[at], image.rgba[at + 1], image.rgba[at + 2]]
    }

    /// The shader reads the same colours as [`colors`]: a wall lit by a torch
    /// at level 7 is the fully lit wall times the ratio of the two entries.
    #[test]
    #[ignore = "requires a GPU adapter"]
    fn the_shader_samples_the_lightmap_the_cpu_built() {
        let environment = RenderEnvironment::DEFAULT;
        let sun = crate::SkyState::at(environment).sun_brightness;
        let table = colors(&LightmapParams::of(&environment, sun, 0.0));
        let full = wall_pixel(15, 15);
        let torch = wall_pixel(7, 0);
        let (lit, dim) = (at(&table, 15, 15), at(&table, 0, 7));
        for channel in 0..3 {
            let expected = f32::from(full[channel]) * dim[channel] / lit[channel];
            let got = f32::from(torch[channel]);
            assert!(
                (got - expected).abs() < 4.0,
                "channel {channel}: {got} vs {expected}"
            );
        }
        // And the torch light is warm: red over green over blue in the ratio.
        let ratio = |channel: usize| f32::from(torch[channel]) / f32::from(full[channel].max(1));
        assert!(ratio(0) > ratio(2), "{torch:?} vs {full:?}");
    }

    #[test]
    fn the_flicker_is_a_damped_walk_that_a_stall_does_not_replay() {
        let mut flicker = TorchFlicker::new();
        assert_eq!(flicker.at(10), 0.0, "the first frame runs no tick");
        let mut moved = false;
        for tick in 11..60 {
            moved |= flicker.at(tick) != 0.0;
        }
        assert!(moved);
        let mut stalled = flicker;
        let mut stepped = flicker;
        // A jump of 1000 ticks runs as many as four, like four single ticks.
        let jump = stalled.at(1_059);
        for tick in 60..=63 {
            stepped.at(tick);
        }
        assert_eq!(jump, stepped.value);
    }
}

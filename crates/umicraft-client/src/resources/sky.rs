// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sky pictures of the active packs and the user's client jar (P7e).
//!
//! `sun.png`, `moon_phases.png`, `end_sky.png`, `clouds.png`, `rain.png`, `snow.png`, the particle sheet
//! and the overlay pictures (`underwater.png`, `pumpkinblur.png`, `vignette.png`) are read at runtime
//! through [`GameFiles`] (packs first, then the verified jar) and never stored in the repository
//! (spec §3). A picture that neither has, or that does not decode, stays `None` and the renderer
//! draws its original stand-in.

use umicraft_game::debug_f3::biomes::biome_name;
use umicraft_packext::sky::{LayerFrame, SkyInputs, SkyPlan, SkyState, DIMENSIONS};
use umicraft_render::{SkyImage, SkyTextures};

use super::game_files::GameFiles;

/// Jar path of the sun.
const SUN: &str = "assets/minecraft/textures/environment/sun.png";
/// Jar path of the eight phases of the moon.
const MOON_PHASES: &str = "assets/minecraft/textures/environment/moon_phases.png";
/// Jar path of the End sky.
const END_SKY: &str = "assets/minecraft/textures/environment/end_sky.png";

/// Jar path of the clouds.
const CLOUDS: &str = "assets/minecraft/textures/environment/clouds.png";

/// Jar path of the world border's force field.
const FORCEFIELD: &str = "assets/minecraft/textures/misc/forcefield.png";

/// Reads and decodes one picture; `None` when absent or malformed.
fn read_image(files: &GameFiles, path: &str) -> Option<SkyImage> {
    let (rgba, width, height) = files.read_png(path)?;
    Some(SkyImage {
        width,
        height,
        rgba,
    })
}

/// Reads the world border picture; `None` keeps the renderer's original
/// fallback.
#[must_use]
pub fn load_border_texture(files: &GameFiles) -> Option<SkyImage> {
    read_image(files, FORCEFIELD)
}

/// Jar path of the rain streaks.
const RAIN: &str = "assets/minecraft/textures/environment/rain.png";
/// Jar path of the snowflakes.
const SNOW: &str = "assets/minecraft/textures/environment/snow.png";

/// Jar path of the particle sheet.
const PARTICLES: &str = "assets/minecraft/textures/particle/particles.png";

/// Jar path of the veil seen under water.
const UNDERWATER: &str = "assets/minecraft/textures/misc/underwater.png";
/// Jar path of the pumpkin worn on the head.
const PUMPKIN_BLUR: &str = "assets/minecraft/textures/misc/pumpkinblur.png";
/// Jar path of the shade of the screen edges.
const VIGNETTE: &str = "assets/minecraft/textures/misc/vignette.png";

/// Reads the sky pictures; every missing or undecodable one is `None`.
#[must_use]
pub fn load_sky_textures(files: &GameFiles) -> SkyTextures {
    let read = |path: &str| read_image(files, path);
    SkyTextures {
        sun: read(SUN),
        moon_phases: read(MOON_PHASES),
        end_sky: read(END_SKY),
        clouds: read(CLOUDS),
        rain: read(RAIN),
        snow: read(SNOW),
        particles: read(PARTICLES),
        underwater: read(UNDERWATER),
        pumpkin_blur: read(PUMPKIN_BLUR),
        vignette: read(VIGNETTE),
    }
}

/// What one frame tells the custom sky.
#[derive(Debug, Clone, Copy)]
pub struct CustomSkyFrame {
    /// The dimension id of the world.
    pub dimension: i32,
    /// The world clock in ticks.
    pub world_time: i64,
    /// `World.getCelestialAngle` for the frame.
    pub celestial_angle: f32,
    /// Interpolated rain strength.
    pub rain: f32,
    /// Interpolated thunder strength.
    pub thunder: f32,
    /// The biome id at the camera, `None` while its column is not loaded.
    pub biome_id: Option<u8>,
    /// The block row of the eye.
    pub eye_block_y: i64,
    /// Seconds since the previous frame (drives the biome/height fade).
    pub frame_seconds: f32,
}

/// The custom sky of the active packs (`sky/world<N>/sky<K>.properties`,
/// ADR-0089): the parsed layers, the smoothing state and the buffers the
/// per-frame evaluation reuses.
#[derive(Debug, Default)]
pub struct CustomSky {
    plan: SkyPlan,
    /// For each dimension, the index of its first picture in the flat list
    /// handed to the renderer.
    offsets: Vec<usize>,
    state: SkyState,
    frames: Vec<LayerFrame>,
    /// The last biome looked up, so the name is built only when it changes.
    biome: Option<(u8, String)>,
}

impl CustomSky {
    /// Scans the packs for sky layers and decodes their pictures. A layer
    /// whose picture is missing or does not decode is dropped with a warning
    /// (as OptiFine skips it). Returns the sky and the pictures, in the flat
    /// order `Renderer::set_custom_sky_textures` expects.
    #[must_use]
    pub fn load(files: &GameFiles) -> (Self, Vec<SkyImage>) {
        let scanned = SkyPlan::scan(|relative| {
            files
                .packs()
                .feature(relative)
                .map(|found| (found.path, found.bytes))
        });
        let mut images = Vec::new();
        let mut offsets = Vec::with_capacity(DIMENSIONS);
        let mut worlds = Vec::with_capacity(DIMENSIONS);
        for dimension in 0..DIMENSIONS as i32 {
            offsets.push(images.len());
            let kept = scanned
                .layers(dimension)
                .iter()
                .filter_map(|layer| {
                    let Some(image) = read_image(files, &layer.source) else {
                        tracing::warn!(source = %layer.source, "sky layer texture not found, skipped");
                        return None;
                    };
                    images.push(image);
                    Some(layer.clone())
                })
                .collect();
            worlds.push(kept);
        }
        let sky = Self {
            plan: SkyPlan::from_worlds(worlds),
            offsets,
            ..Self::default()
        };
        (sky, images)
    }

    /// Evaluates the layers of this frame. Returns whether the dimension has
    /// any layer (they replace the stars even when faded out) and the layers
    /// to draw, with indices into the uploaded pictures. `enabled` is the
    /// Custom Sky option: off gives the vanilla sky.
    pub fn evaluate(&mut self, enabled: bool, frame: &CustomSkyFrame) -> (bool, &[LayerFrame]) {
        self.frames.clear();
        let layers = self.plan.layers(frame.dimension);
        if !enabled || layers.is_empty() {
            self.state.reset();
            return (false, &self.frames);
        }
        self.refresh_biome(frame.biome_id);
        let name = self.biome.as_ref().map_or("", |(_, name)| name.as_str());
        let inputs = SkyInputs {
            world_time: frame.world_time,
            celestial_angle: frame.celestial_angle,
            rain: frame.rain,
            thunder: frame.thunder,
            biome_name: name,
            eye_block_y: frame.eye_block_y,
            frame_seconds: frame.frame_seconds,
        };
        self.plan
            .evaluate(frame.dimension, &inputs, &mut self.state, &mut self.frames);
        let offset = usize::try_from(frame.dimension)
            .ok()
            .and_then(|dimension| self.offsets.get(dimension))
            .copied()
            .unwrap_or(0);
        for layer in &mut self.frames {
            layer.index += offset;
        }
        (true, &self.frames)
    }

    /// Looks the display name of `id` up when the biome changed; an unknown
    /// biome has no name, so it matches no biome list.
    fn refresh_biome(&mut self, id: Option<u8>) {
        let Some(id) = id else {
            self.biome = None;
            return;
        };
        if self.biome.as_ref().is_none_or(|(cached, _)| *cached != id) {
            self.biome = Some((id, biome_name(id).unwrap_or_default()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_jar_every_picture_is_left_to_the_fallback() {
        let textures = load_sky_textures(&GameFiles::default());
        assert!(textures.sun.is_none());
        assert!(textures.moon_phases.is_none());
        assert!(textures.end_sky.is_none());
        assert!(textures.clouds.is_none());
        assert!(textures.rain.is_none());
        assert!(textures.snow.is_none());
        assert!(textures.particles.is_none());
        assert!(textures.underwater.is_none());
        assert!(textures.pumpkin_blur.is_none());
        assert!(textures.vignette.is_none());
        assert!(load_border_texture(&GameFiles::default()).is_none());
    }

    /// Regression: the sky pictures were read from the jar alone, so a pack's
    /// `sun.png` was ignored.
    #[test]
    fn a_pack_sun_is_used() {
        let files = crate::resources::game_files::test_support::files_with(
            SUN,
            crate::resources::game_files::test_support::png(32, 32),
        );
        let textures = load_sky_textures(&files);
        assert_eq!(
            textures.sun.map(|sun| (sun.width, sun.height)),
            Some((32, 32))
        );
        assert!(textures.moon_phases.is_none());
    }

    use std::collections::BTreeMap;
    use umicraft_assets::{PackFiles, ResourcePack};

    use crate::resources::game_files::test_support::png;

    const ROOT: &str = "assets/minecraft/mcpatcher/sky/world0/";
    const DAYLONG: &str = "startFadeIn=06:00\nendFadeIn=07:00\nendFadeOut=05:00\n";

    fn pack_files(entries: &[(&str, Vec<u8>)]) -> GameFiles {
        let files = entries
            .iter()
            .map(|(name, bytes)| (format!("{ROOT}{name}"), bytes.clone()))
            .collect::<BTreeMap<_, _>>();
        let pack = ResourcePack::from_parts("sky", None, None, files);
        GameFiles::new(PackFiles::from_packs([pack]), None)
    }

    fn frame(dimension: i32, biome_id: Option<u8>) -> CustomSkyFrame {
        CustomSkyFrame {
            dimension,
            world_time: 6_000,
            celestial_angle: 0.0,
            rain: 0.0,
            thunder: 0.0,
            biome_id,
            eye_block_y: 70,
            frame_seconds: 0.016,
        }
    }

    #[test]
    fn layers_load_with_their_pictures_and_a_missing_picture_drops_the_layer() {
        let files = pack_files(&[
            ("sky1.properties", DAYLONG.as_bytes().to_vec()),
            ("sky1.png", png(3, 2)),
            ("sky2.properties", DAYLONG.as_bytes().to_vec()),
            ("sky3.properties", DAYLONG.as_bytes().to_vec()),
            ("sky3.png", png(6, 4)),
        ]);
        let (mut sky, images) = CustomSky::load(&files);
        // sky2 has no picture; scanning continues past it because the
        // properties file exists.
        assert_eq!(images.len(), 2);
        let (has, frames) = sky.evaluate(true, &frame(0, Some(1)));
        assert!(has);
        assert_eq!(frames.iter().map(|f| f.index).collect::<Vec<_>>(), [0, 1]);
    }

    #[test]
    fn the_option_off_gives_the_vanilla_sky() {
        let files = pack_files(&[
            ("sky1.properties", DAYLONG.as_bytes().to_vec()),
            ("sky1.png", png(3, 2)),
        ]);
        let (mut sky, _) = CustomSky::load(&files);
        let (has, frames) = sky.evaluate(false, &frame(0, None));
        assert!(!has && frames.is_empty());
    }

    #[test]
    fn a_dimension_without_layers_is_untouched() {
        let files = pack_files(&[
            ("sky1.properties", DAYLONG.as_bytes().to_vec()),
            ("sky1.png", png(3, 2)),
        ]);
        let (mut sky, _) = CustomSky::load(&files);
        assert!(!sky.evaluate(true, &frame(-1, None)).0);
        assert!(!sky.evaluate(true, &frame(2, None)).0);
    }

    #[test]
    fn no_pack_means_no_custom_sky() {
        let (mut sky, images) = CustomSky::load(&GameFiles::default());
        assert!(images.is_empty());
        assert!(!sky.evaluate(true, &frame(0, Some(1))).0);
    }

    #[test]
    fn a_biome_list_follows_the_camera_biome() {
        let props = format!("{DAYLONG}biomes=Desert\ntransition=0\n");
        let files = pack_files(&[
            ("sky1.properties", props.into_bytes()),
            ("sky1.png", png(3, 2)),
        ]);
        let (mut sky, _) = CustomSky::load(&files);
        // Biome 2 is Desert, 1 is Plains (F3 names).
        assert_eq!(sky.evaluate(true, &frame(0, Some(2))).1.len(), 1);
        assert!(sky.evaluate(true, &frame(0, Some(1))).1.is_empty());
        assert!(sky.evaluate(true, &frame(0, None)).1.is_empty());
    }
}

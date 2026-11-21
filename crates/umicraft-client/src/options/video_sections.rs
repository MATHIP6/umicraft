// SPDX-License-Identifier: MIT OR Apache-2.0

//! The OptiFine-style video subsections (ADR-0089, phase 2):
//! `[video.details]`, `[video.quality]` and `[video.animations]` in
//! `options.toml`.
//!
//! Every field has a default. A file written before these sections existed
//! loads with the defaults, and a section that does not parse falls back to
//! its defaults without touching the rest of `[video]`.
//!
//! Defaults follow OptiFine, except where a faster choice costs nothing or
//! almost nothing on screen. The deviations are listed in
//! `docs/tasks/optifine/README.md`, section "Defaults"; here the only one is
//! Trees Smart.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

/// Reads a subsection, or its defaults when it does not parse, so one bad
/// subsection does not cost the rest of `[video]`.
pub(super) fn lenient<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned + Default,
{
    let value = toml::Value::deserialize(deserializer)?;
    Ok(value.try_into().unwrap_or_else(|error| {
        tracing::warn!(%error, "a video subsection does not parse; using its defaults");
        T::default()
    }))
}

/// A value that can follow the Graphics option: OptiFine's
/// Default / Fast / Fancy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphicsChoice {
    /// Follow Graphics (Fast or Fancy).
    #[default]
    Default,
    /// Always the fast version.
    Fast,
    /// Always the fancy version.
    Fancy,
}

impl GraphicsChoice {
    /// The choices in cycle order.
    pub const ALL: [Self; 3] = [Self::Default, Self::Fast, Self::Fancy];

    /// Whether the fancy version is drawn, given the Graphics option.
    #[must_use]
    pub fn is_fancy(self, fancy_graphics: bool) -> bool {
        match self {
            Self::Default => fancy_graphics,
            Self::Fast => false,
            Self::Fancy => true,
        }
    }
}

/// How leaves are drawn (OptiFine's "Trees").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TreesChoice {
    /// Follow Graphics: Fast or Fancy.
    Default,
    /// Opaque leaves; the faces between two leaf blocks are culled.
    Fast,
    /// Transparent leaves; the faces between two leaf blocks are culled.
    /// Close to Fancy at the cost of Fast (the Umicraft default).
    #[default]
    Smart,
    /// Transparent leaves with every face drawn (vanilla fancy).
    Fancy,
}

/// The leaf rendering a [`TreesChoice`] resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeavesMode {
    /// Opaque, inner faces culled.
    Fast,
    /// Cut out, inner faces culled.
    Smart,
    /// Cut out, every face drawn.
    Fancy,
}

impl TreesChoice {
    /// The choices in cycle order, as in OptiFine.
    pub const ALL: [Self; 4] = [Self::Default, Self::Fast, Self::Smart, Self::Fancy];

    /// The leaf rendering, given the Graphics option.
    #[must_use]
    pub fn resolve(self, fancy_graphics: bool) -> LeavesMode {
        match self {
            Self::Default if fancy_graphics => LeavesMode::Fancy,
            Self::Default | Self::Fast => LeavesMode::Fast,
            Self::Smart => LeavesMode::Smart,
            Self::Fancy => LeavesMode::Fancy,
        }
    }
}

/// How rain and snow are drawn (OptiFine's "Rain & Snow").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RainChoice {
    /// Follow Graphics.
    #[default]
    Default,
    /// The fast precipitation.
    Fast,
    /// The fancy precipitation.
    Fancy,
    /// No falling rain or snow. Splashes and sounds stay.
    Off,
}

impl RainChoice {
    /// The choices in cycle order.
    pub const ALL: [Self; 4] = [Self::Default, Self::Fast, Self::Fancy, Self::Off];

    /// `None` when off, else whether the fancy version is drawn.
    #[must_use]
    pub fn fancy(self, fancy_graphics: bool) -> Option<bool> {
        match self {
            Self::Default => Some(fancy_graphics),
            Self::Fast => Some(false),
            Self::Fancy => Some(true),
            Self::Off => None,
        }
    }
}

/// How the distance fog is computed (OptiFine's "Fog").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FogChoice {
    /// Fog by depth along the view direction (vanilla).
    #[default]
    Fast,
    /// Fog by the true distance to the eye, so it does not move when the
    /// view turns.
    Fancy,
    /// No distance fog. The fog of water, lava and blindness stays.
    Off,
}

impl FogChoice {
    /// The choices in cycle order.
    pub const ALL: [Self; 3] = [Self::Fast, Self::Fancy, Self::Off];
}

/// An optional improvement with a cheap and a full version: Better Grass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OffFastFancy {
    /// Not applied (vanilla).
    #[default]
    Off,
    /// The cheap version.
    Fast,
    /// The full version.
    Fancy,
}

impl OffFastFancy {
    /// The choices in cycle order.
    pub const ALL: [Self; 3] = [Self::Off, Self::Fast, Self::Fancy];
}

/// The steps of the Fog Start option, as fractions of the fog distance.
pub const FOG_STARTS: [f32; 4] = [0.2, 0.4, 0.6, 0.8];

/// `[video.details]`: what is drawn of the scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DetailOptions {
    /// How far above the vanilla altitude the clouds float: `0.0` keeps
    /// vanilla's 128, `1.0` adds 128 blocks (OptiFine `ofCloudsHeight`).
    pub cloud_height: f32,
    /// How leaves are drawn.
    pub trees: TreesChoice,
    /// How rain and snow are drawn.
    pub rain: RainChoice,
    /// Whether the sky dome and its colour are drawn.
    pub sky: bool,
    /// Whether the stars are drawn.
    pub stars: bool,
    /// Whether the sun and the moon are drawn.
    pub sun_moon: bool,
    /// How the distance fog is computed.
    pub fog: FogChoice,
    /// Where the fog starts, as a fraction of its end distance; one of
    /// [`FOG_STARTS`].
    pub fog_start: f32,
    /// Whether dropped items are drawn the fancy way.
    pub dropped_items: GraphicsChoice,
    /// Whether the dark edges of the screen are drawn.
    pub vignette: GraphicsChoice,
    /// Whether the swamp tints grass, leaves and water its own way.
    pub swamp_colors: bool,
    /// Whether biome colours blend over the 3×3 columns around a block
    /// instead of changing at the border.
    pub smooth_biomes: bool,
}

impl Default for DetailOptions {
    fn default() -> Self {
        Self {
            cloud_height: 0.0,
            trees: TreesChoice::default(),
            rain: RainChoice::default(),
            sky: true,
            stars: true,
            sun_moon: true,
            fog: FogChoice::default(),
            fog_start: FOG_STARTS[3],
            dropped_items: GraphicsChoice::default(),
            vignette: GraphicsChoice::default(),
            swamp_colors: true,
            smooth_biomes: true,
        }
    }
}

impl DetailOptions {
    /// Puts the numbers back in range; a value that is not a number returns
    /// to its default. The fog start snaps to the nearest step.
    pub(super) fn clamp(&mut self) {
        let defaults = Self::default();
        self.cloud_height = if self.cloud_height.is_finite() {
            self.cloud_height.clamp(0.0, 1.0)
        } else {
            defaults.cloud_height
        };
        self.fog_start = if self.fog_start.is_finite() {
            FOG_STARTS[fog_start_step(self.fog_start)]
        } else {
            defaults.fog_start
        };
    }
}

/// The index in [`FOG_STARTS`] nearest to `start`.
#[must_use]
pub fn fog_start_step(start: f32) -> usize {
    let step = ((start - FOG_STARTS[0]) / 0.2).round();
    (step.max(0.0) as usize).min(FOG_STARTS.len() - 1)
}

/// `[video.quality]`: texture quality.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct QualityOptions {
    /// The sides of grass, mycelium and podzol take the top texture where it
    /// continues: Fast on every side, Fancy only towards a lower block of the
    /// same kind.
    pub better_grass: OffFastFancy,
    /// Blocks next to snow layers are drawn as covered by snow.
    pub better_snow: bool,
}

/// `[video.animations]`: animated textures and particle kinds. All on by
/// default, as in OptiFine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimationOptions {
    /// The water texture animates.
    pub water: bool,
    /// The lava texture animates.
    pub lava: bool,
    /// The fire texture animates.
    pub fire: bool,
    /// The nether portal texture animates.
    pub portal: bool,
    /// The other animated block textures animate.
    pub terrain: bool,
    /// Redstone dust particles.
    pub redstone_particles: bool,
    /// Explosion particles.
    pub explosion_particles: bool,
    /// Flame particles (torches, furnaces, fire).
    pub flame_particles: bool,
    /// Smoke particles.
    pub smoke_particles: bool,
    /// Void fog particles near the bottom of the world.
    pub void_particles: bool,
    /// Water particles: bubbles, splashes and suspended water.
    pub water_particles: bool,
    /// The splashes rain makes on the ground.
    pub rain_splash: bool,
    /// Portal particles.
    pub portal_particles: bool,
    /// Potion and spell particles.
    pub potion_particles: bool,
    /// Dripping water and lava particles.
    pub dripping_particles: bool,
    /// Firework sparks.
    pub firework_particles: bool,
}

impl Default for AnimationOptions {
    fn default() -> Self {
        Self {
            water: true,
            lava: true,
            fire: true,
            portal: true,
            terrain: true,
            redstone_particles: true,
            explosion_particles: true,
            flame_particles: true,
            smoke_particles: true,
            void_particles: true,
            water_particles: true,
            rain_splash: true,
            portal_particles: true,
            potion_particles: true,
            dripping_particles: true,
            firework_particles: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_follows_graphics() {
        assert!(GraphicsChoice::Default.is_fancy(true));
        assert!(!GraphicsChoice::Default.is_fancy(false));
        assert!(GraphicsChoice::Fancy.is_fancy(false));
        assert!(!GraphicsChoice::Fast.is_fancy(true));
    }

    #[test]
    fn trees_resolve_against_graphics() {
        assert_eq!(TreesChoice::Default.resolve(true), LeavesMode::Fancy);
        assert_eq!(TreesChoice::Default.resolve(false), LeavesMode::Fast);
        assert_eq!(TreesChoice::Smart.resolve(true), LeavesMode::Smart);
        assert_eq!(TreesChoice::default(), TreesChoice::Smart);
    }

    #[test]
    fn rain_off_draws_nothing() {
        assert_eq!(RainChoice::Off.fancy(true), None);
        assert_eq!(RainChoice::Default.fancy(false), Some(false));
    }

    #[test]
    fn the_fog_start_snaps_to_its_steps() {
        let mut details = DetailOptions {
            fog_start: 0.47,
            cloud_height: 7.0,
            ..DetailOptions::default()
        };
        details.clamp();
        assert_eq!(details.fog_start, 0.4);
        assert_eq!(details.cloud_height, 1.0);
        details.fog_start = f32::NAN;
        details.clamp();
        assert_eq!(details.fog_start, 0.8);
        assert_eq!(fog_start_step(-3.0), 0);
        assert_eq!(fog_start_step(9.0), 3);
    }

    #[test]
    fn a_bad_subsection_keeps_the_rest_of_video() {
        let video: super::super::VideoOptions =
            toml::from_str("render_distance = 12\n[details]\ntrees = 5\n").unwrap();
        assert_eq!(video.render_distance, 12);
        assert_eq!(video.details, DetailOptions::default());
    }

    #[test]
    fn the_sections_read_from_toml_with_snake_case_choices() {
        let details: DetailOptions =
            toml::from_str("trees = \"fancy\"\nrain = \"off\"\nfog = \"fancy\"").unwrap();
        assert_eq!(
            (details.trees, details.rain, details.fog),
            (TreesChoice::Fancy, RainChoice::Off, FogChoice::Fancy)
        );
        assert!(details.sky, "a missing key keeps its default");
    }
}

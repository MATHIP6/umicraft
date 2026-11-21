// SPDX-License-Identifier: MIT OR Apache-2.0

//! The video options as one declarative table (ADR-0089).
//!
//! Each [`OptionSpec`] describes one option once: the page it sits on, how
//! it is drawn, its label, its tooltip and what a change asks the client to
//! redo. The video pages, the tooltips and the apply step are all derived
//! from [`VIDEO_SPECS`], so adding an option takes one entry here, its field
//! in [`VideoOptions`], and its texts in the language files.
//!
//! Every value is seen as a *position* in `0..positions`:
//! - a toggle has two positions (off, on);
//! - a cycle has one position per value;
//! - a slider has one position per step.
//!
//! A button advances the position by one and wraps around. A slider maps its
//! fraction onto the positions. Comparing positions before and after tells
//! which options changed, and so which [`Effect`] to apply.
//!
//! The layout of the pages follows OptiFine's video settings (ADR-0088): a
//! root page, plus the Details, Quality, Performance, Animations and Other
//! pages. Only options that do something are listed (ADR-0089).

use umicraft_core::lang::Lang;

use super::video::{
    VideoOptions, MAX_CLOUD_SETTING, MAX_FPS, MAX_GUI_SCALE, MAX_MESH_THREADS,
    MAX_PARTICLE_SETTING, MAX_SMOOTH_LIGHTING, MIPMAP_LEVELS, RENDER_DISTANCE, UNLIMITED_FPS,
};
use super::video_sections::{
    fog_start_step, FogChoice, GraphicsChoice, OffFastFancy, RainChoice, TreesChoice, FOG_STARTS,
};

/// A page of the video settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoPage {
    /// The root page, with the buttons to the other pages.
    Root,
    /// Clouds, shadows and the other details of the scene.
    Details,
    /// Texture quality: mipmaps and, later, the pack features.
    Quality,
    /// The knobs that trade looks for speed.
    Performance,
    /// The animated textures and the particles.
    Animations,
    /// Window and miscellaneous options, and the reset button.
    Other,
}

impl VideoPage {
    /// The sub-pages, in the order of their buttons on the root page.
    pub const SUB_PAGES: [Self; 5] = [
        Self::Quality,
        Self::Details,
        Self::Performance,
        Self::Animations,
        Self::Other,
    ];

    /// The stable name of the page, used in widget ids and language keys.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Details => "details",
            Self::Quality => "quality",
            Self::Performance => "performance",
            Self::Animations => "animations",
            Self::Other => "other",
        }
    }
}

/// How an option is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// A button: a click advances to the next position.
    Button,
    /// A slider over the positions.
    Slider,
}

/// What a changed option asks the client to redo. The variants are ordered:
/// when several options change, the strongest effect wins, and it includes
/// the weaker ones (a resource reload also re-meshes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Effect {
    /// Nothing more: the value is read where it is used, or applied every
    /// frame.
    #[default]
    Immediate,
    /// Re-mesh every loaded column: the mesh workers read the option.
    ReMesh,
    /// Reload the resources (atlases, models, mesh workers), then re-mesh.
    ReloadResources,
}

/// One video option.
#[derive(Clone, Copy)]
pub struct OptionSpec {
    /// The stable id: widget id `video:<id>` and tooltip keys
    /// `umicraft.options.tooltip.<id>.N`.
    pub id: &'static str,
    /// The language key of the option's name.
    pub label: &'static str,
    /// The page the option is on.
    pub page: VideoPage,
    /// How the option is drawn.
    pub control: Control,
    /// The number of positions, at least two.
    pub positions: u32,
    /// The position of the current value.
    pub get: fn(&VideoOptions) -> u32,
    /// Sets the value of a position (`< positions`).
    pub set: fn(&mut VideoOptions, u32),
    /// The text of the current value, after "Name: ".
    pub value: fn(&Lang, &VideoOptions) -> String,
    /// What a change of this option asks the client to redo.
    pub effect: Effect,
    /// The positions the Animations page's "All OFF" and "All ON" buttons set,
    /// in that order. `None` for an option the buttons leave alone.
    pub all: Option<[u32; 2]>,
}

impl OptionSpec {
    /// The position after a click: the next one, wrapping around.
    #[must_use]
    pub fn next(&self, video: &VideoOptions) -> u32 {
        ((self.get)(video) + 1) % self.positions
    }

    /// The slider fraction of the current value, in `0.0..=1.0`.
    #[must_use]
    pub fn fraction(&self, video: &VideoOptions) -> f32 {
        (self.get)(video) as f32 / (self.positions - 1) as f32
    }

    /// The position nearest to a slider `fraction`.
    #[must_use]
    pub fn position_at(&self, fraction: f32) -> u32 {
        let last = self.positions - 1;
        ((fraction.clamp(0.0, 1.0) * last as f32).round() as u32).min(last)
    }

    /// Whether this option has another value in `after` than in `before`.
    #[must_use]
    pub fn changed(&self, before: &VideoOptions, after: &VideoOptions) -> bool {
        (self.get)(before) != (self.get)(after)
    }
}

impl std::fmt::Debug for OptionSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OptionSpec")
            .field("id", &self.id)
            .field("page", &self.page)
            .finish_non_exhaustive()
    }
}

/// The options of `page`, in display order.
pub fn specs_on(page: VideoPage) -> impl Iterator<Item = &'static OptionSpec> {
    VIDEO_SPECS.iter().filter(move |spec| spec.page == page)
}

/// The strongest effect among the options that differ between `before` and
/// `after`.
#[must_use]
pub fn effect_of_change(before: &VideoOptions, after: &VideoOptions) -> Effect {
    VIDEO_SPECS
        .iter()
        .filter(|spec| spec.changed(before, after))
        .map(|spec| spec.effect)
        .max()
        .unwrap_or_default()
}

/// What the Animations page's "All ON" (`on`) or "All OFF" buttons do: every
/// option with an `all` pair takes its on or off position.
pub fn set_all(video: &mut VideoOptions, on: bool) {
    for spec in VIDEO_SPECS {
        if let Some(positions) = spec.all {
            (spec.set)(video, positions[usize::from(on)]);
        }
    }
}

// ---------------------------------------------------------------------------
// Value texts
// ---------------------------------------------------------------------------

/// The unit vanilla appends to the render distance (`GameSettings`
/// hard-codes `" chunks"`; there is no language key for it).
const CHUNKS_UNIT: &str = " chunks";
/// The unit vanilla appends to the frame rate limit (hard-coded `" fps"`).
const FPS_UNIT: &str = " fps";

/// `GameSettings.AMBIENT_OCCLUSIONS`.
const SMOOTH_LIGHTING_KEYS: [&str; 3] = ["options.ao.off", "options.ao.min", "options.ao.max"];
/// `GameSettings.field_181149_aW`.
const CLOUD_KEYS: [&str; 3] = [
    "options.off",
    "options.graphics.fast",
    "options.graphics.fancy",
];
/// `GameSettings.GUISCALES`.
const GUI_SCALE_KEYS: [&str; 4] = [
    "options.guiScale.auto",
    "options.guiScale.small",
    "options.guiScale.normal",
    "options.guiScale.large",
];

fn on_off(lang: &Lang, on: bool) -> String {
    lang.tr(if on { "options.on" } else { "options.off" })
        .to_string()
}

/// The text of `keys[value]`; an index out of range reads as 0, as
/// `GameSettings.getTranslation` does.
fn keyed(lang: &Lang, keys: &[&str], value: u8) -> String {
    let key = keys.get(usize::from(value)).unwrap_or(&keys[0]);
    lang.tr(key).to_string()
}

/// The value of the brightness slider (`GameSettings.getKeyBinding(GAMMA)`):
/// "Moody" at 0, "Bright" at 1, else "+N%".
#[must_use]
pub fn gamma_value(lang: &Lang, gamma: f32) -> String {
    if gamma == 0.0 {
        lang.tr("options.gamma.min").to_string()
    } else if gamma == 1.0 {
        lang.tr("options.gamma.max").to_string()
    } else {
        format!("+{}%", (gamma * 100.0) as i32)
    }
}

// ---------------------------------------------------------------------------
// Positions of the sliders that are not a plain integer range
// ---------------------------------------------------------------------------

/// The frame-rate limit steps, as OptiFine lays the slider out: VSync first,
/// then 10 to 250 fps by tens, then "Unlimited" (`UNLIMITED_FPS`).
const FPS_STEP: u32 = 10;
const FPS_POSITIONS: u32 = (UNLIMITED_FPS - *MAX_FPS.start()) / FPS_STEP + 2;

fn fps_position(video: &VideoOptions) -> u32 {
    if video.vsync {
        0
    } else {
        (video.max_fps.clamp(*MAX_FPS.start(), UNLIMITED_FPS) - MAX_FPS.start()) / FPS_STEP + 1
    }
}

fn set_fps_position(video: &mut VideoOptions, position: u32) {
    video.vsync = position == 0;
    if position > 0 {
        video.max_fps = MAX_FPS.start() + (position - 1) * FPS_STEP;
    }
}

fn fps_value(lang: &Lang, video: &VideoOptions) -> String {
    if video.vsync {
        lang.tr("umicraft.options.framerate.vsync").to_string()
    } else if video.max_fps >= UNLIMITED_FPS {
        lang.tr("options.framerateLimit.max").to_string()
    } else {
        format!("{}{FPS_UNIT}", video.max_fps)
    }
}

/// Brightness is stored as a fraction; the slider has one position per
/// percent.
const GAMMA_POSITIONS: u32 = 101;

fn mesh_threads_value(lang: &Lang, video: &VideoOptions) -> String {
    match video.mesh_threads {
        0 => lang.tr("umicraft.options.meshThreads.auto").to_string(),
        threads => threads.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Enum choices
// ---------------------------------------------------------------------------

/// An option whose values are the variants of an enum, cycled in `ALL`
/// order.
trait Choice: Copy + PartialEq + 'static {
    /// Every value, in cycle order.
    const ALL: &'static [Self];
    /// The language key of the value's text.
    fn lang_key(self) -> &'static str;
}

/// The position of `value` in its `ALL` (0 if missing, which cannot happen
/// for a well-formed `ALL`).
fn position_of<T: Choice>(value: T) -> u32 {
    T::ALL.iter().position(|&v| v == value).unwrap_or(0) as u32
}

/// The value at `position`, clamped to the last one.
fn choice_at<T: Choice>(position: u32) -> T {
    T::ALL[(position as usize).min(T::ALL.len() - 1)]
}

const DEFAULT_KEY: &str = "umicraft.options.choice.default";
const FAST_KEY: &str = "options.graphics.fast";
const FANCY_KEY: &str = "options.graphics.fancy";
const OFF_KEY: &str = "options.off";

impl Choice for GraphicsChoice {
    const ALL: &'static [Self] = &GraphicsChoice::ALL;
    fn lang_key(self) -> &'static str {
        match self {
            Self::Default => DEFAULT_KEY,
            Self::Fast => FAST_KEY,
            Self::Fancy => FANCY_KEY,
        }
    }
}

impl Choice for TreesChoice {
    const ALL: &'static [Self] = &TreesChoice::ALL;
    fn lang_key(self) -> &'static str {
        match self {
            Self::Default => DEFAULT_KEY,
            Self::Fast => FAST_KEY,
            Self::Smart => "umicraft.options.choice.smart",
            Self::Fancy => FANCY_KEY,
        }
    }
}

impl Choice for RainChoice {
    const ALL: &'static [Self] = &RainChoice::ALL;
    fn lang_key(self) -> &'static str {
        match self {
            Self::Default => DEFAULT_KEY,
            Self::Fast => FAST_KEY,
            Self::Fancy => FANCY_KEY,
            Self::Off => OFF_KEY,
        }
    }
}

impl Choice for FogChoice {
    const ALL: &'static [Self] = &FogChoice::ALL;
    fn lang_key(self) -> &'static str {
        match self {
            Self::Fast => FAST_KEY,
            Self::Fancy => FANCY_KEY,
            Self::Off => OFF_KEY,
        }
    }
}

impl Choice for OffFastFancy {
    const ALL: &'static [Self] = &OffFastFancy::ALL;
    fn lang_key(self) -> &'static str {
        match self {
            Self::Off => OFF_KEY,
            Self::Fast => FAST_KEY,
            Self::Fancy => FANCY_KEY,
        }
    }
}

/// The cloud height slider: one position per percent; 0 reads "Off" (the
/// vanilla altitude).
const CLOUD_HEIGHT_POSITIONS: u32 = 101;

fn percent_or_off(lang: &Lang, fraction: f32) -> String {
    match (fraction * 100.0).round() as u32 {
        0 => lang.tr(OFF_KEY).to_string(),
        percent => format!("{percent}%"),
    }
}

// ---------------------------------------------------------------------------
// The table
// ---------------------------------------------------------------------------

/// A two-position option over a boolean field (`toggle!(id, label, page,
/// field.path, effect)`). A trailing `all` puts it under the Animations
/// page's "All ON" / "All OFF" buttons.
macro_rules! toggle {
    ($id:literal, $label:literal, $page:ident, $($field:ident).+, $effect:ident) => {
        toggle!(@spec $id, $label, $page, $($field).+, $effect, None)
    };
    ($id:literal, $label:literal, $page:ident, $($field:ident).+, $effect:ident, all) => {
        toggle!(@spec $id, $label, $page, $($field).+, $effect, Some([0, 1]))
    };
    (@spec $id:literal, $label:literal, $page:ident, $($field:ident).+, $effect:ident, $all:expr) => {
        OptionSpec {
            id: $id,
            label: $label,
            page: VideoPage::$page,
            control: Control::Button,
            positions: 2,
            get: |video| u32::from(video.$($field).+),
            set: |video, position| video.$($field).+ = position == 1,
            value: |lang, video| on_off(lang, video.$($field).+),
            effect: Effect::$effect,
            all: $all,
        }
    };
}

/// A button that cycles through the values of a [`Choice`] enum
/// (`choice!(id, label, page, Type, field.path, effect)`).
macro_rules! choice {
    ($id:literal, $label:literal, $page:ident, $ty:ty, $($field:ident).+, $effect:ident) => {
        OptionSpec {
            id: $id,
            label: $label,
            page: VideoPage::$page,
            control: Control::Button,
            positions: <$ty as Choice>::ALL.len() as u32,
            get: |video| position_of(video.$($field).+),
            set: |video, position| video.$($field).+ = choice_at(position),
            value: |lang, video| lang.tr(video.$($field).+.lang_key()).to_string(),
            effect: Effect::$effect,
            all: None,
        }
    };
}

/// Every video option, in display order within each page.
pub static VIDEO_SPECS: &[OptionSpec] = &[
    // Root page, in OptiFine's order.
    OptionSpec {
        id: "graphics",
        label: "options.graphics",
        page: VideoPage::Root,
        control: Control::Button,
        positions: 2,
        get: |video| u32::from(video.fancy_graphics),
        set: |video, position| video.fancy_graphics = position == 1,
        value: |lang, video| {
            lang.tr(if video.fancy_graphics {
                "options.graphics.fancy"
            } else {
                "options.graphics.fast"
            })
            .to_string()
        },
        effect: Effect::Immediate,
        all: None,
    },
    OptionSpec {
        id: "renderDistance",
        label: "options.renderDistance",
        page: VideoPage::Root,
        control: Control::Slider,
        positions: (*RENDER_DISTANCE.end() - *RENDER_DISTANCE.start()) as u32 + 1,
        get: |video| u32::from(video.render_distance - RENDER_DISTANCE.start()),
        set: |video, position| {
            video.render_distance = RENDER_DISTANCE.start() + position as u8;
        },
        value: |_, video| format!("{}{CHUNKS_UNIT}", video.render_distance),
        effect: Effect::Immediate,
        all: None,
    },
    OptionSpec {
        id: "smoothLighting",
        label: "options.ao",
        page: VideoPage::Root,
        control: Control::Button,
        positions: MAX_SMOOTH_LIGHTING as u32 + 1,
        get: |video| u32::from(video.smooth_lighting),
        set: |video, position| video.smooth_lighting = position as u8,
        value: |lang, video| keyed(lang, &SMOOTH_LIGHTING_KEYS, video.smooth_lighting),
        effect: Effect::ReMesh,
        all: None,
    },
    OptionSpec {
        id: "maxFramerate",
        label: "options.framerateLimit",
        page: VideoPage::Root,
        control: Control::Slider,
        positions: FPS_POSITIONS,
        get: fps_position,
        set: set_fps_position,
        value: fps_value,
        effect: Effect::Immediate,
        all: None,
    },
    toggle!(
        "viewBobbing",
        "options.viewBobbing",
        Root,
        view_bobbing,
        Immediate
    ),
    OptionSpec {
        id: "guiScale",
        label: "options.guiScale",
        page: VideoPage::Root,
        control: Control::Button,
        positions: MAX_GUI_SCALE as u32 + 1,
        get: |video| u32::from(video.gui_scale),
        set: |video, position| video.gui_scale = position as u8,
        value: |lang, video| keyed(lang, &GUI_SCALE_KEYS, video.gui_scale),
        effect: Effect::Immediate,
        all: None,
    },
    OptionSpec {
        id: "brightness",
        label: "options.gamma",
        page: VideoPage::Root,
        control: Control::Slider,
        positions: GAMMA_POSITIONS,
        get: |video| (video.gamma.clamp(0.0, 1.0) * 100.0).round() as u32,
        set: |video, position| video.gamma = position as f32 / 100.0,
        value: |lang, video| gamma_value(lang, video.gamma),
        effect: Effect::Immediate,
        all: None,
    },
    toggle!(
        "blockAlternatives",
        "options.blockAlternatives",
        Root,
        block_alternatives,
        ReMesh
    ),
    toggle!(
        "dynamicFov",
        "umicraft.options.dynamicFov",
        Root,
        dynamic_fov,
        Immediate
    ),
    // Details, in OptiFine's order.
    OptionSpec {
        id: "clouds",
        label: "options.renderClouds",
        page: VideoPage::Details,
        control: Control::Button,
        positions: MAX_CLOUD_SETTING as u32 + 1,
        get: |video| u32::from(video.clouds),
        set: |video, position| video.clouds = position as u8,
        value: |lang, video| keyed(lang, &CLOUD_KEYS, video.clouds),
        effect: Effect::Immediate,
        all: None,
    },
    OptionSpec {
        id: "cloudHeight",
        label: "umicraft.options.cloudHeight",
        page: VideoPage::Details,
        control: Control::Slider,
        positions: CLOUD_HEIGHT_POSITIONS,
        get: |video| (video.details.cloud_height * 100.0).round() as u32,
        set: |video, position| video.details.cloud_height = position as f32 / 100.0,
        value: |lang, video| percent_or_off(lang, video.details.cloud_height),
        effect: Effect::Immediate,
        all: None,
    },
    choice!(
        "trees",
        "umicraft.options.trees",
        Details,
        TreesChoice,
        details.trees,
        ReMesh
    ),
    choice!(
        "rain",
        "umicraft.options.rain",
        Details,
        RainChoice,
        details.rain,
        Immediate
    ),
    toggle!(
        "sky",
        "umicraft.options.sky",
        Details,
        details.sky,
        Immediate
    ),
    toggle!(
        "stars",
        "umicraft.options.stars",
        Details,
        details.stars,
        Immediate
    ),
    toggle!(
        "sunMoon",
        "umicraft.options.sunMoon",
        Details,
        details.sun_moon,
        Immediate
    ),
    choice!(
        "fog",
        "umicraft.options.fog",
        Details,
        FogChoice,
        details.fog,
        Immediate
    ),
    OptionSpec {
        id: "fogStart",
        label: "umicraft.options.fogStart",
        page: VideoPage::Details,
        control: Control::Button,
        positions: FOG_STARTS.len() as u32,
        get: |video| fog_start_step(video.details.fog_start) as u32,
        set: |video, position| {
            video.details.fog_start = FOG_STARTS[(position as usize).min(FOG_STARTS.len() - 1)];
        },
        value: |_, video| format!("{:.1}", video.details.fog_start),
        effect: Effect::Immediate,
        all: None,
    },
    choice!(
        "droppedItems",
        "umicraft.options.droppedItems",
        Details,
        GraphicsChoice,
        details.dropped_items,
        Immediate
    ),
    toggle!(
        "entityShadows",
        "options.entityShadows",
        Details,
        entity_shadows,
        Immediate
    ),
    toggle!(
        "swampColors",
        "umicraft.options.swampColors",
        Details,
        details.swamp_colors,
        ReMesh
    ),
    toggle!(
        "smoothBiomes",
        "umicraft.options.smoothBiomes",
        Details,
        details.smooth_biomes,
        ReMesh
    ),
    choice!(
        "vignette",
        "umicraft.options.vignette",
        Details,
        GraphicsChoice,
        details.vignette,
        Immediate
    ),
    // Quality.
    OptionSpec {
        id: "mipmapLevels",
        label: "options.mipmapLevels",
        page: VideoPage::Quality,
        control: Control::Slider,
        positions: (*MIPMAP_LEVELS.end() - *MIPMAP_LEVELS.start()) as u32 + 1,
        get: |video| u32::from(video.mipmap_levels - MIPMAP_LEVELS.start()),
        set: |video, position| video.mipmap_levels = MIPMAP_LEVELS.start() + position as u8,
        value: |lang, video| match video.mipmap_levels {
            0 => lang.tr("options.off").to_string(),
            levels => levels.to_string(),
        },
        effect: Effect::ReloadResources,
        all: None,
    },
    choice!(
        "betterGrass",
        "umicraft.options.betterGrass",
        Quality,
        OffFastFancy,
        quality.better_grass,
        ReMesh
    ),
    toggle!(
        "betterSnow",
        "umicraft.options.betterSnow",
        Quality,
        quality.better_snow,
        ReMesh
    ),
    // Performance.
    toggle!(
        "caveCulling",
        "umicraft.options.caveCulling",
        Performance,
        cave_culling,
        Immediate
    ),
    OptionSpec {
        id: "meshThreads",
        label: "umicraft.options.meshThreads",
        page: VideoPage::Performance,
        control: Control::Slider,
        positions: MAX_MESH_THREADS as u32 + 1,
        get: |video| u32::from(video.mesh_threads),
        set: |video, position| video.mesh_threads = position as u8,
        value: mesh_threads_value,
        // The worker pool is rebuilt with the terrain.
        effect: Effect::ReloadResources,
        all: None,
    },
    // Animations, in OptiFine's order.
    toggle!(
        "animatedWater",
        "umicraft.options.animatedWater",
        Animations,
        animations.water,
        Immediate,
        all
    ),
    toggle!(
        "animatedLava",
        "umicraft.options.animatedLava",
        Animations,
        animations.lava,
        Immediate,
        all
    ),
    toggle!(
        "animatedFire",
        "umicraft.options.animatedFire",
        Animations,
        animations.fire,
        Immediate,
        all
    ),
    toggle!(
        "animatedPortal",
        "umicraft.options.animatedPortal",
        Animations,
        animations.portal,
        Immediate,
        all
    ),
    toggle!(
        "redstoneParticles",
        "umicraft.options.redstoneParticles",
        Animations,
        animations.redstone_particles,
        Immediate,
        all
    ),
    toggle!(
        "explosionParticles",
        "umicraft.options.explosionParticles",
        Animations,
        animations.explosion_particles,
        Immediate,
        all
    ),
    toggle!(
        "flameParticles",
        "umicraft.options.flameParticles",
        Animations,
        animations.flame_particles,
        Immediate,
        all
    ),
    toggle!(
        "smokeParticles",
        "umicraft.options.smokeParticles",
        Animations,
        animations.smoke_particles,
        Immediate,
        all
    ),
    toggle!(
        "voidParticles",
        "umicraft.options.voidParticles",
        Animations,
        animations.void_particles,
        Immediate,
        all
    ),
    toggle!(
        "waterParticles",
        "umicraft.options.waterParticles",
        Animations,
        animations.water_particles,
        Immediate,
        all
    ),
    toggle!(
        "rainSplash",
        "umicraft.options.rainSplash",
        Animations,
        animations.rain_splash,
        Immediate,
        all
    ),
    toggle!(
        "portalParticles",
        "umicraft.options.portalParticles",
        Animations,
        animations.portal_particles,
        Immediate,
        all
    ),
    toggle!(
        "potionParticles",
        "umicraft.options.potionParticles",
        Animations,
        animations.potion_particles,
        Immediate,
        all
    ),
    toggle!(
        "drippingParticles",
        "umicraft.options.drippingParticles",
        Animations,
        animations.dripping_particles,
        Immediate,
        all
    ),
    toggle!(
        "animatedTerrain",
        "umicraft.options.animatedTerrain",
        Animations,
        animations.terrain,
        Immediate,
        all
    ),
    toggle!(
        "fireworkParticles",
        "umicraft.options.fireworkParticles",
        Animations,
        animations.firework_particles,
        Immediate,
        all
    ),
    OptionSpec {
        id: "particles",
        label: "options.particles",
        page: VideoPage::Animations,
        control: Control::Button,
        positions: MAX_PARTICLE_SETTING as u32 + 1,
        get: |video| u32::from(video.particles),
        set: |video, position| video.particles = position as u8,
        value: |lang, video| lang.tr(video.particle_setting().lang_key()).to_string(),
        effect: Effect::Immediate,
        // OptiFine's "All OFF" picks Minimal, "All ON" picks All.
        all: Some([MAX_PARTICLE_SETTING as u32, 0]),
    },
    // Other.
    toggle!(
        "fullscreen",
        "options.fullscreen",
        Other,
        fullscreen,
        Immediate
    ),
];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn spec(id: &str) -> Option<&'static OptionSpec> {
        VIDEO_SPECS.iter().find(|spec| spec.id == id)
    }

    #[test]
    fn ids_are_unique_and_every_spec_has_two_positions_or_more() {
        let mut ids = HashSet::new();
        for spec in VIDEO_SPECS {
            assert!(ids.insert(spec.id), "duplicate id {}", spec.id);
            assert!(spec.positions >= 2, "{}", spec.id);
        }
    }

    #[test]
    fn every_position_round_trips() {
        for spec in VIDEO_SPECS {
            for position in 0..spec.positions {
                let mut video = VideoOptions::default();
                (spec.set)(&mut video, position);
                assert_eq!((spec.get)(&video), position, "{} at {position}", spec.id);
                let mut clamped = video.clone();
                clamped.clamp();
                assert_eq!(clamped, video, "{} at {position} is in range", spec.id);
            }
        }
    }

    #[test]
    fn the_defaults_sit_on_a_position() {
        let video = VideoOptions::default();
        for spec in VIDEO_SPECS {
            assert!((spec.get)(&video) < spec.positions, "{}", spec.id);
        }
    }

    #[test]
    fn a_button_wraps_around() {
        let spec = spec("guiScale").expect("listed");
        let mut video = VideoOptions {
            gui_scale: MAX_GUI_SCALE,
            ..VideoOptions::default()
        };
        let next = spec.next(&video);
        (spec.set)(&mut video, next);
        assert_eq!(video.gui_scale, 0);
    }

    #[test]
    fn the_framerate_slider_runs_from_vsync_to_unlimited() {
        let spec = spec("maxFramerate").expect("listed");
        let mut video = VideoOptions::default();
        (spec.set)(&mut video, 0);
        assert!(video.vsync);
        (spec.set)(&mut video, 1);
        assert_eq!((video.vsync, video.max_fps), (false, 10));
        (spec.set)(&mut video, spec.positions - 1);
        assert_eq!(video.max_fps, UNLIMITED_FPS);
        assert_eq!(spec.position_at(1.0), spec.positions - 1);
        assert_eq!(spec.position_at(0.0), 0);
    }

    #[test]
    fn the_render_distance_goes_to_32() {
        let spec = spec("renderDistance").expect("listed");
        let mut video = VideoOptions::default();
        (spec.set)(&mut video, spec.positions - 1);
        assert_eq!(video.render_distance, 32);
    }

    #[test]
    fn the_strongest_effect_wins() {
        let before = VideoOptions::default();
        let mut after = before.clone();
        assert_eq!(effect_of_change(&before, &after), Effect::Immediate);
        after.view_bobbing = !after.view_bobbing;
        assert_eq!(effect_of_change(&before, &after), Effect::Immediate);
        after.smooth_lighting = 0;
        assert_eq!(effect_of_change(&before, &after), Effect::ReMesh);
        after.mipmap_levels = 0;
        assert_eq!(effect_of_change(&before, &after), Effect::ReloadResources);
    }

    #[test]
    fn all_on_and_all_off_set_the_animation_options() {
        let mut video = VideoOptions::default();
        set_all(&mut video, false);
        assert_eq!(video.particles, MAX_PARTICLE_SETTING);
        assert!(!video.animations.water && !video.animations.firework_particles);
        set_all(&mut video, true);
        assert_eq!(video.particles, 0);
        assert_eq!(
            video.animations,
            super::super::video_sections::AnimationOptions::default()
        );
    }

    #[test]
    fn every_page_but_the_root_is_a_sub_page() {
        assert!(!VideoPage::SUB_PAGES.contains(&VideoPage::Root));
        for spec in VIDEO_SPECS {
            assert!(spec.page == VideoPage::Root || VideoPage::SUB_PAGES.contains(&spec.page));
        }
    }
}

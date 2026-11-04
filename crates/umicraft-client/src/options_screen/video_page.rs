// SPDX-License-Identifier: MIT OR Apache-2.0

//! The video settings page (`GuiVideoSettings`).

use umicraft_core::lang::Lang;
use umicraft_ui::{FontAtlas, Rect, Ui};

use super::layout::{Gui, BUTTON_H, HALF_W, VIDEO_ROW};
use super::{on_off, option_label, OptionsScreen, Outcome};
use crate::options::{
    Options, VideoOptions, MAX_CLOUD_SETTING, MAX_FPS, MAX_GUI_SCALE, MAX_PARTICLE_SETTING,
    MAX_SMOOTH_LIGHTING, MIPMAP_LEVELS, RENDER_DISTANCE, UNLIMITED_FPS,
};

/// The unit vanilla appends to the render distance (`GameSettings`
/// hard-codes `" chunks"`; there is no language key for it).
const CHUNKS_UNIT: &str = " chunks";
/// The unit vanilla appends to the frame rate limit (hard-coded `" fps"`).
const FPS_UNIT: &str = " fps";

/// One entry of `GuiVideoSettings.videoOptions`.
///
/// The list is the vanilla one without the two entries that have no meaning
/// here: "3D Anaglyph" (not offered, ADR-0043) and "Use VBOs", which vanilla
/// itself hides when the driver has no vertex buffer objects (wgpu has no
/// such switch).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VideoEntry {
    Graphics,
    RenderDistance,
    SmoothLighting,
    MaxFps,
    ViewBobbing,
    GuiScale,
    Brightness,
    Clouds,
    Particles,
    Fullscreen,
    Vsync,
    Mipmaps,
    BlockAlternatives,
    EntityShadows,
}

/// The entries in the order of `GuiVideoSettings.videoOptions`, two per row.
const VIDEO_ENTRIES: [VideoEntry; 14] = [
    VideoEntry::Graphics,
    VideoEntry::RenderDistance,
    VideoEntry::SmoothLighting,
    VideoEntry::MaxFps,
    VideoEntry::ViewBobbing,
    VideoEntry::GuiScale,
    VideoEntry::Brightness,
    VideoEntry::Clouds,
    VideoEntry::Particles,
    VideoEntry::Fullscreen,
    VideoEntry::Vsync,
    VideoEntry::Mipmaps,
    VideoEntry::BlockAlternatives,
    VideoEntry::EntityShadows,
];

/// The language keys of the three values of an option that cycles
/// (`GameSettings.AMBIENT_OCCLUSIONS` and `field_181149_aW`).
const SMOOTH_LIGHTING_KEYS: [&str; 3] = ["options.ao.off", "options.ao.min", "options.ao.max"];
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

/// Where one entry is drawn and whether a click on it counts.
#[derive(Clone, Copy)]
struct Slot {
    index: usize,
    rect: Rect,
    /// The row lies fully inside the list area, so it can be clicked.
    inside: bool,
}

impl OptionsScreen {
    /// `GuiVideoSettings`.
    pub(super) fn show_video(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        gui: Gui,
        lang: &Lang,
        outcome: &mut Outcome,
    ) {
        let (top, bottom) = (32.0, gui.height - 32.0);
        let cx = gui.cx();
        let rows = VIDEO_ENTRIES.len().div_ceil(2);
        let content = rows as f32 * VIDEO_ROW;
        self.scroll_list(ui, content, bottom - top, VIDEO_ROW);
        Self::list_background(ui, gui, top, bottom);

        // The list is clipped to its area; a row is only clickable while it
        // lies inside.
        ui.push_clip(gui.rect(0.0, top, gui.width, bottom - top));
        for (index, entry) in VIDEO_ENTRIES.into_iter().enumerate() {
            let x = cx - 155.0 + (index % 2) as f32 * 160.0;
            let y = top + 4.0 - self.scroll + (index / 2) as f32 * VIDEO_ROW;
            let slot = Slot {
                index,
                rect: gui.rect(x, y, HALF_W, BUTTON_H),
                inside: y >= top && y + BUTTON_H <= bottom,
            };
            self.video_entry(ui, atlas, options, lang, entry, slot, outcome);
        }
        ui.pop_clip();
        Self::scrollbar(ui, gui, cx + 156.0, (top, bottom), content, self.scroll);

        Self::title(ui, atlas, gui, lang.tr("options.videoTitle"), 5.0);
        let done = gui.rect(cx - 100.0, gui.height - 27.0, 200.0, BUTTON_H);
        self.back(ui, atlas, done, lang);
    }

    /// Draws one entry and applies a click on it.
    #[allow(clippy::too_many_arguments)]
    fn video_entry(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        lang: &Lang,
        entry: VideoEntry,
        slot: Slot,
        outcome: &mut Outcome,
    ) {
        let video = &mut options.video;
        let id = format!("video:{}", slot.index);
        if let Some((name_key, flag)) = on_off_option(video, entry) {
            if self.live_toggle(ui, atlas, lang, name_key, &id, slot, flag) {
                outcome.changed = true;
            }
            return;
        }
        match entry {
            VideoEntry::RenderDistance => {
                let span = f32::from(RENDER_DISTANCE.end() - RENDER_DISTANCE.start());
                let distance = video.render_distance;
                let label = option_label(
                    lang,
                    "options.renderDistance",
                    &format!("{distance}{CHUNKS_UNIT}"),
                );
                let fraction = f32::from(distance - RENDER_DISTANCE.start()) / span;
                if let Some(value) = self.live_slider(ui, atlas, "distance", slot, &label, fraction)
                {
                    video.render_distance = RENDER_DISTANCE.start() + (value * span).round() as u8;
                    outcome.changed = true;
                }
            }
            VideoEntry::MaxFps => {
                let span = (MAX_FPS.end() - MAX_FPS.start()) as f32;
                let fps = video.max_fps;
                let value = if fps >= UNLIMITED_FPS {
                    lang.tr("options.framerateLimit.max").to_string()
                } else {
                    format!("{fps}{FPS_UNIT}")
                };
                let label = option_label(lang, "options.framerateLimit", &value);
                let fraction = (fps - MAX_FPS.start()) as f32 / span;
                if let Some(value) = self.live_slider(ui, atlas, "fps", slot, &label, fraction) {
                    // `GameSettings.snapToStep`: steps of 10.
                    let steps = (value * span / 10.0).round() as u32;
                    video.max_fps = MAX_FPS.start() + steps * 10;
                    outcome.changed = true;
                }
            }
            VideoEntry::Brightness => {
                let label = option_label(lang, "options.gamma", &gamma_value(lang, video.gamma));
                if let Some(value) = self.live_slider(ui, atlas, "gamma", slot, &label, video.gamma)
                {
                    video.gamma = value;
                    outcome.changed = true;
                }
            }
            VideoEntry::Mipmaps => {
                let span = f32::from(MIPMAP_LEVELS.end() - MIPMAP_LEVELS.start());
                let levels = video.mipmap_levels;
                let value = if levels == 0 {
                    lang.tr("options.off").to_string()
                } else {
                    levels.to_string()
                };
                let label = option_label(lang, "options.mipmapLevels", &value);
                let fraction = f32::from(levels - MIPMAP_LEVELS.start()) / span;
                if let Some(value) = self.live_slider(ui, atlas, "mipmap", slot, &label, fraction) {
                    video.mipmap_levels = MIPMAP_LEVELS.start() + (value * span).round() as u8;
                    outcome.changed = true;
                }
            }
            VideoEntry::Graphics => {
                let value = lang.tr(if video.fancy_graphics {
                    "options.graphics.fancy"
                } else {
                    "options.graphics.fast"
                });
                let label = option_label(lang, "options.graphics", value);
                if self.live_button(ui, atlas, &id, slot, &label) {
                    video.fancy_graphics = !video.fancy_graphics;
                    outcome.changed = true;
                }
            }
            VideoEntry::SmoothLighting => {
                let label = cycle_label(
                    lang,
                    "options.ao",
                    &SMOOTH_LIGHTING_KEYS,
                    video.smooth_lighting,
                );
                if self.live_button(ui, atlas, &id, slot, &label) {
                    video.smooth_lighting = (video.smooth_lighting + 1) % (MAX_SMOOTH_LIGHTING + 1);
                    outcome.changed = true;
                }
            }
            VideoEntry::GuiScale => {
                let label = cycle_label(lang, "options.guiScale", &GUI_SCALE_KEYS, video.gui_scale);
                if self.live_button(ui, atlas, &id, slot, &label) {
                    // `GameSettings.setOptionValue(GUI_SCALE)`: `+ 1 & 3`.
                    video.gui_scale = (video.gui_scale + 1) % (MAX_GUI_SCALE + 1);
                    outcome.changed = true;
                }
            }
            VideoEntry::Clouds => {
                let label = cycle_label(lang, "options.renderClouds", &CLOUD_KEYS, video.clouds);
                if self.live_button(ui, atlas, &id, slot, &label) {
                    video.clouds = (video.clouds + 1) % (MAX_CLOUD_SETTING + 1);
                    outcome.changed = true;
                }
            }
            VideoEntry::Particles => {
                // `GameSettings.setOptionValue(PARTICLES)`: cycles 0, 1, 2.
                let setting = lang.tr(video.particle_setting().lang_key());
                let label = option_label(lang, "options.particles", setting);
                if self.live_button(ui, atlas, &id, slot, &label) {
                    video.particles = (video.particles + 1) % (MAX_PARTICLE_SETTING + 1);
                    outcome.changed = true;
                }
            }
            // The on/off entries were handled above.
            VideoEntry::ViewBobbing
            | VideoEntry::Fullscreen
            | VideoEntry::Vsync
            | VideoEntry::BlockAlternatives
            | VideoEntry::EntityShadows => {}
        }
    }

    /// A button of the list; true when it was clicked while fully visible.
    fn live_button(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        id: &str,
        slot: Slot,
        label: &str,
    ) -> bool {
        self.button(ui, atlas, id, slot.rect, label, true, None) && slot.inside
    }

    /// An on/off button of the list; true when its value flipped.
    #[allow(clippy::too_many_arguments)]
    fn live_toggle(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
        name_key: &str,
        id: &str,
        slot: Slot,
        value: &mut bool,
    ) -> bool {
        let label = option_label(lang, name_key, on_off(lang, *value));
        let clicked = self.live_button(ui, atlas, id, slot, &label);
        if clicked {
            *value = !*value;
        }
        clicked
    }

    /// A slider of the list; its new fraction when it moved. A row that is
    /// only partly visible is not drawn as a slider, only registered.
    fn live_slider(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        id: &str,
        slot: Slot,
        label: &str,
        fraction: f32,
    ) -> Option<f32> {
        if slot.inside {
            self.slider(ui, atlas, id, slot.rect, label, fraction)
        } else {
            self.rects.insert(id.to_string(), slot.rect);
            None
        }
    }
}

/// The name key and the flag of an entry that is a plain on/off button.
fn on_off_option(video: &mut VideoOptions, entry: VideoEntry) -> Option<(&'static str, &mut bool)> {
    Some(match entry {
        VideoEntry::ViewBobbing => ("options.viewBobbing", &mut video.view_bobbing),
        VideoEntry::Fullscreen => ("options.fullscreen", &mut video.fullscreen),
        VideoEntry::Vsync => ("options.vsync", &mut video.vsync),
        VideoEntry::BlockAlternatives => {
            ("options.blockAlternatives", &mut video.block_alternatives)
        }
        VideoEntry::EntityShadows => ("options.entityShadows", &mut video.entity_shadows),
        _ => return None,
    })
}

/// "Name: value" for an option whose value is one of `keys`
/// (`GameSettings.getTranslation`: an index out of range reads as 0).
fn cycle_label(lang: &Lang, name_key: &str, keys: &[&str], value: u8) -> String {
    let key = keys.get(usize::from(value)).unwrap_or(&keys[0]);
    option_label(lang, name_key, lang.tr(key))
}

/// The value of the brightness slider (`GameSettings.getKeyBinding(GAMMA)`):
/// "Moody" at 0, "Bright" at 1, else "+N%".
pub(super) fn gamma_value(lang: &Lang, gamma: f32) -> String {
    if gamma == 0.0 {
        lang.tr("options.gamma.min").to_string()
    } else if gamma == 1.0 {
        lang.tr("options.gamma.max").to_string()
    } else {
        format!("+{}%", (gamma * 100.0) as i32)
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The video settings page (`GuiVideoSettings`).

use umicraft_ui::{FontAtlas, Rect, Ui};

use super::layout::{Gui, BUTTON_H, HALF_W, VIDEO_ROW};
use super::{OptionsScreen, Outcome};
use crate::options::{Options, MAX_FPS, MAX_PARTICLE_SETTING, RENDER_DISTANCE, UNLIMITED_FPS};

impl OptionsScreen {
    /// `GuiVideoSettings`.
    pub(super) fn show_video(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        gui: Gui,
        outcome: &mut Outcome,
    ) {
        let (top, bottom) = (32.0, gui.height - 32.0);
        let cx = gui.cx();
        let rows = 8;
        let content = rows as f32 * VIDEO_ROW;
        self.scroll_list(ui, content, bottom - top, VIDEO_ROW);
        Self::list_background(ui, gui, top, bottom);

        // The list is clipped to its area; a row is only clickable while it
        // lies inside.
        ui.push_clip(gui.rect(0.0, top, gui.width, bottom - top));
        for index in 0..rows * 2 {
            let x = cx - 155.0 + (index % 2) as f32 * 160.0;
            let y = top + 4.0 - self.scroll + (index / 2) as f32 * VIDEO_ROW;
            let rect = gui.rect(x, y, HALF_W, BUTTON_H);
            let inside = y >= top && y + BUTTON_H <= bottom;
            self.video_option(ui, atlas, options, index, rect, inside, outcome);
        }
        ui.pop_clip();
        Self::scrollbar(ui, gui, cx + 156.0, (top, bottom), content, self.scroll);

        Self::title(ui, atlas, gui, "Video Settings", 5.0);
        let done = gui.rect(cx - 100.0, gui.height - 27.0, 200.0, BUTTON_H);
        self.back(ui, atlas, done);
    }

    /// One entry of `GuiVideoSettings.videoOptions`.
    #[allow(clippy::too_many_arguments)]
    fn video_option(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        index: usize,
        rect: Rect,
        inside: bool,
        outcome: &mut Outcome,
    ) {
        match index {
            1 => {
                let span = f32::from(RENDER_DISTANCE.end() - RENDER_DISTANCE.start());
                let distance = options.video.render_distance;
                let fraction = f32::from(distance - RENDER_DISTANCE.start()) / span;
                let label = format!("Render Distance: {distance} chunks");
                if inside {
                    if let Some(value) = self.slider(ui, atlas, "distance", rect, &label, fraction)
                    {
                        options.video.render_distance =
                            RENDER_DISTANCE.start() + (value * span).round() as u8;
                        outcome.changed = true;
                    }
                } else {
                    self.rects.insert("distance".to_string(), rect);
                }
            }
            3 => {
                let span = (MAX_FPS.end() - MAX_FPS.start()) as f32;
                let fps = options.video.max_fps;
                let fraction = (fps - MAX_FPS.start()) as f32 / span;
                let label = if fps >= UNLIMITED_FPS {
                    "Max Framerate: Unlimited".to_string()
                } else {
                    format!("Max Framerate: {fps} fps")
                };
                if inside {
                    if let Some(value) = self.slider(ui, atlas, "fps", rect, &label, fraction) {
                        // `GameSettings.snapToStep`: steps of 10.
                        let steps = (value * span / 10.0).round() as u32;
                        options.video.max_fps = MAX_FPS.start() + steps * 10;
                        outcome.changed = true;
                    }
                } else {
                    self.rects.insert("fps".to_string(), rect);
                }
            }
            9 => {
                // `GameSettings.setOptionValue(PARTICLES)`: cycles 0, 1, 2.
                let label = format!("Particles: {}", options.video.particle_setting().label());
                if self.button(ui, atlas, "video:9", rect, &label, true, None) && inside {
                    options.video.particles =
                        (options.video.particles + 1) % (MAX_PARTICLE_SETTING + 1);
                    outcome.changed = true;
                }
            }
            _ => {
                let id = format!("video:{index}");
                self.button(ui, atlas, &id, rect, VIDEO_DISABLED[index], true, None);
            }
        }
    }
}

/// `GuiVideoSettings.videoOptions` without a backend: the label with the
/// vanilla default, in the vanilla order (1 and 3 are live sliders).
const VIDEO_DISABLED: [&str; 16] = [
    "Graphics: Fancy",
    "Render Distance",
    "Smooth Lighting: Maximum",
    "Max Framerate",
    "3D Anaglyph: OFF",
    "View Bobbing: ON",
    "GUI Scale: Auto",
    "Brightness: Moody",
    "Clouds: Fancy",
    "Particles: All",
    "Fullscreen: OFF",
    "Use VSync: ON",
    "Mipmap Levels: 4",
    "Allow Block Alternatives: ON",
    "Use VBOs: OFF",
    "Entity Shadows: ON",
];

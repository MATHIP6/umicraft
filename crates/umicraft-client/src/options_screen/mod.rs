// SPDX-License-Identifier: MIT OR Apache-2.0

//! The options screens, laid out like Minecraft 1.8.8 (M10, UI-3).
//!
//! The pages replicate `GuiOptions`, `GuiVideoSettings`, `GuiControls` and
//! `ScreenChatOptions`: every coordinate below is in GUI units (the
//! `ScaledResolution` of the window) and multiplied by the GUI scale when
//! drawn. A vanilla option the client has no backend for yet is drawn in its
//! place, enabled but inert, with the vanilla default value.
//!
//! [`OptionsScreen`] draws through a [`Ui`] and a [`FontAtlas`], reads no
//! file, clock or network, and edits the [`Options`] it is given. The window
//! saves the options and tells the server about the changes when the screen
//! closes. A key rebinding is a two-step flow: a click on the key button
//! starts the capture, and the next key press is handed to
//! [`OptionsScreen::capture_key`] by the window.

use std::collections::HashMap;

use umicraft_ui::game_menu::draw_default_background;
use umicraft_ui::widgets::{button_tinted, slider, SliderState};
use umicraft_ui::{Color, FontAtlas, Rect, Ui};
use winit::keyboard::KeyCode;

use crate::options::{Action, Options};

mod chat_page;
mod controls_page;
mod layout;
mod main_page;
#[cfg(test)]
mod tests;
mod video_page;

use layout::Gui;

/// One page of the options screens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// `GuiOptions`: the hub.
    Main,
    /// `GuiVideoSettings`.
    Video,
    /// `GuiControls`.
    Controls,
    /// `ScreenChatOptions`.
    Chat,
}

/// What one frame of the screen did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Outcome {
    /// An option changed in this frame.
    pub changed: bool,
    /// The player pressed Done on the hub page.
    pub done: bool,
}

/// The state of the options screens.
#[derive(Debug, Clone)]
pub struct OptionsScreen {
    page: Page,
    capturing: Option<Action>,
    /// Scroll offset of the list of the page, in GUI units.
    scroll: f32,
    sliders: HashMap<String, SliderState>,
    /// The rectangle of every widget in the last frame, by id.
    rects: HashMap<String, Rect>,
}

impl Default for OptionsScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl OptionsScreen {
    /// Creates the screen on the hub page.
    #[must_use]
    pub fn new() -> Self {
        Self {
            page: Page::Main,
            capturing: None,
            scroll: 0.0,
            sliders: HashMap::new(),
            rects: HashMap::new(),
        }
    }

    /// The page that is shown.
    #[must_use]
    pub fn page(&self) -> Page {
        self.page
    }

    /// The action waiting for a key, if a rebinding is under way.
    #[must_use]
    pub fn capturing(&self) -> Option<Action> {
        self.capturing
    }

    /// Gives up a rebinding without changing the key.
    pub fn cancel_capture(&mut self) {
        self.capturing = None;
    }

    /// Goes back to the hub page, as when the screen is opened again.
    pub fn reset(&mut self) {
        self.page = Page::Main;
        self.capturing = None;
        self.scroll = 0.0;
    }

    /// The rectangle of a widget in the last frame (`"done"`, `"nav:video"`,
    /// `"key:forward"`, `"fov"`, …), so a driver can click it.
    #[must_use]
    pub fn rect_of(&self, id: &str) -> Option<Rect> {
        self.rects.get(id).copied()
    }

    /// Binds `key` to the action that is waiting for one. A key the client
    /// keeps for itself is refused and the capture goes on. Returns whether
    /// the binding changed.
    pub fn capture_key(&mut self, options: &mut Options, key: KeyCode) -> bool {
        let Some(action) = self.capturing else {
            return false;
        };
        if options.keys.set(action, key) {
            self.capturing = None;
            true
        } else {
            false
        }
    }

    /// Draws one frame of the screen and edits `options`.
    ///
    /// `ui` must be between [`Ui::begin_frame`] and [`Ui::end_frame`].
    pub fn show(&mut self, ui: &mut Ui, atlas: &FontAtlas, options: &mut Options) -> Outcome {
        self.rects.clear();
        let gui = Gui::new(ui.viewport());
        let mut outcome = Outcome::default();
        draw_default_background(ui);
        ui.push_id("options");
        match self.page {
            Page::Main => self.show_main(ui, atlas, options, gui, &mut outcome),
            Page::Video => self.show_video(ui, atlas, options, gui, &mut outcome),
            Page::Controls => self.show_controls(ui, atlas, options, gui, &mut outcome),
            Page::Chat => self.show_chat(ui, atlas, options, gui, &mut outcome),
        }
        ui.pop_id();
        outcome
    }

    /// Draws a button and reports whether it was clicked.
    #[allow(clippy::too_many_arguments)]
    fn button(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        id: &str,
        rect: Rect,
        label: &str,
        enabled: bool,
        tint: Option<Color>,
    ) -> bool {
        self.rects.insert(id.to_string(), rect);
        let widget = ui.id(id);
        button_tinted(ui, widget, rect, atlas, label, enabled, tint).clicked
    }

    /// Draws a slider and returns its new fraction when it moved.
    fn slider(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        id: &str,
        rect: Rect,
        label: &str,
        fraction: f32,
    ) -> Option<f32> {
        self.rects.insert(id.to_string(), rect);
        let widget = ui.id(id);
        let mut state = self.sliders.remove(id).unwrap_or_default();
        let (value, changed) = slider(ui, widget, rect, atlas, label, fraction, &mut state);
        self.sliders.insert(id.to_string(), state);
        changed.then_some(value)
    }

    /// The bottom "Done" of the sub-pages: back to the hub.
    fn back(&mut self, ui: &mut Ui, atlas: &FontAtlas, rect: Rect) {
        if self.button(ui, atlas, "done", rect, "Done", true, None) {
            self.page = Page::Main;
            self.capturing = None;
            self.scroll = 0.0;
        }
    }
}

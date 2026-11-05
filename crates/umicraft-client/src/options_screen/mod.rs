// SPDX-License-Identifier: MIT OR Apache-2.0

//! The options screens, laid out like Minecraft 1.8.8 (M10, UI-3).
//!
//! The pages replicate `GuiOptions`, `GuiVideoSettings`, `GuiControls`,
//! `ScreenChatOptions`, `GuiCustomizeSkin`, `GuiScreenOptionsSounds`,
//! `GuiScreenResourcePacks` and `GuiLanguage`: every coordinate below is in
//! GUI units (the `ScaledResolution` of the window) and multiplied by the GUI
//! scale when drawn. The hub buttons of the pages the client does not have
//! (Super Secret Settings, Broadcast, Snooper) are drawn in their place and
//! inert.
//!
//! [`OptionsScreen`] draws through a [`Ui`] and a [`FontAtlas`], reads no
//! file, clock or network, and edits the [`Options`] it is given. The window
//! saves the options and tells the server about the changes when the screen
//! closes. A key rebinding is a two-step flow: a click on the key button
//! starts the capture, and the next key press is handed to
//! [`OptionsScreen::capture_key`] by the window.

use std::collections::HashMap;

use umicraft_core::lang::Lang;
use umicraft_ui::game_menu::draw_default_background;
use umicraft_ui::menus::draw_menu_background;
use umicraft_ui::widgets::{button_tinted, slider, SliderState};
use umicraft_ui::{Color, FontAtlas, Rect, Ui};

use crate::options::{Action, Binding, Options};

mod catalog;
mod chat_page;
mod controls_page;
mod language_page;
mod layout;
mod main_page;
mod packs_page;
mod skin_page;
mod snooper_page;
mod sounds_page;
#[cfg(test)]
mod tests;
mod video_page;

pub use catalog::Catalog;
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
    /// `GuiCustomizeSkin`.
    Skin,
    /// `GuiScreenOptionsSounds`.
    Sounds,
    /// `GuiScreenResourcePacks`.
    ResourcePacks,
    /// `GuiLanguage`.
    Language,
    /// `GuiSnooper`.
    Snooper,
}

/// What one frame of the screen did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Outcome {
    /// An option changed in this frame.
    pub changed: bool,
    /// The player pressed Done on the hub page.
    pub done: bool,
    /// The player picked another language: the window rebuilds its `Lang`.
    pub language_changed: bool,
    /// The player left the resource packs page with another selection: the
    /// window reloads the resources.
    pub packs_changed: bool,
    /// A skin layer was switched: the server is told when the screen closes.
    pub skin_changed: bool,
    /// The player asked to open the folder of the resource packs.
    pub open_packs_folder: bool,
}

/// The state of the options screens.
#[derive(Debug, Clone)]
pub struct OptionsScreen {
    page: Page,
    capturing: Option<Action>,
    /// Scroll offset of the list of the page, in GUI units.
    scroll: f32,
    sliders: HashMap<String, SliderState>,
    /// The languages and packs the pages choose from.
    catalog: Catalog,
    /// The selected packs of the resource packs page, highest priority first.
    pack_draft: Vec<String>,
    /// Scroll offsets of the available and selected pack lists.
    pack_scroll: [f32; 2],
    /// The rectangle of every widget in the last frame, by id.
    rects: HashMap<String, Rect>,
    /// Whether a world is behind the screen (opened from the game menu):
    /// the translucent gradient instead of the dirt background.
    over_world: bool,
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
            catalog: Catalog::default(),
            pack_draft: Vec::new(),
            pack_scroll: [0.0; 2],
            rects: HashMap::new(),
            over_world: false,
        }
    }

    /// Chooses the background: the gradient over a running session (options
    /// opened from the game menu), the dirt of `drawBackground` otherwise.
    pub fn set_over_world(&mut self, over_world: bool) {
        self.over_world = over_world;
    }

    /// Gives the language and resource pack pages what they list. The window
    /// does this each time it opens the options, so a pack added meanwhile
    /// shows up.
    pub fn set_catalog(&mut self, catalog: Catalog) {
        self.catalog = catalog;
    }

    /// Opens `page` from the hub, preparing the state it needs.
    fn open(&mut self, page: Page, options: &Options) {
        if page == Page::ResourcePacks {
            self.begin_pack_draft(options);
        }
        self.page = page;
        self.scroll = 0.0;
    }

    /// Opens `page` directly, as the title screen's language button does
    /// (the hub is skipped; Done on a sub-page goes to the hub).
    pub fn open_page(&mut self, page: Page, options: &Options) {
        self.open(page, options);
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

    /// Binds a key or mouse button to the action that is waiting for one
    /// (`GuiControls.keyTyped` and `mouseClicked`). A key the client keeps
    /// for itself is refused and the capture goes on. Returns whether the
    /// binding changed.
    pub fn capture(&mut self, options: &mut Options, binding: Binding) -> bool {
        let Some(action) = self.capturing else {
            return false;
        };
        if options.keys.set(action, binding) {
            self.capturing = None;
            true
        } else {
            false
        }
    }

    /// Draws one frame of the screen and edits `options`.
    ///
    /// `ui` must be between [`Ui::begin_frame`] and [`Ui::end_frame`].
    pub fn show(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        lang: &Lang,
    ) -> Outcome {
        self.rects.clear();
        let gui = Gui::new(ui.viewport());
        let mut outcome = Outcome::default();
        if self.over_world {
            draw_default_background(ui);
        } else {
            draw_menu_background(ui);
        }
        ui.push_id("options");
        match self.page {
            Page::Main => self.show_main(ui, atlas, options, gui, lang, &mut outcome),
            Page::Video => self.show_video(ui, atlas, options, gui, lang, &mut outcome),
            Page::Controls => self.show_controls(ui, atlas, options, gui, lang, &mut outcome),
            Page::Chat => self.show_chat(ui, atlas, options, gui, lang, &mut outcome),
            Page::Skin => self.show_skin(ui, atlas, options, gui, lang, &mut outcome),
            Page::Sounds => self.show_sounds(ui, atlas, options, gui, lang, &mut outcome),
            Page::ResourcePacks => self.show_packs(ui, atlas, options, gui, lang, &mut outcome),
            Page::Language => self.show_language(ui, atlas, options, gui, lang, &mut outcome),
            Page::Snooper => self.show_snooper(ui, atlas, options, gui, lang, &mut outcome),
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

    /// The bottom "Done" (`gui.done`) of the sub-pages: back to the hub.
    fn back(&mut self, ui: &mut Ui, atlas: &FontAtlas, rect: Rect, lang: &Lang) {
        if self.button(ui, atlas, "done", rect, lang.tr("gui.done"), true, None) {
            self.page = Page::Main;
            self.capturing = None;
            self.scroll = 0.0;
        }
    }
}

/// The label of an option button, as `GameSettings.getKeyBinding` builds it:
/// the translated option name, `": "`, then the value.
fn option_label(lang: &Lang, key: &str, value: &str) -> String {
    format!("{}: {value}", lang.tr(key))
}

/// `options.on` / `options.off` for a boolean option.
fn on_off(lang: &Lang, value: bool) -> &str {
    lang.tr(if value { "options.on" } else { "options.off" })
}

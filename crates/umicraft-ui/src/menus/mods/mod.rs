// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mods menu, opened with the right Shift key (ADR-0065).
//!
//! It opens on the **list**: a panel with a search box, one tab per category
//! and a row per mod, each with an on/off switch and a button that opens the
//! **settings** page of that mod, generated from its [`SettingView`]s.
//!
//! Escape steps back from the settings to the list; on the list, and with the
//! cross of the panel, it is the menu that closes. The widgets of the enabled
//! HUD mods are drawn over the dark sheet on both pages and can be dragged with
//! the mouse anywhere but on a button or on the open panel (see [`hud_layer`]).
//!
//! The screen owns only what the player is doing (page, query, tab, scroll);
//! the mods themselves come in as a [`ModsView`] each frame and every change
//! leaves as a [`ModsOutcome`] for the client to apply, so this crate never
//! depends on the mods crate.

mod animation;
mod button;
mod card;
mod color;
mod hud_layer;
mod layout;
mod list;
mod motion;
mod pages;
mod scrollbar;
mod search;
mod settings_panel;
mod style;
#[cfg(test)]
mod tests;
mod view;

pub use hud_layer::{HudEditorOutcome, HudEditorView, HudWidgetView, Snap};
pub use layout::{mods_layout, ModsLayout};
pub use view::{ModCardView, ModValue, ModsOutcome, ModsView, SettingControl, SettingView};

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::geom::Rect;
use crate::menus::common::escape_pressed;
use crate::modern::theme::with_opacity;
use crate::modern::SliderDrag;
use crate::text::FontAtlas;

use animation::{BLUR, OPEN_TICKS, SHEET};
use hud_layer::{HudLayer, HudZones};
use motion::Motion;
use scrollbar::BarGrab;
use style::BACKDROP;

/// Ticks per half period of the caret blink of the search box.
const BLINK_TICKS: u32 = 10;

/// The page of the menu the player is on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
enum Page {
    /// The panel with the search box, the tabs and the cards.
    #[default]
    List,
    /// The settings of the mod with this id.
    Settings(String),
}

/// The mods menu state. The flow makes a new one each time the menu opens, so
/// the opening animation plays again, the search starts empty and the list is
/// shown.
#[derive(Debug, Clone, Default)]
pub struct ModsScreen {
    page: Page,
    /// Ticks since the menu opened, up to [`OPEN_TICKS`].
    ticks: u8,
    blink: u32,
    query: String,
    /// `ALL_TAB`, or the index of the selected category plus one.
    tab: usize,
    scroll: f32,
    /// Scroll offset of the settings page, in GUI units.
    settings_scroll: f32,
    drag: SliderDrag,
    /// The drag of the thumb of the scrollbar of the page.
    bar: BarGrab,
    hud: HudLayer,
    /// The knobs of the switches, the hover of the rows and the entrance of
    /// the pages.
    motion: Motion,
    /// The page whose entrance has been started.
    entered: Page,
}

impl ModsScreen {
    /// Creates the menu on its list, the page coming in.
    #[must_use]
    pub fn new() -> Self {
        let mut screen = Self::default();
        screen.motion.restart();
        screen
    }

    /// Whether Escape steps back inside the menu (from the settings to the
    /// list) instead of closing it, so the window asks before it takes the key.
    #[must_use]
    pub fn can_go_back(&self) -> bool {
        matches!(self.page, Page::Settings(_))
    }

    /// Ends the entrance of the page at once: the panel is fully there and the
    /// rows are in place. A capture of a single frame needs it, the game does
    /// not.
    pub fn settle(&mut self) {
        self.motion.skip_entrance();
    }

    /// Ticks since the menu opened, `partial_tick` of the way into the current
    /// one; it stands still once the animation is over.
    fn elapsed(&self, partial_tick: f32) -> f32 {
        let partial = if self.ticks < OPEN_TICKS {
            partial_tick.clamp(0.0, 1.0)
        } else {
            0.0
        };
        f32::from(self.ticks) + partial
    }

    /// How blurred the game behind should be, `0.0` to `1.0`: the renderer
    /// fades the blur in with it.
    #[must_use]
    pub fn blur_strength(&self, partial_tick: f32) -> f32 {
        BLUR.at(self.elapsed(partial_tick))
    }

    /// Advances the opening animation, the motion of the widgets and the caret
    /// blink by one game tick.
    pub fn tick(&mut self) {
        self.ticks = (self.ticks + 1).min(OPEN_TICKS);
        self.blink = self.blink.wrapping_add(1);
        self.motion.tick();
    }

    /// One page back: the settings to the list.
    fn go_back(&mut self) {
        if matches!(self.page, Page::Settings(_)) {
            self.page = Page::List;
        }
        self.settings_scroll = 0.0;
        self.motion.settle("settings_scroll");
        self.drag = SliderDrag::default();
        self.bar = BarGrab::default();
    }

    /// Draws one frame of the menu over `view` and returns what the player
    /// did. `fonts` are the font of the HUD (the widgets are drawn with it) and
    /// the brand font of the menus; `snap` snaps a dragged widget;
    /// `partial_tick` is how far the tick has come.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        fonts: (&FontAtlas, &FontAtlas),
        lang: &Lang,
        view: &ModsView,
        (snap, partial_tick): (Snap<'_>, f32),
    ) -> Option<ModsOutcome> {
        let viewport = ui.viewport();
        let brand = fonts.1;
        self.motion.set_partial(partial_tick);
        // A mod that vanished leaves its settings page for the list.
        if matches!(self.page, Page::Settings(_)) && self.open_card(view).is_none() {
            self.page = Page::List;
        }
        // Read before the pages run, so one press steps back one page only.
        let escape = self.can_go_back() && escape_pressed(ui);
        let sheet = with_opacity(BACKDROP, SHEET.at(self.elapsed(partial_tick)));
        ui.draw().rect(viewport, sheet);

        let blocked: [Rect; 1] = [mods_layout(viewport).panel];
        let zones = HudZones {
            snap,
            blocked: &blocked,
            bar: true,
        };
        let hud = self.hud.show(ui, fonts, lang, &view.hud, zones);
        // The gear of a widget opens the settings of its mod; the client never sees it.
        let opened = match &hud {
            Some(HudEditorOutcome::OpenSettings { id }) => Some(id.clone()),
            _ => None,
        };

        ui.push_id("mods_menu");
        // The panel fades in as a page comes in.
        ui.draw().set_opacity(self.motion.panel_opacity());
        let page = self.panel_page(ui, brand, lang, view);
        ui.draw().set_opacity(1.0);
        ui.pop_id();
        if escape {
            self.go_back();
        }
        if let Some(id) = opened {
            self.page = Page::Settings(id);
            self.settings_scroll = 0.0;
            self.motion.settle("settings_scroll");
            self.drag = SliderDrag::default();
            self.bar = BarGrab::default();
        }
        // A new page plays its entrance, from the next frame.
        if self.entered != self.page {
            self.entered = self.page.clone();
            self.motion.restart();
        }
        page.or(hud
            .filter(|outcome| !matches!(outcome, HudEditorOutcome::OpenSettings { .. }))
            .map(ModsOutcome::Hud))
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mods menu, opened with the right Shift key (ADR-0065).
//!
//! It is a small state machine of pages over the blurred game:
//!
//! 1. the **landing** page: the logo, the "UMICRAFT" wordmark, one **Mods**
//!    button and a muted hint, with a close cross in the top right corner of
//!    the screen (laid out like the Lunar Client menu);
//! 2. the **list**: a panel with a search box, one tab per category and a grid
//!    of cards, each with an on/off switch and a button that opens
//! 3. the **settings** page of that mod, generated from its [`SettingView`]s.
//!
//! Escape and the cross of the panel step back one page; Escape on the landing
//! page is the window's, which closes the menu. On every page the widgets of the
//! enabled HUD mods are drawn over the dark sheet and can be dragged with the
//! mouse anywhere but on a button or on the open panel (see [`hud_layer`]).
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
mod landing;
mod landing_layout;
mod layout;
mod list;
mod pages;
mod search;
mod settings_panel;
mod style;
#[cfg(test)]
mod tests;
mod view;

pub use hud_layer::{HudEditorOutcome, HudEditorView, HudWidgetView, Snap};
pub use landing_layout::{landing_layout, LandingLayout};
pub use layout::{mods_layout, ModsLayout};
pub use view::{ModCardView, ModValue, ModsOutcome, ModsView, SettingControl, SettingView};

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::geom::Rect;
use crate::menus::common::escape_pressed;
use crate::modern::theme::with_opacity;
use crate::modern::{HoverFade, SliderDrag};
use crate::text::FontAtlas;

use animation::{BLUR, OPEN_TICKS, SHEET};
use hud_layer::{HudLayer, HudZones};
use style::BACKDROP;

/// Ticks per half period of the caret blink of the search box.
const BLINK_TICKS: u32 = 10;

/// The page of the menu the player is on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
enum Page {
    /// The logo, the Mods button and the cross.
    #[default]
    Landing,
    /// The panel with the search box, the tabs and the cards.
    List,
    /// The settings of the mod with this id.
    Settings(String),
}

/// The mods menu state. The flow makes a new one each time the menu opens, so
/// the opening animation plays again, the search starts empty and the landing
/// page is shown.
#[derive(Debug, Clone, Default)]
pub struct ModsScreen {
    page: Page,
    /// Hover fades of the Mods button and the cross of the landing page.
    fades: [HoverFade; 2],
    hovered: [bool; 2],
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
    hud: HudLayer,
}

impl ModsScreen {
    /// Creates the menu on its landing page.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether Escape steps back inside the menu (from the settings to the
    /// list, from the list to the landing page) instead of closing it, so the
    /// window asks before it takes the key.
    #[must_use]
    pub fn can_go_back(&self) -> bool {
        self.page != Page::Landing
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

    /// Advances the opening animation, the hover fades and the caret blink by
    /// one game tick.
    pub fn tick(&mut self) {
        self.ticks = (self.ticks + 1).min(OPEN_TICKS);
        self.blink = self.blink.wrapping_add(1);
        for (fade, hovered) in self.fades.iter_mut().zip(self.hovered) {
            fade.tick(hovered);
        }
    }

    /// One page back: settings to the list, the list to the landing page.
    fn go_back(&mut self) {
        self.page = match self.page {
            Page::Settings(_) => Page::List,
            Page::List | Page::Landing => Page::Landing,
        };
        self.settings_scroll = 0.0;
        self.drag = SliderDrag::default();
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
        // A mod that vanished leaves its settings page for the list.
        if matches!(self.page, Page::Settings(_)) && self.open_card(view).is_none() {
            self.page = Page::List;
        }
        // Read before the pages run, so one press steps back one page only.
        let escape = self.can_go_back() && escape_pressed(ui);
        let sheet = with_opacity(BACKDROP, SHEET.at(self.elapsed(partial_tick)));
        ui.draw().rect(viewport, sheet);

        let landing = landing_layout(viewport);
        let on_landing = self.page == Page::Landing;
        let landing_blocked = [landing.mods, landing.close];
        let panel_blocked = [mods_layout(viewport).panel];
        let blocked: &[Rect] = if on_landing {
            &landing_blocked
        } else {
            &panel_blocked
        };
        let zones = HudZones {
            snap,
            blocked,
            bar: on_landing,
        };
        let hud = self.hud.show(ui, fonts, lang, &view.hud, zones);

        ui.push_id("mods_menu");
        let page = if on_landing {
            self.landing_page(ui, brand, lang, view, (&landing, partial_tick))
        } else {
            self.panel_page(ui, brand, lang, view)
        };
        ui.pop_id();
        if escape {
            self.go_back();
        }
        page.or(hud.map(ModsOutcome::Hud))
    }
}

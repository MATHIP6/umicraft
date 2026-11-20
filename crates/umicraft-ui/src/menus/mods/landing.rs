// SPDX-License-Identifier: MIT OR Apache-2.0

//! The landing page of the mods menu: over the blurred game, with no panel,
//! the logo, the "UMICRAFT" wordmark under it and one **Mods** button, with a
//! muted hint below and, when a HUD mod is on, a line on how to move its widget.
//! A close cross sits in the top right corner of the screen, as on the title
//! screen. The Mods button opens the list; the cross closes the menu.

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::icons::Icon;
use crate::menus::common::Gui;
use crate::menus::title::modern::wordmark::{draw_logo, draw_wordmark};
use crate::menus::title::modern::{lowered, scaled};
use crate::modern::{draw_text, theme, Align};
use crate::text::FontAtlas;

use super::animation::{LOGO, LOGO_RISE, LOGO_START_SCALE};
use super::button::{icon_button, mods_button};
use super::landing_layout::{LandingLayout, WORDMARK_SCALE};
use super::style::hover_level;
use super::view::{ModsOutcome, ModsView};
use super::{ModsScreen, Page};

impl ModsScreen {
    /// Draws the landing page and returns [`ModsOutcome::Close`] when the cross
    /// was clicked; the Mods button switches to the list itself.
    pub(super) fn landing_page(
        &mut self,
        ui: &mut Ui,
        brand: &FontAtlas,
        lang: &Lang,
        view: &ModsView,
        (layout, partial_tick): (&LandingLayout, f32),
    ) -> Option<ModsOutcome> {
        let gui = Gui::new(ui.viewport());
        let logo_in = LOGO.at(self.elapsed(partial_tick));
        let logo_box = lowered(layout.logo, LOGO_RISE, gui.scale, logo_in);
        let logo_box = scaled(
            logo_box,
            LOGO_START_SCALE + (1.0 - LOGO_START_SCALE) * logo_in,
        );
        let logo = ui.menu_art().brand.logo;
        draw_logo(ui, logo, logo_box, logo_in);
        let name_scale = gui.scale * WORDMARK_SCALE;
        draw_wordmark(ui, brand, layout.wordmark, name_scale, 1.0);
        let size = theme::TEXT_SIZE_SMALL * gui.scale;
        let muted = (size, theme::TEXT_MUTED, Align::Center);
        let hint = lang.tr("umicraft.mods.hint");
        draw_text(ui, brand, hint, (layout.hint[0], layout.hint[1]), muted);
        let tip_key = if view.hud.widgets.is_empty() {
            "umicraft.mods.editor.empty"
        } else {
            "umicraft.mods.editor.hint"
        };
        let tip = lang.tr(tip_key);
        draw_text(ui, brand, tip, (layout.tip[0], layout.tip[1]), muted);

        let mods_id = ui.id("mods");
        let close_id = ui.id("close");
        let label = lang.tr("umicraft.mods.button");
        let mods_fade = hover_level(ui, layout.mods);
        let close_fade = hover_level(ui, layout.close);
        let mods = mods_button(
            ui,
            brand,
            (mods_id, layout.mods),
            (gui.scale, mods_fade),
            label,
        );
        let close = icon_button(
            ui,
            brand,
            (close_id, layout.close),
            (Icon::Close, gui.scale, close_fade),
        );
        if mods.clicked {
            self.page = Page::List;
            None
        } else {
            close.clicked.then_some(ModsOutcome::Close)
        }
    }
}

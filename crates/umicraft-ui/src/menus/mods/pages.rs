// SPDX-License-Identifier: MIT OR Apache-2.0

//! The panel pages of the mods menu: the list of cards and the settings of one
//! mod, under a header with the title and the cross that goes back to the landing
//! page.

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::geom::Rect;
use crate::modern::theme;
use crate::modern::{self, search_field, Align, ModernButton, SliderDrag};
use crate::text::FontAtlas;

use super::layout::{mods_layout, ModsLayout};
use super::list::{draw_bar, list, wheel_scrolled};
use super::search::visible;
use super::settings_panel::{content_height, settings_panel};
use super::style::{hover_level, Paint, CLOSE_CROSS, PANEL};
use super::view::{ModCardView, ModsOutcome, ModsView};
use super::{card, ModsScreen, Page, BLINK_TICKS};

impl ModsScreen {
    /// The mod whose settings page is open, if it still exists.
    pub(super) fn open_card<'v>(&self, view: &'v ModsView) -> Option<&'v ModCardView> {
        let Page::Settings(id) = &self.page else {
            return None;
        };
        view.cards.iter().find(|card| card.id == *id)
    }

    /// The panel with its header and cross, and the page inside it. The cross
    /// goes back to the landing page.
    pub(super) fn panel_page(
        &mut self,
        ui: &mut Ui,
        brand: &FontAtlas,
        lang: &Lang,
        view: &ModsView,
    ) -> Option<ModsOutcome> {
        let layout = mods_layout(ui.viewport());
        let paint = Paint {
            brand,
            scale: layout.scale,
        };
        ui.draw()
            .rounded_rect(layout.panel, theme::RADIUS * paint.scale, PANEL);
        let title = (theme::TEXT_SIZE_BODY * 1.3, theme::TEXT, Align::Left);
        let middle = layout.header.y + layout.header.height * 0.5;
        let title_key = if self.open_card(view).is_some() {
            "umicraft.mods.settings"
        } else {
            "umicraft.mods.title"
        };
        paint.text(ui, lang.tr(title_key), (layout.header.x, middle), title);
        let hint = (theme::TEXT_SIZE_SMALL, theme::TEXT_MUTED, Align::Center);
        let hint_at = (layout.hint[0], layout.hint[1]);
        paint.text(ui, lang.tr("umicraft.mods.hint"), hint_at, hint);

        let close_id = ui.id("close");
        let close_fade = hover_level(ui, layout.close);
        let close = ModernButton::new(close_id, layout.close, CLOSE_CROSS).show(
            ui,
            brand,
            paint.scale,
            close_fade,
        );
        let page = match self.open_card(view) {
            Some(card) => self.settings_page(ui, paint, lang, &layout, card),
            None => self.list_page(ui, paint, lang, view, &layout),
        };
        if close.clicked {
            self.page = Page::Landing;
            return None;
        }
        page
    }

    /// The search box, the tabs and the cards.
    pub(super) fn list_page(
        &mut self,
        ui: &mut Ui,
        paint: Paint<'_>,
        lang: &Lang,
        view: &ModsView,
        layout: &ModsLayout,
    ) -> Option<ModsOutcome> {
        let blink_on = (self.blink / BLINK_TICKS).is_multiple_of(2);
        let placeholder = lang.tr("umicraft.mods.search");
        let fields = (&mut self.query, placeholder);
        if search_field(
            ui,
            paint.brand,
            layout.search,
            fields,
            (paint.scale, blink_on),
        ) {
            self.scroll = 0.0;
        }
        let all = lang.tr("umicraft.mods.tab.all");
        let labels: Vec<&str> = std::iter::once(all)
            .chain(view.categories.iter().map(String::as_str))
            .collect();
        let selected = self.tab.min(labels.len() - 1);
        if let Some(tab) =
            modern::tabs(ui, paint.brand, layout.tabs, &labels, selected, paint.scale)
        {
            self.tab = tab;
            self.scroll = 0.0;
        }
        let shown = visible(view, &self.query, self.tab);
        let empty = lang.tr("umicraft.mods.empty");
        let clicked = list(
            ui,
            paint,
            layout.list,
            (view, &shown),
            (&mut self.scroll, empty),
        )?;
        match clicked.action {
            card::CardAction::Toggle(enabled) => Some(ModsOutcome::Toggle {
                id: clicked.id,
                enabled,
            }),
            card::CardAction::OpenSettings => {
                self.page = Page::Settings(clicked.id);
                self.settings_scroll = 0.0;
                self.drag = SliderDrag::default();
                None
            }
        }
    }

    /// The back button, the name of the mod and its settings rows.
    pub(super) fn settings_page(
        &mut self,
        ui: &mut Ui,
        paint: Paint<'_>,
        lang: &Lang,
        layout: &ModsLayout,
        card: &ModCardView,
    ) -> Option<ModsOutcome> {
        let back_id = ui.id("back");
        let fade = hover_level(ui, layout.back);
        let back = ModernButton::new(back_id, layout.back, lang.tr("umicraft.mods.back")).show(
            ui,
            paint.brand,
            paint.scale,
            fade,
        );
        let name_at = (
            layout.back.right() + 6.0 * paint.scale,
            layout.back.y + layout.back.height * 0.5,
        );
        paint.text(
            ui,
            &card.name,
            name_at,
            (theme::TEXT_SIZE_BODY, theme::TEXT, Align::Left),
        );
        let area = Rect {
            y: layout.tabs.y,
            ..layout.detail
        };
        let edit = if card.settings.is_empty() {
            let empty = lang.tr("umicraft.mods.no_settings");
            let at = (area.x + area.width * 0.5, area.y + 12.0 * paint.scale);
            paint.text(
                ui,
                empty,
                at,
                (theme::TEXT_SIZE_BODY, theme::TEXT_MUTED, Align::Center),
            );
            None
        } else {
            let height = content_height(card.settings.len());
            self.settings_scroll = wheel_scrolled(ui, area, height, self.settings_scroll);
            let drag = (&mut self.drag, self.settings_scroll);
            let edit = settings_panel(ui, paint, area, &card.settings, drag);
            draw_bar(ui, area, height, self.settings_scroll);
            edit
        };
        if back.clicked {
            self.go_back();
            return None;
        }
        edit.map(|(key, value)| ModsOutcome::Set {
            id: card.id.clone(),
            key,
            value,
        })
    }
}

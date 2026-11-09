// SPDX-License-Identifier: MIT OR Apache-2.0

//! The panel pages of the mods menu: the list of cards and the settings of one
//! mod, under a header with the logo, the title and the cross that closes the
//! menu.

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::geom::Rect;
use crate::icons::Icon;
use crate::menus::title::modern::wordmark::draw_logo;
use crate::modern::theme;
use crate::modern::{self, fit_text, measure_text, search_field, Align, ModernButton, SliderDrag};
use crate::text::FontAtlas;

use super::button::icon_button;
use super::layout::{mods_layout, ModsLayout};
use super::list::{list, wheel_scrolled};
use super::scrollbar::{draw_scroll_bar, scroll_input, BarGrab};
use super::search::visible;
use super::settings_panel::{content_height, settings_panel};
use super::style::{hover_level, Paint, NAME_SIZE, PANEL, PANEL_RIM};
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
    /// closes the menu.
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
        let radius = theme::RADIUS * paint.scale * 1.5;
        let rim = paint.scale.max(1.0);
        ui.draw().rounded_rect(layout.panel, radius, PANEL_RIM);
        let inner = Rect::new(
            layout.panel.x + rim,
            layout.panel.y + rim,
            layout.panel.width - 2.0 * rim,
            layout.panel.height - 2.0 * rim,
        );
        ui.draw().rounded_rect(inner, radius - rim, PANEL);
        let title = (NAME_SIZE * 1.3, theme::TEXT, Align::Left);
        let middle = layout.header.y + layout.header.height * 0.5;
        let title_key = if self.open_card(view).is_some() {
            "umicraft.mods.settings"
        } else {
            "umicraft.mods.title"
        };
        let logo_box = Rect::new(
            layout.header.x,
            layout.header.y,
            layout.header.height,
            layout.header.height,
        );
        let logo = ui.menu_art().brand.logo;
        draw_logo(ui, logo, logo_box, 1.0);
        let title_x = layout.header.x + (layout.header.height + 4.0 * paint.scale);
        paint.text(ui, lang.tr(title_key), (title_x, middle), title);
        if !view.hud.widgets.is_empty() {
            // How to move the widgets, above the panel, on a dark pill so it
            // reads over a bright world.
            let text = lang.tr("umicraft.mods.editor.hint");
            let size = theme::TEXT_SIZE_SMALL * paint.scale;
            let width = measure_text(brand, text, size) + 10.0 * paint.scale;
            let height = 9.0 * paint.scale;
            let middle_x = layout.panel.x + layout.panel.width * 0.5;
            let top = layout.panel.y - height - 3.0 * paint.scale;
            let pill = Rect::new(middle_x - width * 0.5, top, width, height);
            ui.draw()
                .rounded_rect(pill, height * 0.5, [0.0, 0.0, 0.0, 0.4]);
            let tip = (theme::TEXT_SIZE_SMALL, theme::TEXT, Align::Center);
            paint.text(ui, text, (middle_x, top + height * 0.5), tip);
        }
        let hint = (theme::TEXT_SIZE_SMALL, theme::TEXT_MUTED, Align::Center);
        let hint_at = (layout.hint[0], layout.hint[1]);
        paint.text(ui, lang.tr("umicraft.mods.hint"), hint_at, hint);

        let close_id = ui.id("close");
        let close_fade = hover_level(ui, layout.close);
        let close = icon_button(
            ui,
            brand,
            (close_id, layout.close),
            (Icon::Close, paint.scale, close_fade),
        );
        let page = match self.open_card(view) {
            Some(card) => self.settings_page(ui, paint, lang, &layout, card),
            None => self.list_page(ui, paint, lang, view, &layout),
        };
        if close.clicked {
            return Some(ModsOutcome::Close);
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
        // The line under the selected tab springs from one tab to the next.
        let line = self.motion.spring("tabs", selected as f32).0;
        let tabs = (selected, line);
        if let Some(tab) = modern::tabs(ui, paint.brand, layout.tabs, &labels, tabs, paint.scale) {
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
            (&mut self.scroll, empty, lang.tr("umicraft.mods.settings")),
            (&mut self.motion, &mut self.bar),
        )?;
        match clicked.action {
            card::CardAction::Toggle(enabled) => Some(ModsOutcome::Toggle {
                id: clicked.id,
                enabled,
            }),
            card::CardAction::OpenSettings => {
                self.page = Page::Settings(clicked.id);
                self.settings_scroll = 0.0;
                self.motion.settle("settings_scroll");
                self.drag = SliderDrag::default();
                self.bar = BarGrab::default();
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
        // What the mod does, muted, after its name.
        let name_width = measure_text(paint.brand, &card.name, theme::TEXT_SIZE_BODY * paint.scale);
        let desc_x = name_at.0 + name_width + 6.0 * paint.scale;
        let room = layout.close.x - desc_x - 4.0 * paint.scale;
        let desc = fit_text(
            paint.brand,
            &card.description,
            theme::TEXT_SIZE_SMALL * paint.scale,
            room,
        );
        paint.text(
            ui,
            &desc,
            (desc_x, name_at.1),
            (theme::TEXT_SIZE_SMALL, theme::TEXT_MUTED, Align::Left),
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
            self.settings_scroll =
                scroll_input(ui, area, height, self.settings_scroll, &mut self.bar);
            // The page glides to where the wheel sent it, and follows the thumb at once.
            let shown = self.motion.glide(
                "settings_scroll",
                self.settings_scroll,
                self.bar.is_active(),
            );
            let drag = (&mut self.drag, shown);
            let scope = (card.id.as_str(), &mut self.motion);
            let edit = settings_panel(ui, paint, area, &card.settings, drag, scope);
            draw_scroll_bar(ui, area, height, shown, (self.bar, &mut self.motion));
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

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The panel pages of the mods menu: the list of cards and the settings of one
//! mod, in a panel with the sidebar of categories on its left and a header with
//! the cross that goes back to the landing page.

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::geom::Rect;
use crate::icons::Icon;
use crate::modern::theme::{self, with_opacity};
use crate::modern::{
    fit_text, measure_text, search_field, toggle, Align, SliderDrag, TOGGLE_HEIGHT, TOGGLE_WIDTH,
};
use crate::text::FontAtlas;

use super::button::icon_button;
use super::card::{icon_tile, CardAction};
use super::layout::{mods_layout, ModsLayout};
use super::list::{list, wheel_scrolled};
use super::scrollbar::{draw_scroll_bar, scroll_input, BarGrab};
use super::search::{visible, ALL_TAB};
use super::segmented::segmented;
use super::settings_panel::{content_height, settings_panel};
use super::sidebar::SidebarAction;
use super::style::{
    bordered, hover_level, Paint, DESC_SIZE, FIELD_BORDER, PANEL, PANEL_RIM, TITLE_SIZE,
};
use super::view::{ModCardView, ModsOutcome, ModsView};
use super::{ModsScreen, Page, BLINK_TICKS};

/// Edge of the icon tile in the band of a mod, in GUI units.
const MOD_TILE: f32 = 23.0;
/// Space between that tile and the name.
const TILE_GAP: f32 = 7.0;
/// The switch of the band of a mod is this many times the standard one.
const BAND_TOGGLE: f32 = 1.5;

impl ModsScreen {
    /// The mod whose settings page is open, if it still exists.
    pub(super) fn open_card<'v>(&self, view: &'v ModsView) -> Option<&'v ModCardView> {
        let Page::Settings(id) = &self.page else {
            return None;
        };
        view.cards.iter().find(|card| card.id == *id)
    }

    /// Opens the settings page of the mod `id`, scrolled to the top.
    pub(super) fn open_settings(&mut self, id: String) {
        self.page = Page::Settings(id);
        self.reset_settings_view();
    }

    /// Puts the settings page back at its top with no drag in progress.
    pub(super) fn reset_settings_view(&mut self) {
        self.settings_scroll = 0.0;
        self.motion.settle("settings_scroll");
        self.drag = SliderDrag::default();
        self.bar = BarGrab::default();
    }

    /// The panel with its sidebar, header and cross, and the page inside it.
    /// The cross goes back to the landing page.
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
        bordered(ui, layout.panel, 10.0 * paint.scale, (PANEL, PANEL_RIM));
        let open = self.open_card(view);
        let active = open.map_or(self.tab, |card| card.category + 1);
        let action = self.sidebar(ui, paint, (lang, view, &layout), active);
        let hint = (
            theme::TEXT_SIZE_SMALL,
            with_opacity(theme::TEXT, 0.6),
            Align::Center,
        );
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
        let page = match open {
            Some(card) => self.settings_page(ui, paint, lang, &layout, card),
            None => self.list_page(ui, paint, lang, view, &layout),
        };
        if close.clicked {
            self.page = Page::Landing;
            return None;
        }
        match action {
            Some(SidebarAction::Select(tab)) => {
                self.tab = tab;
                self.scroll = 0.0;
                if matches!(self.page, Page::Settings(_)) {
                    self.page = Page::List;
                    self.reset_settings_view();
                }
                None
            }
            Some(SidebarAction::EditHud) => {
                self.open_editor();
                None
            }
            None => page,
        }
    }

    /// The header, the search box, the All / Enabled control and the cards.
    pub(super) fn list_page(
        &mut self,
        ui: &mut Ui,
        paint: Paint<'_>,
        lang: &Lang,
        view: &ModsView,
        layout: &ModsLayout,
    ) -> Option<ModsOutcome> {
        let title = if self.tab == ALL_TAB {
            lang.tr("umicraft.mods.tab.all")
        } else {
            view.categories.get(self.tab - 1).map_or("", String::as_str)
        };
        self.list_header(ui, paint, lang, view, (layout, title));

        // The border of the field, one pixel around it.
        let rim = Rect::new(
            layout.search.x - 1.0,
            layout.search.y - 1.0,
            layout.search.width + 2.0,
            layout.search.height + 2.0,
        );
        ui.draw()
            .rounded_rect(rim, theme::RADIUS * paint.scale + 1.0, FIELD_BORDER);
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
        let filters = [
            lang.tr("umicraft.mods.filter.all"),
            lang.tr("umicraft.mods.filter.enabled"),
        ];
        let selected = usize::from(self.only_enabled);
        if let Some(chosen) = segmented(ui, paint, layout.filter, &filters, selected) {
            self.only_enabled = chosen == 1;
            self.scroll = 0.0;
        }

        let shown = visible(view, (&self.query, self.tab), self.only_enabled);
        let empty = lang.tr("umicraft.mods.empty");
        let clicked = list(
            ui,
            paint,
            layout.list,
            (view, &shown),
            (&mut self.scroll, empty),
            (&mut self.motion, &mut self.bar),
        )?;
        match clicked.action {
            CardAction::Toggle(enabled) => Some(ModsOutcome::Toggle {
                id: clicked.id,
                enabled,
            }),
            CardAction::OpenSettings => {
                self.open_settings(clicked.id);
                None
            }
        }
    }

    /// The title of the list, the category shown, and how many mods are on.
    fn list_header(
        &self,
        ui: &mut Ui,
        paint: Paint<'_>,
        lang: &Lang,
        view: &ModsView,
        (layout, title): (&ModsLayout, &str),
    ) {
        let middle = layout.header.y + layout.header.height * 0.5;
        let at = (layout.header.x, middle);
        paint.text(ui, title, at, (TITLE_SIZE, theme::TEXT, Align::Left));
        let width = measure_text(paint.brand, title, TITLE_SIZE * paint.scale);
        let on = view.cards.iter().filter(|card| card.enabled).count();
        let count = format!("{on} {}", lang.tr("umicraft.mods.enabled"));
        let count_at = (at.0 + width + 8.0 * paint.scale, middle);
        let muted = (DESC_SIZE, theme::TEXT_MUTED, Align::Left);
        paint.text(ui, &count, count_at, muted);
    }

    /// The back button, the band of the mod and its settings rows.
    pub(super) fn settings_page(
        &mut self,
        ui: &mut Ui,
        paint: Paint<'_>,
        lang: &Lang,
        layout: &ModsLayout,
        card: &ModCardView,
    ) -> Option<ModsOutcome> {
        let back = self.breadcrumb(ui, paint, lang, layout, &card.name);
        let flipped = self.mod_band(ui, paint, layout, card);

        let area = layout.detail;
        let edit = if card.settings.is_empty() {
            let empty = lang.tr("umicraft.mods.no_settings");
            let at = (area.x + area.width * 0.5, area.y + 12.0 * paint.scale);
            let muted = (theme::TEXT_SIZE_BODY, theme::TEXT_MUTED, Align::Center);
            paint.text(ui, empty, at, muted);
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
        if back {
            self.page = Page::List;
            self.reset_settings_view();
            return None;
        }
        if let Some(enabled) = flipped {
            return Some(ModsOutcome::Toggle {
                id: card.id.clone(),
                enabled,
            });
        }
        edit.map(|(key, value)| ModsOutcome::Set {
            id: card.id.clone(),
            key,
            value,
        })
    }

    /// The back button, a slash and the name of the mod at the left of the
    /// header; whether the button was clicked.
    fn breadcrumb(
        &self,
        ui: &mut Ui,
        paint: Paint<'_>,
        lang: &Lang,
        layout: &ModsLayout,
        name: &str,
    ) -> bool {
        let scale = paint.scale;
        let id = ui.id("back");
        let response = ui.response(id, layout.back);
        if response.clicked {
            ui.note_button_press();
        }
        let fill = with_opacity(theme::TEXT, 0.06 + 0.05 * hover_level(ui, layout.back));
        ui.draw().rounded_rect(layout.back, 4.0 * scale, fill);
        let middle = layout.back.y + layout.back.height * 0.5;
        let label = fit_text(
            paint.brand,
            lang.tr("umicraft.mods.back"),
            theme::TEXT_SIZE_BODY * scale,
            layout.back.width - 12.0 * scale,
        );
        let centre = (layout.back.x + layout.back.width * 0.5, middle);
        paint.text(
            ui,
            &label,
            centre,
            (theme::TEXT_SIZE_BODY, theme::TEXT, Align::Center),
        );
        let slash_at = (layout.back.right() + 6.0 * scale, middle);
        let faint = with_opacity(theme::TEXT, 0.45);
        paint.text(
            ui,
            "/",
            slash_at,
            (theme::TEXT_SIZE_BODY, faint, Align::Center),
        );
        let name_at = (slash_at.0 + 6.0 * scale, middle);
        let soft = with_opacity(theme::TEXT, 0.8);
        paint.text(
            ui,
            name,
            name_at,
            (theme::TEXT_SIZE_BODY, soft, Align::Left),
        );
        response.clicked
    }

    /// The band of the mod: its tile, its name and what it does, and its
    /// switch at the right. Returns the state the switch asks for.
    fn mod_band(
        &mut self,
        ui: &mut Ui,
        paint: Paint<'_>,
        layout: &ModsLayout,
        card: &ModCardView,
    ) -> Option<bool> {
        let scale = paint.scale;
        let band = layout.mod_header;
        let (on, speed) = self.motion.level(&format!("on/{}", card.id), card.enabled);
        let tile = Rect::new(band.x, band.y, MOD_TILE * scale, MOD_TILE * scale);
        icon_tile(ui, paint, tile, (&card.id, on));

        let switch = Rect::new(
            band.right() - TOGGLE_WIDTH * BAND_TOGGLE * scale,
            band.y + (MOD_TILE - TOGGLE_HEIGHT * BAND_TOGGLE) * 0.5 * scale,
            TOGGLE_WIDTH * BAND_TOGGLE * scale,
            TOGGLE_HEIGHT * BAND_TOGGLE * scale,
        );
        let left = tile.right() + TILE_GAP * scale;
        let room = switch.x - left - 8.0 * scale;
        let name = fit_text(paint.brand, &card.name, TITLE_SIZE * scale, room);
        let name_at = (left, band.y + 7.0 * scale);
        paint.text(ui, &name, name_at, (TITLE_SIZE, theme::TEXT, Align::Left));
        let desc = fit_text(paint.brand, &card.description, DESC_SIZE * scale, room);
        let desc_at = (left, band.y + 17.0 * scale);
        paint.text(
            ui,
            &desc,
            desc_at,
            (DESC_SIZE, theme::TEXT_MUTED, Align::Left),
        );

        ui.push_id("band");
        let switch_id = ui.id("switch");
        let look = (hover_level(ui, switch), on, speed);
        let flipped = toggle(ui, switch_id, switch, card.enabled, look);
        ui.pop_id();
        flipped
    }
}

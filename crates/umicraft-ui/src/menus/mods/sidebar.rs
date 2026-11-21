// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sidebar of the panel pages: the brand, one item per category with the
//! number of mods in it, and the button that opens the HUD editor.

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::geom::Rect;
use crate::icons::Icon;
use crate::menus::title::modern::wordmark::draw_logo;
use crate::modern::theme::{self, mix, with_opacity};
use crate::modern::{draw_icon, draw_text, fit_text, Align};

use super::layout::{item_rect, ModsLayout};
use super::search::ALL_TAB;
use super::style::{
    hover_level, Paint, ACCENT_HOVER, ACCENT_SOFT, DIVIDER, ITEM_HOVER, LABEL_SIZE, SIDEBAR,
};
use super::view::ModsView;
use super::ModsScreen;

/// Size of the label of an item and of the wordmark of the brand row.
const ITEM_SIZE: f32 = 6.5;
/// Edge of the icon of an item and of the button.
const ITEM_ICON: f32 = 8.0;
/// Space around the content of an item.
const ITEM_PAD: f32 = 5.0;
/// Gap between the icon of an item and its label.
const ICON_GAP: f32 = 5.0;
/// Edge of the logo of the brand row and its space to the name.
const LOGO: f32 = 15.0;

/// What the player did on the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SidebarAction {
    /// The item of this tab (`ALL_TAB`, or the category plus one) was clicked.
    Select(usize),
    /// The button of the HUD editor was clicked.
    EditHud,
}

/// The icon of the category number `index`; the first ones are the HUD, the
/// gameplay and the visual mods, the rest get the sliders.
fn category_icon(index: usize) -> Icon {
    match index {
        0 => Icon::Gauge,
        1 => Icon::Sword,
        2 => Icon::Lightbulb,
        _ => Icon::Sliders,
    }
}

impl ModsScreen {
    /// Draws the sidebar with the item of tab `active` lit and reports a click.
    pub(super) fn sidebar(
        &mut self,
        ui: &mut Ui,
        paint: Paint<'_>,
        (lang, view, layout): (&Lang, &ModsView, &ModsLayout),
        active: usize,
    ) -> Option<SidebarAction> {
        let scale = paint.scale;
        ui.draw().rect(layout.sidebar, SIDEBAR);
        let line = Rect::new(
            layout.sidebar.right() - 1.0,
            layout.sidebar.y,
            1.0,
            layout.sidebar.height,
        );
        ui.draw().rect(line, DIVIDER);
        brand_row(ui, paint, layout.brand);
        let section = (LABEL_SIZE, with_opacity(theme::TEXT, 0.5), Align::Left);
        let section_at = (
            layout.section.x,
            layout.section.y + layout.section.height * 0.5,
        );
        paint.text(
            ui,
            &lang.tr("umicraft.mods.categories").to_uppercase(),
            section_at,
            section,
        );

        let all = lang.tr("umicraft.mods.all_mods");
        let labels = std::iter::once(all).chain(view.categories.iter().map(String::as_str));
        let mut action = None;
        ui.push_id("sidebar");
        for (tab, label) in labels.enumerate() {
            let rect = item_rect(layout.items, scale, tab);
            let (icon, count) = if tab == ALL_TAB {
                (Icon::Grid, view.cards.len())
            } else {
                let count = view.cards.iter().filter(|c| c.category + 1 == tab).count();
                (category_icon(tab - 1), count)
            };
            let id = ui.id(label);
            let response = ui.response(id, rect);
            if response.clicked {
                ui.note_button_press();
                action = Some(SidebarAction::Select(tab));
            }
            item(
                ui,
                paint,
                rect,
                (icon, label, count),
                (tab == active, response.hovered),
            );
        }
        ui.pop_id();

        if !view.hud.entries.is_empty() && self.edit_button(ui, paint, layout.edit, lang) {
            action = Some(SidebarAction::EditHud);
        }
        action
    }

    /// The primary button of the sidebar; whether it was clicked.
    fn edit_button(&mut self, ui: &mut Ui, paint: Paint<'_>, rect: Rect, lang: &Lang) -> bool {
        let scale = paint.scale;
        let id = ui.id("edit_hud");
        let response = ui.response(id, rect);
        if response.clicked {
            ui.note_button_press();
        }
        let fade = hover_level(ui, rect);
        let fill = mix(theme::ACCENT, ACCENT_HOVER, fade);
        ui.draw().rounded_rect(rect, 6.0 * scale, fill);
        let label = fit_text(
            paint.brand,
            lang.tr("umicraft.mods.editor.open"),
            ITEM_SIZE * scale,
            rect.width - (ITEM_ICON + ICON_GAP + 2.0 * ITEM_PAD) * scale,
        );
        let width = crate::modern::measure_text(paint.brand, &label, ITEM_SIZE * scale);
        let group = (ITEM_ICON + ICON_GAP) * scale + width;
        let left = rect.x + (rect.width - group) * 0.5;
        let middle = rect.y + rect.height * 0.5;
        let white = [1.0, 1.0, 1.0, 1.0];
        let icon_at = (left + ITEM_ICON * scale * 0.5, middle);
        draw_icon(
            ui,
            paint.brand,
            Icon::Target,
            icon_at,
            (ITEM_ICON * scale, white),
        );
        let text_at = (left + (ITEM_ICON + ICON_GAP) * scale, middle);
        draw_text(
            ui,
            paint.brand,
            &label,
            text_at,
            (ITEM_SIZE * scale, white, Align::Left),
        );
        response.clicked
    }
}

/// The logo and the wordmark at the top of the sidebar.
fn brand_row(ui: &mut Ui, paint: Paint<'_>, rect: Rect) {
    let scale = paint.scale;
    let logo_box = Rect::new(rect.x, rect.y, LOGO * scale, LOGO * scale);
    let logo = ui.menu_art().brand.logo;
    draw_logo(ui, logo, logo_box, 1.0);
    let at = (rect.x + (LOGO + 5.0) * scale, rect.y + rect.height * 0.5);
    paint.text(ui, "UMICRAFT", at, (ITEM_SIZE, theme::TEXT, Align::Left));
}

/// One item: the icon, the label and the count at the right end.
fn item(
    ui: &mut Ui,
    paint: Paint<'_>,
    rect: Rect,
    (icon, label, count): (Icon, &str, usize),
    (active, hovered): (bool, bool),
) {
    let scale = paint.scale;
    if active {
        ui.draw().rounded_rect(rect, 5.0 * scale, ACCENT_SOFT);
    } else if hovered {
        ui.draw().rounded_rect(rect, 5.0 * scale, ITEM_HOVER);
    }
    let colour = if active {
        theme::TEXT
    } else {
        with_opacity(theme::TEXT, 0.78)
    };
    let middle = rect.y + rect.height * 0.5;
    let icon_at = (rect.x + (ITEM_PAD + ITEM_ICON * 0.5) * scale, middle);
    draw_icon(ui, paint.brand, icon, icon_at, (ITEM_ICON * scale, colour));
    let count_text = count.to_string();
    let count_at = (rect.right() - ITEM_PAD * scale, middle);
    let muted = with_opacity(theme::TEXT, 0.55);
    paint.text(ui, &count_text, count_at, (LABEL_SIZE, muted, Align::Right));
    let left = rect.x + (ITEM_PAD + ITEM_ICON + ICON_GAP) * scale;
    let room = count_at.0 - 12.0 * scale - left;
    let text = fit_text(paint.brand, label, ITEM_SIZE * scale, room);
    paint.text(ui, &text, (left, middle), (ITEM_SIZE, colour, Align::Left));
}

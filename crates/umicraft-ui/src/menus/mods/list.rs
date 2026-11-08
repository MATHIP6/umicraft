// SPDX-License-Identifier: MIT OR Apache-2.0

//! The scrolling grid of mod cards.

use crate::context::Ui;
use crate::geom::Rect;
use crate::menus::common::{draw_scrollbar, wheel_scrolled_by, Gui};
use crate::modern::theme;
use crate::modern::Align;

use super::card::{card, CardAction};
use super::layout::{card_rect, columns, content_height};
use super::style::Paint;
use super::view::ModsView;

/// GUI units one wheel notch scrolls.
const WHEEL_STEP: f32 = 12.0;

/// A click on a card.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ListAction {
    /// Identifier of the mod whose card was clicked.
    pub id: String,
    /// What was clicked on it.
    pub action: CardAction,
}

/// Draws the cards `shown` (indices in `view.cards`) in `area`, scrolled by
/// `offset` GUI units (updated from the wheel), and reports a click. `empty`
/// is the text shown when no card is left.
pub(super) fn list(
    ui: &mut Ui,
    paint: Paint<'_>,
    area: Rect,
    (view, shown): (&ModsView, &[usize]),
    (offset, empty): (&mut f32, &str),
) -> Option<ListAction> {
    if shown.is_empty() {
        let at = (area.x + area.width * 0.5, area.y + 12.0 * paint.scale);
        paint.text(
            ui,
            empty,
            at,
            (theme::TEXT_SIZE_BODY, theme::TEXT_MUTED, Align::Center),
        );
        return None;
    }
    let columns = columns(area.width / paint.scale);
    let height = content_height(shown.len(), columns);
    *offset = wheel_scrolled(ui, area, height, *offset);
    let mut result = None;
    ui.push_clip(area);
    for (slot, &index) in shown.iter().enumerate() {
        let rect = card_rect(area, paint.scale, slot, (columns, *offset));
        if rect.bottom() < area.y || rect.y > area.bottom() {
            continue;
        }
        let Some(item) = view.cards.get(index) else {
            continue;
        };
        let category = view
            .categories
            .get(item.category)
            .map_or("", String::as_str);
        if let Some(action) = card(ui, paint, rect, (item, category)) {
            result = Some(ListAction {
                id: item.id.clone(),
                action,
            });
        }
    }
    ui.pop_clip();
    draw_bar(ui, area, height, *offset);
    result
}

/// The scrollbar in the margin right of the list, when the cards overflow.
/// Applies this frame's wheel input to the scroll `offset` of a list whose
/// content is `height` GUI units tall and shown in `area`. The settings page
/// scrolls with the same step and bounds as the card list.
pub(super) fn wheel_scrolled(ui: &Ui, area: Rect, height: f32, offset: f32) -> f32 {
    wheel_scrolled_by(
        ui,
        &Gui::new(ui.viewport()),
        area,
        height,
        offset,
        WHEEL_STEP,
    )
}

/// Draws the scrollbar right of `area` when the content is taller than it.
pub(super) fn draw_bar(ui: &mut Ui, area: Rect, height: f32, offset: f32) {
    let gui = Gui::new(ui.viewport());
    let [x, y] = gui.to_gui([area.right(), area.y]);
    let view_height = area.height / gui.scale;
    draw_scrollbar(ui, &gui, [x + 1.0, y, view_height], height, offset);
}

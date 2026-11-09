// SPDX-License-Identifier: MIT OR Apache-2.0

//! The scrolling grid of mod cards.

use crate::context::Ui;
use crate::geom::Rect;
use crate::menus::common::{wheel_scrolled_by, Gui};
use crate::modern::theme;
use crate::modern::Align;

use super::card::{card, CardAction};
use super::layout::{card_rect, columns, content_height};
use super::motion::Motion;
use super::scrollbar::{draw_scroll_bar, scroll_input, BarGrab};
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
    (offset, empty, options): (&mut f32, &str, &str),
    (motion, bar): (&mut Motion, &mut BarGrab),
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
    *offset = scroll_input(ui, area, height, *offset, bar);
    // The list glides to where the wheel sent it, and follows the thumb at once.
    let glided = motion.glide("scroll", *offset, bar.is_active());
    let mut result = None;
    ui.push_clip(area);
    for (slot, &index) in shown.iter().enumerate() {
        let mut rect = card_rect(area, paint.scale, slot, (columns, glided));
        // The rows come in one after the other, rising into place.
        let (opacity, rise) = motion.row(slot);
        rect.y += rise * paint.scale;
        if rect.bottom() < area.y || rect.y > area.bottom() {
            continue;
        }
        let Some(item) = view.cards.get(index) else {
            continue;
        };
        ui.draw().set_opacity(opacity);
        if let Some(action) = card(ui, paint, rect, (item, &mut *motion), options) {
            result = Some(ListAction {
                id: item.id.clone(),
                action,
            });
        }
    }
    ui.draw().set_opacity(motion.panel_opacity());
    ui.pop_clip();
    draw_scroll_bar(ui, area, height, glided, (*bar, motion));
    result
}

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

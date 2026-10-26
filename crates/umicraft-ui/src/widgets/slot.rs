// SPDX-License-Identifier: MIT OR Apache-2.0

//! The inventory slot widget.
//!
//! A slot draws one inventory cell: a background rectangle, an item (as text
//! fallback, since real 16×16 sprites belong to a rendering improvement), a
//! stack count badge and a damage bar. The hover highlight and click detection
//! follow the same [`crate::Ui::response`] path as the other M4 widgets.

use crate::context::{Ui, UiResponse};
use crate::draw::{Color, TextStyle};
use crate::geom::Rect;
use crate::id::WidgetId;
use crate::text::FontAtlas;

use super::style;

/// Background of an inventory slot.
pub(crate) const SLOT_BACKGROUND: Color = [0.14, 0.14, 0.18, 1.0];
/// Highlight overlay when the pointer hovers over a slot.
pub(crate) const SLOT_HOVER: Color = [1.0, 1.0, 1.0, 0.25];
/// Colour of the stack count badge.
pub(crate) const SLOT_COUNT: Color = [0.94, 0.95, 0.98, 1.0];
/// Colour of the damage bar background.
pub(crate) const DAMAGE_BAR_BG: Color = [0.0, 0.0, 0.0, 0.8];

/// The item data a slot needs to draw.
#[derive(Debug, Clone)]
pub struct SlotItem {
    /// The item id (used for the text fallback label).
    pub item_id: i32,
    /// The stack count.
    pub count: u8,
    /// The item damage, 0 when undamaged.
    pub damage: i16,
    /// The max durability of the item, 0 when not a tool.
    pub max_durability: i16,
}

/// Draws one inventory slot and returns the interaction state.
///
/// `id` must be stable across frames. The `item` is `None` for an empty slot.
/// When `textured` is true (the GUI texture provides slot backgrounds), the
/// opaque slot fill is skipped so the texture shows through. When false, a
/// solid [`SLOT_BACKGROUND`] rectangle is drawn.
pub fn slot(
    ui: &mut Ui,
    id: WidgetId,
    rect: Rect,
    atlas: &FontAtlas,
    item: Option<&SlotItem>,
    textured: bool,
) -> UiResponse {
    let response = ui.response(id, rect);

    if !textured {
        ui.draw().rect(rect, SLOT_BACKGROUND);
    }

    if let Some(item) = item {
        draw_item(ui, rect, atlas, item);
    }

    if response.hovered {
        ui.draw().rect(rect, SLOT_HOVER);
    }

    response
}

/// Draws the item content inside a slot rectangle.
fn draw_item(ui: &mut Ui, rect: Rect, atlas: &FontAtlas, item: &SlotItem) {
    let scale = style::TEXT_SCALE;

    // Item id label, centred.
    let label = item.item_id.to_string();
    let width = atlas.measure(&label, scale);
    let line_height = atlas.line_height(scale);
    let x = rect.x + (rect.width - width).max(0.0) * 0.5;
    let y = rect.y + (rect.height - line_height).max(0.0) * 0.5;

    ui.push_clip(rect);
    ui.draw()
        .text([x, y], &label, TextStyle::new(scale, style::TEXT));

    // Stack count badge in the bottom-right corner.
    if item.count > 1 {
        let count_label = item.count.to_string();
        let count_width = atlas.measure(&count_label, scale);
        let cx = rect.x + rect.width - count_width - 2.0;
        let cy = rect.y + rect.height - line_height - 2.0;
        ui.draw()
            .text([cx, cy], &count_label, TextStyle::new(scale, SLOT_COUNT));
    }

    // Damage bar at the bottom.
    if item.max_durability > 0 && item.damage > 0 {
        let fraction = 1.0 - (item.damage as f32 / item.max_durability as f32).clamp(0.0, 1.0);
        let bar_height = 2.0;
        let bar_y = rect.y + rect.height - bar_height;
        let bar_rect = Rect::new(rect.x, bar_y, rect.width, bar_height);
        ui.draw().rect(bar_rect, DAMAGE_BAR_BG);

        let fill_width = rect.width * fraction;
        let fill_rect = Rect::new(rect.x, bar_y, fill_width, bar_height);
        let green = fraction;
        let red = 1.0 - fraction;
        ui.draw().rect(fill_rect, [red, green, 0.0, 1.0]);
    }

    ui.pop_clip();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FontAtlas, UiInput};

    #[test]
    fn an_empty_slot_draws_only_the_background() {
        let viewport = Rect::new(0.0, 0.0, 320.0, 180.0);
        let mut ui = Ui::new(viewport);
        ui.begin_frame(viewport, UiInput::default());

        let id = ui.id("slot0");
        let response = slot(
            &mut ui,
            id,
            Rect::new(0.0, 0.0, 32.0, 32.0),
            &FontAtlas::fallback(),
            None,
            false,
        );
        assert!(!response.clicked);
        assert!(!response.hovered);

        let output = ui.end_frame();
        assert!(!output.draw_list.is_empty());
    }

    #[test]
    fn a_slot_with_an_item_draws_the_label_and_count() {
        let viewport = Rect::new(0.0, 0.0, 320.0, 180.0);
        let mut ui = Ui::new(viewport);
        ui.begin_frame(viewport, UiInput::default());

        let id = ui.id("slot1");
        let item = SlotItem {
            item_id: 1,
            count: 64,
            damage: 0,
            max_durability: 0,
        };
        let response = slot(
            &mut ui,
            id,
            Rect::new(0.0, 0.0, 32.0, 32.0),
            &FontAtlas::fallback(),
            Some(&item),
            false,
        );
        assert!(!response.clicked);

        let output = ui.end_frame();
        // Background + item label + count badge = at least 3 draw commands.
        assert!(output.draw_list.commands().len() >= 3);
    }
}

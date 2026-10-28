//! The inventory slot widget.
//!
//! A slot draws one inventory cell: a background rectangle, the item icon with
//! its stack count and durability bar (see [`super::item_overlay`]) and the
//! hover highlight. The slot rectangle is the 16x16 GUI-pixel item area, which
//! is what the layouts in [`super::inventory`] produce. Hover and click
//! detection follow the same [`crate::Ui::response`] path as the other M4
//! widgets.

use crate::context::{Ui, UiResponse};
use crate::draw::Color;
use crate::geom::Rect;
use crate::id::WidgetId;
use crate::text::FontAtlas;

use super::item_overlay::draw_item;

/// Background of an inventory slot.
pub(crate) const SLOT_BACKGROUND: Color = [0.14, 0.14, 0.18, 1.0];
/// Highlight overlay when the pointer hovers over a slot.
pub(crate) const SLOT_HOVER: Color = [1.0, 1.0, 1.0, 0.25];

/// The item data a slot needs to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotItem {
    /// Opaque icon key the renderer resolves to an atlas icon.
    pub key: u64,
    /// The stack count.
    pub count: u8,
    /// The item damage, 0 when undamaged.
    pub damage: i16,
    /// The max durability of the item, 0 when not a tool.
    pub max_durability: i16,
    /// Whether the icon carries the enchantment glint.
    pub glint: bool,
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
        draw_item(ui.draw(), atlas, rect, item);
    }

    if response.hovered {
        ui.draw().rect(rect, SLOT_HOVER);
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DrawCommand, FontAtlas, InputEvent, UiInput};

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

    fn item(count: u8, damage: i16, max_durability: i16, glint: bool) -> SlotItem {
        SlotItem {
            key: 9,
            count,
            damage,
            max_durability,
            glint,
        }
    }

    fn slot_commands(rect: Rect, item: &SlotItem, hovered: bool) -> Vec<DrawCommand> {
        let viewport = Rect::new(0.0, 0.0, 320.0, 180.0);
        let pointer = if hovered {
            [rect.x + 1.0, rect.y + 1.0]
        } else {
            [300.0, 170.0]
        };
        let mut ui = Ui::new(viewport);
        ui.begin_frame(
            viewport,
            UiInput::new(vec![InputEvent::PointerMoved { position: pointer }]),
        );
        let id = ui.id("slot");
        slot(&mut ui, id, rect, &FontAtlas::fallback(), Some(item), false);
        ui.end_frame().draw_list.commands().to_vec()
    }

    #[test]
    fn a_slot_with_an_item_draws_background_icon_overlays_then_hover() {
        let rect = Rect::new(0.0, 0.0, 16.0, 16.0);
        let commands = slot_commands(rect, &item(64, 10, 100, true), true);
        assert!(matches!(commands[0], DrawCommand::Rect { color, .. } if color == SLOT_BACKGROUND));
        assert_eq!(
            commands[1],
            DrawCommand::Item {
                key: 9,
                dst: rect,
                glint: true
            }
        );
        assert!(matches!(commands[2], DrawCommand::Text { .. }));
        assert!(matches!(commands[3], DrawCommand::Text { .. }));
        assert!(matches!(commands[4], DrawCommand::Rect { .. }));
        let last = commands.last().expect("commands");
        assert!(
            matches!(last, DrawCommand::Rect { color, rect: r } if *color == SLOT_HOVER && *r == rect)
        );
    }

    #[test]
    fn an_undamaged_single_item_has_no_overlay() {
        let rect = Rect::new(0.0, 0.0, 16.0, 16.0);
        let commands = slot_commands(rect, &item(1, 0, 0, false), false);
        assert_eq!(commands.len(), 2);
        let commands = slot_commands(rect, &item(1, 0, 100, false), false);
        assert_eq!(commands.len(), 2);
    }

    #[test]
    fn the_icon_fills_the_slot_rect_at_any_scale() {
        for side in [16.0, 32.0, 64.0] {
            let rect = Rect::new(5.0, 6.0, side, side);
            let commands = slot_commands(rect, &item(1, 0, 0, false), false);
            assert!(matches!(commands[1], DrawCommand::Item { dst, .. } if dst == rect));
        }
    }
}

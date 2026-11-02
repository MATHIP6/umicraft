// SPDX-License-Identifier: MIT OR Apache-2.0

//! The hotbar: background, selection cursor, and the item icons with their
//! counts and durability bars (MCP `GuiIngame.renderTooltip`,
//! `renderHotbarItem`).

use umicraft_ui::{draw_item, FontAtlas, Rect, SpriteSheet};

use super::icons::{Canvas, Region};
use crate::hud::{HudState, HOTBAR_SLOTS};

/// The 182x22 background in `widgets.png`.
const BACKGROUND: Region = (0, 0, 182, 22);
/// The selection cursor: 24 wide and 22 high.
const CURSOR: Region = (0, 22, 24, 22);
/// Distance between two slots.
const SLOT_STEP: i32 = 20;

/// Draws the hotbar and the items of `state.hotbar_items`.
///
/// The background sits at `(width / 2 - 91, height - 22)`, the cursor one
/// pixel left and up of the selected slot, and slot `n` shows its item at
/// `(width / 2 - 90 + n * 20 + 2, height - 16 - 3)`.
pub(super) fn draw_hotbar(
    canvas: &mut Canvas<'_>,
    state: &HudState,
    font: &FontAtlas,
    widgets: &SpriteSheet,
) {
    let left = canvas.width / 2 - 91;
    let bottom = canvas.height;
    canvas.sprite(widgets, BACKGROUND, left, bottom - 22);
    let selected = i32::from(state.selected_slot).min(HOTBAR_SLOTS as i32 - 1);
    canvas.sprite(
        widgets,
        CURSOR,
        left - 1 + selected * SLOT_STEP,
        bottom - 23,
    );

    let scale = canvas.scale;
    for (index, item) in state.hotbar_items.iter().enumerate() {
        let Some(item) = item else { continue };
        let x = canvas.width / 2 - 90 + index as i32 * SLOT_STEP + 2;
        let y = bottom - 16 - 3;
        let cell = Rect::new(
            x as f32 * scale,
            y as f32 * scale,
            16.0 * scale,
            16.0 * scale,
        );
        draw_item(canvas.list(), font, cell, item);
    }
}

#[cfg(test)]
mod tests {
    use umicraft_ui::{DrawCommand, DrawList, SlotItem};

    use super::*;

    fn stone() -> SlotItem {
        SlotItem {
            key: 1,
            count: 5,
            damage: 0,
            max_durability: 0,
            glint: false,
        }
    }

    #[test]
    fn the_cursor_is_twenty_two_high_and_follows_the_selection() {
        let mut list = DrawList::new();
        let sheet = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 256, 256);
        let state = HudState {
            selected_slot: 3,
            ..HudState::default()
        };
        let mut canvas = Canvas::new(&mut list, (640, 480));
        draw_hotbar(&mut canvas, &state, &FontAtlas::fallback(), &sheet);
        // 640x480 is scale 2: 320x240 units, hotbar left at 160 - 91 = 69.
        let rects: Vec<Rect> = list
            .commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Image { dst, .. } => Some(*dst),
                _ => None,
            })
            .collect();
        assert_eq!(rects[0], Rect::new(138.0, 436.0, 364.0, 44.0));
        assert_eq!(
            rects[1],
            Rect::new((69.0 - 1.0 + 60.0) * 2.0, 434.0, 48.0, 44.0)
        );
    }

    #[test]
    fn items_sit_three_units_into_their_slot_with_their_count() {
        let mut list = DrawList::new();
        let sheet = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 256, 256);
        let mut state = HudState::default();
        state.hotbar_items[2] = Some(stone());
        let mut canvas = Canvas::new(&mut list, (640, 480));
        draw_hotbar(&mut canvas, &state, &FontAtlas::fallback(), &sheet);
        let item = list.commands().iter().find_map(|c| match c {
            DrawCommand::Item { dst, .. } => Some(*dst),
            _ => None,
        });
        // 160 - 90 + 40 + 2 = 112 units, 240 - 19 = 221 units.
        assert_eq!(item, Some(Rect::new(224.0, 442.0, 32.0, 32.0)));
        let count = list
            .commands()
            .iter()
            .any(|c| matches!(c, DrawCommand::Text { text, .. } if text == "5"));
        assert!(count);
    }
}

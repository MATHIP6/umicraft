// SPDX-License-Identifier: MIT OR Apache-2.0

//! The hotbar: background, selection cursor, and the item icons with their
//! counts and durability bars (MCP `GuiIngame.renderTooltip`,
//! `renderHotbarItem`).

use umicraft_ui::{draw_item_overlays, FontAtlas, Rect, SpriteSheet};

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
        let icon = popup_rect(cell, state.hotbar_popups[index], scale);
        canvas.list().item(item.key, icon, item.glint);
        draw_item_overlays(canvas.list(), font, cell, item);
    }
}

/// The icon rectangle of a bouncing item (`GuiIngame.renderHotbarItem`): with
/// `f` ticks left, the icon is scaled by `1 / (1 + f / 5)` across and
/// `(2 + f / 5) / 2` up, around `(x + 8, y + 12)` so it stays on the slot's
/// floor. Only the icon bounces; count and durability bar stay put. The HUD
/// is rebuilt once per tick, so `partialTicks` is taken as 0.
fn popup_rect(cell: Rect, ticks_left: u8, scale: f32) -> Rect {
    if ticks_left == 0 {
        return cell;
    }
    let stretch = 1.0 + f32::from(ticks_left) / 5.0;
    let (width, height) = (cell.width / stretch, cell.height * (stretch + 1.0) / 2.0);
    let (pivot_x, pivot_y) = (cell.x + 8.0 * scale, cell.y + 12.0 * scale);
    Rect::new(
        pivot_x + (cell.x - pivot_x) / stretch,
        pivot_y + (cell.y - pivot_y) * (stretch + 1.0) / 2.0,
        width,
        height,
    )
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

    #[test]
    fn a_bouncing_item_is_squeezed_across_and_stretched_up_around_its_floor() {
        let cell = Rect::new(224.0, 442.0, 32.0, 32.0);
        assert_eq!(popup_rect(cell, 0, 2.0), cell);
        // Five ticks left: stretch 2, so half as wide and 1.5 times as high,
        // pivoting on (cell.x + 16, cell.y + 24).
        let rect = popup_rect(cell, 5, 2.0);
        assert_eq!(rect, Rect::new(232.0, 442.0 + 24.0 - 36.0, 16.0, 48.0));
    }

    #[test]
    fn the_count_of_a_bouncing_item_is_not_squeezed() {
        let mut list = DrawList::new();
        let sheet = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 256, 256);
        let mut state = HudState::default();
        state.hotbar_items[2] = Some(stone());
        state.hotbar_popups[2] = 5;
        let mut canvas = Canvas::new(&mut list, (640, 480));
        draw_hotbar(&mut canvas, &state, &FontAtlas::fallback(), &sheet);
        let icon = list.commands().iter().find_map(|c| match c {
            DrawCommand::Item { dst, .. } => Some(*dst),
            _ => None,
        });
        assert_eq!(icon.map(|r| r.width), Some(16.0));
        let count = list
            .commands()
            .iter()
            .any(|c| matches!(c, DrawCommand::Text { text, .. } if text == "5"));
        assert!(count);
    }

    #[test]
    fn items_stay_in_their_slots_at_gui_scale_four() {
        // 1920x1080 is GUI scale 4, where the items once drifted away.
        let item_x = |slot: usize| {
            let mut list = DrawList::new();
            let sheet = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 256, 256);
            let mut state = HudState::default();
            state.hotbar_items[slot] = Some(stone());
            let mut canvas = Canvas::new(&mut list, (1920, 1080));
            draw_hotbar(&mut canvas, &state, &FontAtlas::fallback(), &sheet);
            list.commands().iter().find_map(|c| match c {
                DrawCommand::Item { dst, .. } => Some((dst.x, dst.y, dst.width)),
                _ => None,
            })
        };
        assert_eq!(item_x(0), Some((608.0, 1004.0, 64.0)));
        assert_eq!(item_x(8).map(|i| i.0), Some(608.0 + 8.0 * 20.0 * 4.0));
    }
}

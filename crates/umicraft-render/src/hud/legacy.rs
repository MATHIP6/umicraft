// SPDX-License-Identifier: MIT OR Apache-2.0

//! The legacy [`Overlay`]-based HUD renderer: flat rectangles and the built-in
//! 3x5 font.
//!
//! It is still used by the demos and the fallback paths that have no GUI
//! sprite sheet. The sprite-based renderer in `hud::gui` replaces it, and this
//! module will be deleted in P9-h.

use super::layout::{
    clamped, selected_index, status_fills, Fill, HudLayout, Rect, StatusRow, DEBUG_LEFT,
    DEBUG_PADDING_X, DEBUG_PADDING_Y, DEBUG_TOP,
};
use super::state::{HudState, HOTBAR_SLOTS};
use super::{chat_hud, social_hud};
use crate::font;
use crate::overlay::Overlay;
use std::borrow::Cow;

/// Gap kept between the debug panel and the bottom UI stack, in pixels.
const DEBUG_GAP: f32 = 4.0;
/// Background of the hotbar.
const HOTBAR_BACKGROUND: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
/// Slot separators inside the hotbar.
const SLOT_SEPARATOR: [f32; 4] = [1.0, 1.0, 1.0, 0.08];
/// Selection box around the selected hotbar slot.
const SELECTION: [f32; 4] = [1.0, 1.0, 1.0, 0.9];
/// Full health icon.
const HEALTH_FULL: [f32; 4] = [0.85, 0.15, 0.15, 1.0];
/// Empty health icon.
const HEALTH_EMPTY: [f32; 4] = [0.18, 0.04, 0.04, 0.9];
/// Full food icon.
const FOOD_FULL: [f32; 4] = [0.95, 0.55, 0.15, 1.0];
/// Empty food icon.
const FOOD_EMPTY: [f32; 4] = [0.25, 0.15, 0.05, 0.9];
/// Full armour icon.
const ARMOR_FULL: [f32; 4] = [0.35, 0.8, 0.9, 1.0];
/// Empty armour icon.
const ARMOR_EMPTY: [f32; 4] = [0.05, 0.15, 0.2, 0.9];
/// Background of the experience bar.
const XP_BACKGROUND: [f32; 4] = [0.05, 0.05, 0.05, 0.8];
/// Fill of the experience bar.
const XP_FILL: [f32; 4] = [0.25, 0.85, 0.3, 1.0];
/// Experience level label.
const LEVEL_TEXT: [f32; 4] = [1.0, 0.9, 0.3, 1.0];
/// Shadow behind the experience level label.
const LEVEL_SHADOW: [f32; 4] = [0.0, 0.0, 0.0, 0.8];
/// The crosshair.
const CROSSHAIR: [f32; 4] = [1.0, 1.0, 1.0, 0.85];
/// Background of the debug panel.
const DEBUG_BACKGROUND: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
/// Debug text.
const DEBUG_TEXT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// Draws the whole HUD into `overlay`.
///
/// `width` and `height` are the viewport size in physical pixels and must
/// match the size the `overlay` was created with, because the overlay turns
/// the pixel coordinates produced here into clip space.
///
/// The function is total: an out-of-range `selected_slot` is clamped and
/// non-finite values are treated as zero, so a malformed state can never
/// panic the render thread.
pub fn draw(overlay: &mut Overlay, state: &HudState, width: u32, height: u32) {
    let layout = HudLayout::new(width, height);
    draw_hotbar(overlay, state, &layout);
    draw_status_rows(overlay, state, &layout);
    draw_experience(overlay, state, &layout);
    chat_hud::draw(overlay, state, &layout);
    draw_crosshair(overlay, state, &layout);
    draw_debug(overlay, state, &layout);
    social_hud::draw(overlay, state, &layout);
}

fn draw_hotbar(overlay: &mut Overlay, state: &HudState, layout: &HudLayout) {
    let hotbar = layout.hotbar_rect();
    overlay.rect(
        hotbar.x,
        hotbar.y,
        hotbar.width,
        hotbar.height,
        HOTBAR_BACKGROUND,
    );
    let separator = layout.scale().max(1.0);
    for index in 1..HOTBAR_SLOTS {
        let boundary = layout.slot_rect(index).x;
        overlay.rect(boundary, hotbar.y, separator, hotbar.height, SLOT_SEPARATOR);
    }
    let selected = layout.slot_rect(selected_index(state.selected_slot));
    draw_border(overlay, selected, separator, SELECTION);
}

fn draw_status_rows(overlay: &mut Overlay, state: &HudState, layout: &HudLayout) {
    draw_status_row(
        overlay,
        layout,
        StatusRow::Health,
        state.health,
        HEALTH_FULL,
        HEALTH_EMPTY,
    );
    draw_status_row(
        overlay,
        layout,
        StatusRow::Food,
        state.food,
        FOOD_FULL,
        FOOD_EMPTY,
    );
    if state.armor > 0.0 {
        draw_status_row(
            overlay,
            layout,
            StatusRow::Armor,
            state.armor,
            ARMOR_FULL,
            ARMOR_EMPTY,
        );
    }
}

fn draw_status_row(
    overlay: &mut Overlay,
    layout: &HudLayout,
    row: StatusRow,
    value: f32,
    full: [f32; 4],
    empty: [f32; 4],
) {
    for (index, fill) in status_fills(value).into_iter().enumerate() {
        let icon = layout.status_icon_rect(row, index);
        match fill {
            Fill::Empty => overlay.rect(icon.x, icon.y, icon.width, icon.height, empty),
            Fill::Half => {
                let half = icon.width * 0.5;
                overlay.rect(icon.x + half, icon.y, half, icon.height, empty);
                overlay.rect(icon.x, icon.y, half, icon.height, full);
            }
            Fill::Full => overlay.rect(icon.x, icon.y, icon.width, icon.height, full),
        }
    }
}

fn draw_experience(overlay: &mut Overlay, state: &HudState, layout: &HudLayout) {
    let bar = layout.xp_bar_rect();
    overlay.rect(bar.x, bar.y, bar.width, bar.height, XP_BACKGROUND);
    let fraction = clamped(state.experience, 1.0);
    overlay.rect(bar.x, bar.y, bar.width * fraction, bar.height, XP_FILL);
    if state.level <= 0 {
        return;
    }
    let scale = layout.scale();
    let level = state.level.to_string();
    let level_width = Overlay::text_width(&level, scale);
    let x = bar.x + (bar.width - level_width) * 0.5;
    let y = bar.y + (bar.height - 5.0 * scale) * 0.5;
    overlay.text(x + scale, y + scale, scale, LEVEL_SHADOW, &level);
    overlay.text(x, y, scale, LEVEL_TEXT, &level);
}
fn draw_crosshair(overlay: &mut Overlay, state: &HudState, layout: &HudLayout) {
    if !state.crosshair {
        return;
    }
    let (horizontal, vertical) = layout.crosshair_rects();
    overlay.rect(
        horizontal.x,
        horizontal.y,
        horizontal.width,
        horizontal.height,
        CROSSHAIR,
    );
    overlay.rect(
        vertical.x,
        vertical.y,
        vertical.width,
        vertical.height,
        CROSSHAIR,
    );
}

/// Draws only the debug block, for the textured HUD that draws everything
/// else itself (see `hud_gui`).
pub fn draw_debug_only(overlay: &mut Overlay, state: &HudState, width: u32, height: u32) {
    let layout = HudLayout::new(width, height);
    draw_debug(overlay, state, &layout);
}

fn draw_debug(overlay: &mut Overlay, state: &HudState, layout: &HudLayout) {
    if state.debug_lines.is_empty() {
        return;
    }
    let content_top = layout.content_top(state.armor > 0.0);
    let line_height = layout.line_height();
    let scale = layout.scale();
    // The block must stay clear of the bottom UI stack.
    let available = content_top - DEBUG_TOP - DEBUG_GAP - DEBUG_PADDING_Y;
    let max_lines = (available / line_height).floor().max(0.0) as usize;
    let visible = state.debug_lines.len().min(max_lines);
    if visible == 0 {
        return;
    }
    let advance = font::CELL_WIDTH as f32 * scale;
    let max_chars = max_chars(layout.width() - DEBUG_LEFT - DEBUG_PADDING_X, advance);
    let mut text_width = 0.0_f32;
    let mut lines = Vec::with_capacity(visible);
    for line in &state.debug_lines[..visible] {
        let text = fit(line, max_chars);
        text_width = text_width.max(Overlay::text_width(&text, scale));
        lines.push(text);
    }
    let panel = layout.debug_panel_rect(text_width, visible);
    overlay.rect(
        panel.x,
        panel.y,
        panel.width,
        panel.height,
        DEBUG_BACKGROUND,
    );
    for (index, line) in lines.iter().enumerate() {
        let (x, y) = layout.debug_text_origin(index);
        overlay.text(x, y, scale, DEBUG_TEXT, line);
    }
}

fn draw_border(overlay: &mut Overlay, rect: Rect, thickness: f32, color: [f32; 4]) {
    overlay.rect(rect.x, rect.y, rect.width, thickness, color);
    overlay.rect(
        rect.x,
        rect.bottom() - thickness,
        rect.width,
        thickness,
        color,
    );
    overlay.rect(
        rect.x,
        rect.y + thickness,
        thickness,
        rect.height - 2.0 * thickness,
        color,
    );
    overlay.rect(
        rect.right() - thickness,
        rect.y + thickness,
        thickness,
        rect.height - 2.0 * thickness,
        color,
    );
}

/// The number of characters that fit into `width` pixels.
fn max_chars(width: f32, advance: f32) -> usize {
    if advance <= 0.0 {
        return 0;
    }
    (width / advance).floor().max(0.0) as usize
}

/// Truncates `text` to `max_chars` characters and drops the line breaks the
/// overlay would otherwise turn into a new row.
fn fit(text: &str, max_chars: usize) -> Cow<'_, str> {
    if !text.contains('\n') && text.chars().count() <= max_chars {
        return Cow::Borrowed(text);
    }
    Cow::Owned(
        text.chars()
            .filter(|character| *character != '\n')
            .take(max_chars)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::hud::{ChatLineView, ChatRunView};

    fn test_line(text: &str) -> ChatLineView {
        ChatLineView {
            runs: vec![ChatRunView {
                text: text.to_string(),
                color: [255, 255, 255],
                bold: false,
                italic: false,
                underlined: false,
                strikethrough: false,
                obfuscated: false,
                ..ChatRunView::default()
            }],
            received: Instant::now(),
        }
    }

    /// The screen-space bounds of everything the overlay holds.
    fn screen_bounds(overlay: &Overlay, width: u32, height: u32) -> (f32, f32, f32, f32) {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for vertex in overlay.vertices() {
            let x = (vertex.position[0] + 1.0) * 0.5 * width as f32;
            let y = (1.0 - vertex.position[1]) * 0.5 * height as f32;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        (min_x, min_y, max_x, max_y)
    }

    #[test]
    fn chat_lines_stay_inside_the_viewport() {
        let (width, height) = (320_u32, 240_u32);
        let state = HudState {
            chat: (0..50)
                .map(|index| test_line(&format!("line {index}")))
                .collect(),
            ..HudState::default()
        };
        let mut overlay = Overlay::new(width, height);
        draw(&mut overlay, &state, width, height);
        let (min_x, min_y, max_x, max_y) = screen_bounds(&overlay, width, height);
        assert!(min_x >= -0.5, "chat left {min_x} left the viewport");
        assert!(min_y >= -0.5, "chat top {min_y} left the viewport");
        assert!(
            max_x <= width as f32 + 0.5,
            "chat right {max_x} left the viewport"
        );
        assert!(
            max_y <= height as f32 + 0.5,
            "chat bottom {max_y} left the viewport"
        );
    }

    #[test]
    fn debug_block_does_not_overlap_the_hotbar() {
        let (width, height) = (1280_u32, 720_u32);
        let state = HudState {
            debug_lines: (0..20).map(|index| format!("debug {index}")).collect(),
            ..HudState::default()
        };
        let mut overlay = Overlay::new(width, height);
        draw(&mut overlay, &state, width, height);
        let layout = HudLayout::new(width, height);
        let panel = layout.debug_panel_rect(
            state
                .debug_lines
                .iter()
                .map(|line| Overlay::text_width(line, layout.scale()))
                .fold(0.0_f32, f32::max),
            state.debug_lines.len(),
        );
        assert!(
            panel.bottom() <= layout.hotbar_rect().y,
            "the debug panel reaches the hotbar"
        );
        let (_, _, _, max_y) = screen_bounds(&overlay, width, height);
        assert!(max_y <= height as f32 + 0.5);
    }

    #[test]
    fn empty_chat_and_debug_draw_nothing() {
        let (width, height) = (640_u32, 360_u32);
        let state = HudState::default();
        let mut bare = Overlay::new(width, height);
        draw(&mut bare, &state, width, height);

        let state = HudState {
            chat: vec![test_line("hello")],
            debug_lines: vec!["world".to_string()],
            ..HudState::default()
        };
        let mut full = Overlay::new(width, height);
        draw(&mut full, &state, width, height);
        assert!(
            full.vertex_count() > bare.vertex_count(),
            "the chat and the debug block must add geometry"
        );
    }

    #[test]
    fn drawing_an_out_of_range_selection_does_not_panic() {
        let (width, height) = (640_u32, 360_u32);
        let state = HudState {
            selected_slot: 200,
            ..HudState::default()
        };
        let mut overlay = Overlay::new(width, height);
        draw(&mut overlay, &state, width, height);
        assert!(!overlay.is_empty());
    }

    #[test]
    fn the_selection_is_drawn_around_the_clamped_slot() {
        let (width, height) = (640_u32, 360_u32);
        let state = HudState {
            selected_slot: u8::MAX,
            ..HudState::default()
        };
        let mut overlay = Overlay::new(width, height);
        draw(&mut overlay, &state, width, height);
        let layout = HudLayout::new(width, height);
        let slot = layout.slot_rect(selected_index(state.selected_slot));
        let selection: Vec<_> = overlay
            .vertices()
            .iter()
            .filter(|vertex| vertex.color == SELECTION)
            .collect();
        assert!(!selection.is_empty(), "the selection box was not drawn");
        for vertex in selection {
            let x = (vertex.position[0] + 1.0) * 0.5 * width as f32;
            let y = (1.0 - vertex.position[1]) * 0.5 * height as f32;
            assert!(
                x >= slot.x - 0.5 && x <= slot.right() + 0.5,
                "the selection box left the slot"
            );
            assert!(
                y >= slot.y - 0.5 && y <= slot.bottom() + 0.5,
                "the selection box left the slot"
            );
        }
    }

    #[test]
    fn the_experience_bar_never_leaves_the_viewport() {
        let (width, height) = (200_u32, 120_u32);
        let state = HudState {
            experience: f32::NAN,
            health: f32::NAN,
            ..HudState::default()
        };
        let mut overlay = Overlay::new(width, height);
        draw(&mut overlay, &state, width, height);
        let (min_x, min_y, max_x, max_y) = screen_bounds(&overlay, width, height);
        assert!(min_x >= -0.5 && min_y >= -0.5);
        assert!(max_x <= width as f32 + 0.5 && max_y <= height as f32 + 0.5);
    }
}

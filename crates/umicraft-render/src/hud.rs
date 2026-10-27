// SPDX-License-Identifier: MIT OR Apache-2.0

//! The in-game HUD of spec §6.6 (UI-2): hotbar, health/food/armour rows,
//! experience bar, crosshair, chat, and the debug block.
//!
//! The HUD is pure geometry. Every icon is a rectangle and every label is a
//! glyph of the built-in 3×5 font, so the HUD adds no texture, no font and
//! no pipeline, and the crate keeps shipping no Mojang asset (spec §3).
//!
//! The layout is derived from the viewport size in *physical* pixels with the
//! same integer scale convention as the client debug panel —
//! `scale = (height / 360).floor().clamp(1, 3)` — so the HUD keeps the same
//! apparent size on small and on large displays.
//!
//! # Wiring
//!
//! The viewport size must be the one the [`Overlay`] was built with, because
//! the overlay turns the pixel coordinates produced here into clip space:
//!
//! ```
//! use umicraft_render::{draw_hud, HudState, Overlay};
//!
//! let mut overlay = Overlay::new(640, 360);
//! let state = HudState::default();
//! draw_hud(&mut overlay, &state, 640, 360);
//! assert!(!overlay.is_empty());
//! ```

use std::borrow::Cow;

use crate::font;
use crate::overlay::Overlay;

/// Number of hotbar slots drawn by the HUD.
pub const HOTBAR_SLOTS: usize = 9;

/// Number of icons in each status row (hearts, food, armour).
pub const STATUS_ICONS: usize = 10;

/// Maximum number of chat lines the HUD keeps on screen.
pub const CHAT_LINES: usize = 10;

/// Viewport height that maps to a scale of 1.
const REFERENCE_HEIGHT: f32 = 360.0;
/// Smallest HUD scale.
const MIN_SCALE: f32 = 1.0;
/// Largest HUD scale.
const MAX_SCALE: f32 = 3.0;

/// Edge of one hotbar slot, in unscaled pixels.
const SLOT_SIZE: f32 = 20.0;
/// Height of a status icon, in unscaled pixels.
const ICON_HEIGHT: f32 = 9.0;
/// Height of the experience bar, in unscaled pixels.
const XP_HEIGHT: f32 = 6.0;
/// Vertical gap between two stacked bottom elements, in unscaled pixels.
const STACK_GAP: f32 = 1.0;
/// Distance between the hotbar and the bottom of the viewport, unscaled.
const BOTTOM_MARGIN: f32 = 4.0;
/// Height of one chat or debug line, in unscaled pixels.
const LINE_HEIGHT: f32 = 8.0;
/// Left edge of the chat block, in pixels.
const CHAT_LEFT: f32 = 4.0;
/// Indent of the chat text inside its background, in pixels.
const CHAT_TEXT_INDENT: f32 = 2.0;
/// Left edge of the debug panel, in pixels.
const DEBUG_LEFT: f32 = 4.0;
/// Top edge of the debug panel, in pixels.
const DEBUG_TOP: f32 = 4.0;
/// Top-left corner of the debug text, in pixels.
const DEBUG_TEXT_X: f32 = 10.0;
/// Top-left corner of the debug text, in pixels.
const DEBUG_TEXT_Y: f32 = 10.0;
/// Horizontal padding of the debug panel, in pixels.
const DEBUG_PADDING_X: f32 = 20.0;
/// Vertical padding of the debug panel, in pixels.
const DEBUG_PADDING_Y: f32 = 12.0;
/// Gap kept between the debug panel and the bottom UI stack, in pixels.
const DEBUG_GAP: f32 = 4.0;
/// Alpha of the oldest chat line; the newest ones fade to opaque.
const CHAT_MIN_ALPHA: f32 = 0.35;
/// Reference maximum damage used to scale the durability bar.
///
/// The frozen [`ItemStack`] carries no maximum-durability field, so the bar
/// is scaled against this reference instead of a per-item maximum.
const DURABILITY_REFERENCE: f32 = 256.0;

/// Background of the hotbar.
const HOTBAR_BACKGROUND: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
/// Slot separators inside the hotbar.
const SLOT_SEPARATOR: [f32; 4] = [1.0, 1.0, 1.0, 0.08];
/// Selection box around the selected hotbar slot.
const SELECTION: [f32; 4] = [1.0, 1.0, 1.0, 0.9];
/// Item id label.
const ITEM_TEXT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
/// Stack count label.
const COUNT_TEXT: [f32; 4] = [1.0, 1.0, 0.9, 1.0];
/// Background of the durability bar.
const DURABILITY_BACKGROUND: [f32; 4] = [0.0, 0.0, 0.0, 0.8];
/// Fill of the durability bar.
const DURABILITY_FILL: [f32; 4] = [0.9, 0.2, 0.2, 1.0];
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
/// Background of one chat line.
const CHAT_BACKGROUND: [f32; 4] = [0.0, 0.0, 0.0, 0.5];
/// Chat text.
const CHAT_TEXT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
/// Background of the debug panel.
const DEBUG_BACKGROUND: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
/// Debug text.
const DEBUG_TEXT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// Everything the HUD draws, as plain data.
///
/// The client fills this from the game state every frame and hands it to
/// [`draw`]; the HUD itself keeps no state and owns no resources.
#[derive(Debug, Clone, PartialEq)]
pub struct HudState {
    /// Health points, `0..=20`; twenty points fill ten hearts.
    pub health: f32,
    /// Food points, `0..=20`; twenty points fill ten drumsticks.
    pub food: f32,
    /// Armour points, `0..=20`; zero hides the armour row.
    pub armor: f32,
    /// Progress towards the next level, `0..=1`.
    pub experience: f32,
    /// The current experience level, shown on the experience bar.
    pub level: i32,
    /// The nine hotbar slots; an empty slot is `None`.
    pub hotbar: [Option<ItemStack>; HOTBAR_SLOTS],
    /// The selected hotbar slot; values above eight are clamped.
    pub selected_slot: u8,
    /// Whether the crosshair is drawn.
    pub crosshair: bool,
    /// Chat lines, oldest first; only the last ten are drawn.
    pub chat_lines: Vec<String>,
    /// Debug lines, top to bottom.
    pub debug_lines: Vec<String>,
    /// The score shown on the death screen (MCP `EntityPlayer.getScore`).
    pub score: i32,
}

impl Default for HudState {
    /// A player at full health and food, with an empty hotbar.
    fn default() -> Self {
        Self {
            health: 20.0,
            food: 20.0,
            armor: 0.0,
            experience: 0.0,
            level: 0,
            hotbar: [None; HOTBAR_SLOTS],
            selected_slot: 0,
            crosshair: true,
            chat_lines: Vec::new(),
            debug_lines: Vec::new(),
            score: 0,
        }
    }
}

/// A hotbar slot: item id, damage, and stack count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemStack {
    /// The 1.8 item id.
    pub item_id: i32,
    /// The item damage (metadata) value; zero means undamaged.
    pub damage: i16,
    /// The number of items in the stack.
    pub count: u8,
}

/// A screen-space rectangle in physical pixels, with a top-left origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left edge, in pixels from the left of the viewport.
    pub x: f32,
    /// Top edge, in pixels from the top of the viewport.
    pub y: f32,
    /// Width, in pixels.
    pub width: f32,
    /// Height, in pixels.
    pub height: f32,
}

impl Rect {
    /// Creates a rectangle from its top-left corner and its size.
    #[must_use]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The right edge, in pixels.
    #[must_use]
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    /// The bottom edge, in pixels.
    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    /// True when `other` lies completely inside this rectangle.
    #[must_use]
    pub fn contains(&self, other: &Rect) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.right() <= self.right()
            && other.bottom() <= self.bottom()
    }
}

/// The three status rows of the HUD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusRow {
    /// Hearts, the row just above the experience bar.
    Health,
    /// Food, the row above the health row.
    Food,
    /// Armour, the row above the food row; only drawn when the value is
    /// non-zero.
    Armor,
}

/// How much of a status icon is filled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    /// The icon is empty.
    Empty,
    /// The left half of the icon is filled.
    Half,
    /// The icon is completely filled.
    Full,
}

/// The HUD geometry for one viewport size.
///
/// The layout is a pure function of the viewport size, so it can be computed
/// and tested without a GPU. All rectangles are in physical pixels with a
/// top-left origin, which is the coordinate system [`Overlay`] expects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HudLayout {
    width: f32,
    height: f32,
    scale: f32,
}

impl HudLayout {
    /// Computes the layout for a `width`×`height` viewport in physical pixels.
    ///
    /// The scale follows the client debug panel convention,
    /// `(height / 360).floor().clamp(1, 3)`.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let width = width.max(1) as f32;
        let height = height.max(1) as f32;
        let scale = (height / REFERENCE_HEIGHT)
            .floor()
            .clamp(MIN_SCALE, MAX_SCALE);
        Self {
            width,
            height,
            scale,
        }
    }

    /// The viewport width, in pixels.
    #[must_use]
    pub fn width(&self) -> f32 {
        self.width
    }

    /// The viewport height, in pixels.
    #[must_use]
    pub fn height(&self) -> f32 {
        self.height
    }

    /// The integer UI scale derived from the viewport height.
    #[must_use]
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// The height of one chat or debug line, in pixels.
    #[must_use]
    pub fn line_height(&self) -> f32 {
        LINE_HEIGHT * self.scale
    }

    /// The whole hotbar, centred at the bottom of the viewport.
    #[must_use]
    pub fn hotbar_rect(&self) -> Rect {
        let size = SLOT_SIZE * self.scale;
        let width = HOTBAR_SLOTS as f32 * size;
        Rect {
            x: (self.width - width) * 0.5,
            y: self.height - size - BOTTOM_MARGIN * self.scale,
            width,
            height: size,
        }
    }

    /// The rectangle of hotbar slot `index`; out-of-range indices are clamped.
    #[must_use]
    pub fn slot_rect(&self, index: usize) -> Rect {
        let hotbar = self.hotbar_rect();
        let size = SLOT_SIZE * self.scale;
        let index = index.min(HOTBAR_SLOTS - 1);
        Rect {
            x: hotbar.x + index as f32 * size,
            y: hotbar.y,
            width: size,
            height: size,
        }
    }

    /// The experience bar, directly above the hotbar.
    #[must_use]
    pub fn xp_bar_rect(&self) -> Rect {
        let hotbar = self.hotbar_rect();
        Rect {
            x: hotbar.x,
            y: hotbar.y - STACK_GAP * self.scale - XP_HEIGHT * self.scale,
            width: hotbar.width,
            height: XP_HEIGHT * self.scale,
        }
    }

    /// The whole status row; its width is the hotbar width, so the ten icons
    /// exactly fill the hotbar.
    #[must_use]
    pub fn status_row_rect(&self, row: StatusRow) -> Rect {
        let hotbar = self.hotbar_rect();
        let height = ICON_HEIGHT * self.scale;
        let gap = STACK_GAP * self.scale;
        let health_top = self.xp_bar_rect().y - gap - height;
        let food_top = health_top - gap - height;
        let armor_top = food_top - gap - height;
        let y = match row {
            StatusRow::Health => health_top,
            StatusRow::Food => food_top,
            StatusRow::Armor => armor_top,
        };
        Rect {
            x: hotbar.x,
            y,
            width: hotbar.width,
            height,
        }
    }

    /// The rectangle of one icon of a status row; out-of-range indices are
    /// clamped.
    #[must_use]
    pub fn status_icon_rect(&self, row: StatusRow, index: usize) -> Rect {
        let row_rect = self.status_row_rect(row);
        let index = index.min(STATUS_ICONS - 1);
        let width = row_rect.width / STATUS_ICONS as f32;
        Rect {
            x: row_rect.x + index as f32 * width,
            y: row_rect.y,
            width,
            height: row_rect.height,
        }
    }

    /// The top edge of the bottom UI stack (hotbar, experience bar, status
    /// rows).
    ///
    /// `armor_visible` tells whether the armour row is drawn; the caller
    /// passes the state so the chat and the debug block stay clear of the
    /// elements that are actually on screen.
    #[must_use]
    pub fn content_top(&self, armor_visible: bool) -> f32 {
        let mut top = self.xp_bar_rect().y;
        top = top.min(self.status_row_rect(StatusRow::Health).y);
        top = top.min(self.status_row_rect(StatusRow::Food).y);
        if armor_visible {
            top = top.min(self.status_row_rect(StatusRow::Armor).y);
        }
        top
    }

    /// The band occupied by chat line `index`.
    ///
    /// `bottom` is the top edge of the bottom UI stack and `visible` the
    /// number of chat lines drawn; index zero is the oldest line. The width
    /// is the space available for the line, so a caller can clamp its text.
    #[must_use]
    pub fn chat_line_rect(&self, bottom: f32, index: usize, visible: usize) -> Rect {
        let visible = visible.max(1);
        let index = index.min(visible - 1);
        let line_height = self.line_height();
        Rect {
            x: CHAT_LEFT,
            y: bottom - (visible - index) as f32 * line_height,
            width: (self.width - CHAT_LEFT).max(0.0),
            height: line_height,
        }
    }

    /// The background of the debug panel for `lines` lines of `text_width`
    /// pixels.
    #[must_use]
    pub fn debug_panel_rect(&self, text_width: f32, lines: usize) -> Rect {
        Rect {
            x: DEBUG_LEFT,
            y: DEBUG_TOP,
            width: text_width + DEBUG_PADDING_X,
            height: lines as f32 * self.line_height() + DEBUG_PADDING_Y,
        }
    }

    /// The top-left corner of debug line `index`.
    #[must_use]
    pub fn debug_text_origin(&self, index: usize) -> (f32, f32) {
        (
            DEBUG_TEXT_X,
            DEBUG_TEXT_Y + index as f32 * self.line_height(),
        )
    }

    /// The two rectangles of the crosshair: the horizontal and the vertical
    /// arm, centred on the viewport.
    #[must_use]
    pub fn crosshair_rects(&self) -> (Rect, Rect) {
        let arm = 5.0 * self.scale;
        let thickness = self.scale.max(1.0);
        let center_x = self.width * 0.5;
        let center_y = self.height * 0.5;
        (
            Rect {
                x: center_x - arm,
                y: center_y - thickness * 0.5,
                width: arm * 2.0,
                height: thickness,
            },
            Rect {
                x: center_x - thickness * 0.5,
                y: center_y - arm,
                width: thickness,
                height: arm * 2.0,
            },
        )
    }
}

/// The hotbar slot to highlight, clamped to the nine existing slots.
///
/// A `selected_slot` above eight (a malformed packet, or a key press that
/// raced the inventory update) selects the last slot instead of panicking.
#[must_use]
pub fn selected_index(selected_slot: u8) -> usize {
    (selected_slot as usize).min(HOTBAR_SLOTS - 1)
}

/// The fill state of every icon of a status row.
///
/// The value is measured in hit points and two points fill one icon, so
/// nineteen points give nine full icons and one half icon. Values outside
/// `0..=20` and non-finite values are clamped away.
#[must_use]
pub fn status_fills(value: f32) -> [Fill; STATUS_ICONS] {
    let points = clamped(value, (STATUS_ICONS * 2) as f32).round() as usize;
    let mut fills = [Fill::Empty; STATUS_ICONS];
    for (index, fill) in fills.iter_mut().enumerate() {
        let first = index * 2;
        *fill = if points >= first + 2 {
            Fill::Full
        } else if points > first {
            Fill::Half
        } else {
            Fill::Empty
        };
    }
    fills
}

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
    draw_chat(overlay, state, &layout);
    draw_crosshair(overlay, state, &layout);
    draw_debug(overlay, state, &layout);
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
    for (index, stack) in state.hotbar.iter().enumerate() {
        if let Some(stack) = stack {
            draw_item(overlay, layout, index, stack);
        }
    }
}

fn draw_item(overlay: &mut Overlay, layout: &HudLayout, index: usize, stack: &ItemStack) {
    let slot = layout.slot_rect(index);
    let scale = layout.scale();
    let advance = font::CELL_WIDTH as f32 * scale;

    let id = stack.item_id.to_string();
    let id = fit(&id, max_chars(slot.width - 2.0 * scale, advance));
    let id_width = Overlay::text_width(&id, scale);
    overlay.text(
        slot.x + (slot.width - id_width) * 0.5,
        slot.y + (slot.height - 5.0 * scale) * 0.5,
        scale,
        ITEM_TEXT,
        &id,
    );

    if stack.count > 1 {
        let count = stack.count.to_string();
        let count_width = Overlay::text_width(&count, scale);
        overlay.text(
            slot.x + slot.width - count_width - scale,
            slot.bottom() - 7.0 * scale,
            scale,
            COUNT_TEXT,
            &count,
        );
    }

    if stack.damage > 0 {
        let height = 2.0 * scale;
        let y = slot.bottom() - height;
        let x = slot.x + scale;
        let width = (slot.width - 2.0 * scale).max(0.0);
        overlay.rect(x, y, width, height, DURABILITY_BACKGROUND);
        let fraction = (f32::from(stack.damage) / DURABILITY_REFERENCE).clamp(0.0, 1.0);
        overlay.rect(x, y, width * fraction, height, DURABILITY_FILL);
    }
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

fn draw_chat(overlay: &mut Overlay, state: &HudState, layout: &HudLayout) {
    if state.chat_lines.is_empty() {
        return;
    }
    let bottom = layout.content_top(state.armor > 0.0);
    let line_height = layout.line_height();
    let scale = layout.scale();
    // Only the lines that fit on screen are drawn, and the oldest ones are
    // dropped first, so the chat never leaves the viewport.
    let max_lines = (bottom / line_height).floor().max(0.0) as usize;
    let visible = state.chat_lines.len().min(CHAT_LINES).min(max_lines);
    if visible == 0 {
        return;
    }
    let start = state.chat_lines.len() - visible;
    let advance = font::CELL_WIDTH as f32 * scale;
    let max_chars = max_chars(layout.width() - CHAT_LEFT - 2.0 * CHAT_TEXT_INDENT, advance);
    for (index, line) in state.chat_lines[start..].iter().enumerate() {
        let band = layout.chat_line_rect(bottom, index, visible);
        let text = fit(line, max_chars);
        let text_width = Overlay::text_width(&text, scale);
        let alpha = chat_alpha(index, visible);
        overlay.rect(
            band.x,
            band.y,
            text_width + 2.0 * CHAT_TEXT_INDENT,
            band.height,
            [0.0, 0.0, 0.0, CHAT_BACKGROUND[3] * alpha],
        );
        overlay.text(
            band.x + CHAT_TEXT_INDENT,
            band.y + (band.height - 5.0 * scale) * 0.5,
            scale,
            [CHAT_TEXT[0], CHAT_TEXT[1], CHAT_TEXT[2], alpha],
            &text,
        );
    }
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

/// The alpha of a chat line; the oldest line fades out towards [`CHAT_MIN_ALPHA`].
fn chat_alpha(index: usize, visible: usize) -> f32 {
    if visible <= 1 {
        return 1.0;
    }
    let step = index as f32 / (visible - 1) as f32;
    CHAT_MIN_ALPHA + (1.0 - CHAT_MIN_ALPHA) * step
}

/// The number of characters that fit into `width` pixels.
fn max_chars(width: f32, advance: f32) -> usize {
    if advance <= 0.0 {
        return 0;
    }
    (width / advance).floor().max(0.0) as usize
}

/// Clamps `value` into `0.0..=max`, mapping non-finite input to zero.
fn clamped(value: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, max)
    } else {
        0.0
    }
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
    use super::*;

    fn layout() -> HudLayout {
        HudLayout::new(1280, 720)
    }

    /// A hotbar with a single stack in it.
    fn hotbar_with(index: usize, stack: ItemStack) -> [Option<ItemStack>; HOTBAR_SLOTS] {
        let mut hotbar = [None; HOTBAR_SLOTS];
        hotbar[index] = Some(stack);
        hotbar
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
    fn scale_follows_the_client_convention() {
        assert!((HudLayout::new(640, 360).scale() - 1.0).abs() < f32::EPSILON);
        assert!((HudLayout::new(1280, 720).scale() - 2.0).abs() < f32::EPSILON);
        assert!((HudLayout::new(1920, 1080).scale() - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn scale_stays_within_bounds_for_extreme_viewports() {
        for height in [1_u32, 2, 16, 100, 359, 360, 719, 720, 1080, 2160, 4320] {
            let scale = HudLayout::new(800, height).scale();
            assert!(
                (1.0..=3.0).contains(&scale),
                "height {height} produced scale {scale}"
            );
        }
    }

    #[test]
    fn health_icons_exactly_fill_the_hotbar_width() {
        let layout = layout();
        let hotbar = layout.hotbar_rect();
        let row = layout.status_row_rect(StatusRow::Health);
        assert!((row.width - hotbar.width).abs() < f32::EPSILON);
        assert!((row.x - hotbar.x).abs() < f32::EPSILON);
        let first = layout.status_icon_rect(StatusRow::Health, 0);
        let last = layout.status_icon_rect(StatusRow::Health, STATUS_ICONS - 1);
        assert!((first.x - hotbar.x).abs() < f32::EPSILON);
        assert!((last.x + last.width - hotbar.right()).abs() < f32::EPSILON);
    }

    #[test]
    fn food_row_is_aligned_with_the_health_row() {
        let layout = layout();
        let health = layout.status_row_rect(StatusRow::Health);
        let food = layout.status_row_rect(StatusRow::Food);
        assert!((health.x - food.x).abs() < f32::EPSILON);
        assert!((health.width - food.width).abs() < f32::EPSILON);
        assert!(food.y < health.y, "the food row sits above the health row");
    }

    #[test]
    fn every_slot_is_inside_the_hotbar() {
        let layout = layout();
        let hotbar = layout.hotbar_rect();
        for index in 0..HOTBAR_SLOTS {
            let slot = layout.slot_rect(index);
            assert!(hotbar.contains(&slot), "slot {index} escaped the hotbar");
        }
    }

    #[test]
    fn out_of_range_selected_slot_is_clamped() {
        assert_eq!(selected_index(0), 0);
        assert_eq!(selected_index(8), 8);
        assert_eq!(selected_index(9), 8);
        assert_eq!(selected_index(u8::MAX), HOTBAR_SLOTS - 1);
    }

    #[test]
    fn half_health_gives_nine_full_icons_and_one_half() {
        let fills = status_fills(19.0);
        assert!(fills[..9].iter().all(|fill| *fill == Fill::Full));
        assert_eq!(fills[9], Fill::Half);
        assert!(status_fills(20.0).iter().all(|fill| *fill == Fill::Full));
        assert!(status_fills(0.0).iter().all(|fill| *fill == Fill::Empty));
        assert!(status_fills(-5.0).iter().all(|fill| *fill == Fill::Empty));
        assert!(status_fills(100.0).iter().all(|fill| *fill == Fill::Full));
        assert!(status_fills(f32::NAN)
            .iter()
            .all(|fill| *fill == Fill::Empty));
    }

    #[test]
    fn chat_lines_stay_inside_the_viewport() {
        let (width, height) = (320_u32, 240_u32);
        let state = HudState {
            chat_lines: (0..50).map(|index| format!("line {index}")).collect(),
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
            chat_lines: vec!["hello".to_string()],
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
            hotbar: hotbar_with(
                0,
                ItemStack {
                    item_id: 264,
                    damage: 12,
                    count: 64,
                },
            ),
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
    fn a_damaged_item_draws_a_durability_bar() {
        let (width, height) = (640_u32, 360_u32);
        let undamaged = HudState {
            hotbar: hotbar_with(
                0,
                ItemStack {
                    item_id: 256,
                    damage: 0,
                    count: 1,
                },
            ),
            ..HudState::default()
        };
        let mut overlay = Overlay::new(width, height);
        draw(&mut overlay, &undamaged, width, height);
        let undamaged_vertices = overlay.vertex_count();

        let damaged = HudState {
            hotbar: hotbar_with(
                0,
                ItemStack {
                    item_id: 256,
                    damage: 100,
                    count: 1,
                },
            ),
            ..HudState::default()
        };
        let mut overlay = Overlay::new(width, height);
        draw(&mut overlay, &damaged, width, height);
        assert!(overlay.vertex_count() > undamaged_vertices);
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

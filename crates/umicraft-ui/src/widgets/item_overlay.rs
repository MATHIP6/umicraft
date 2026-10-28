// SPDX-License-Identifier: MIT OR Apache-2.0

//! The item icon plus its stack count and durability bar.
//!
//! [`draw_item`] is shared by the inventory slots and the in-game hotbar, so
//! both look like the vanilla `RenderItem.renderItemOverlayIntoGUI` (item
//! icon, count at the bottom-right with a drop shadow, durability bar).
//! Geometry is expressed in GUI pixels (a 16x16 item area) and scaled to the
//! `cell` the caller passes in.

use crate::draw::{Color, DrawList, TextStyle};
use crate::geom::Rect;
use crate::text::FontAtlas;

use super::slot::SlotItem;

/// Side of the item area in GUI pixels.
const ITEM_GUI_SIZE: f32 = 16.0;
/// Full width of the durability bar in GUI pixels.
const BAR_WIDTH: f32 = 13.0;
/// Colour of the stack count text.
const COUNT_COLOR: Color = [1.0, 1.0, 1.0, 1.0];
/// Colour of the count's drop shadow (vanilla darkens the colour to a quarter).
const COUNT_SHADOW: Color = [0.25, 0.25, 0.25, 1.0];

/// The computed durability bar of a damaged item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurabilityBar {
    /// Width of the coloured bar in GUI pixels, `0..=13`.
    pub width_px: u8,
    /// Colour of the bar, green when fresh and red when nearly broken.
    pub rgb: [u8; 3],
}

impl DurabilityBar {
    /// Colour of the dimmed strip drawn under the bar, as in vanilla.
    fn backdrop_rgb(self) -> [u8; 3] {
        [(255 - self.rgb[1]) / 4, 64, 0]
    }
}

/// Computes the durability bar for `damage` out of `max_durability`.
///
/// Returns `None` when the item is unbreakable (`max_durability <= 0`) or
/// undamaged (`damage <= 0`), matching `ItemStack.isItemDamaged`. Damage above
/// the maximum is clamped. The formulas are those of
/// `RenderItem.renderItemOverlayIntoGUI` (MC 1.8): the width is
/// `round(13 - damage * 13 / max)` and the green channel is
/// `round(255 - damage * 255 / max)` with red as its complement.
///
/// ```
/// use umicraft_ui::widgets::durability_bar;
/// assert_eq!(durability_bar(0, 100), None);
/// assert_eq!(durability_bar(50, 100).map(|bar| bar.width_px), Some(7));
/// ```
#[must_use]
pub fn durability_bar(damage: i16, max_durability: i16) -> Option<DurabilityBar> {
    if max_durability <= 0 || damage <= 0 {
        return None;
    }
    let fraction = f64::from(damage.min(max_durability)) / f64::from(max_durability);
    // Java's Math.round rounds half up; f64::round rounds half away from zero,
    // which is identical for the non-negative values used here.
    let width_px = (BAR_WIDTH as f64 - fraction * BAR_WIDTH as f64).round() as u8;
    let green = (255.0 - fraction * 255.0).round() as u8;
    Some(DurabilityBar {
        width_px,
        rgb: [255 - green, green, 0],
    })
}

/// Draws an item icon with its stack count and durability bar.
///
/// `cell` is the 16x16 GUI-pixel item area already scaled to physical pixels;
/// every offset below scales with `cell.width / 16`. Commands are emitted in
/// this order: the [`crate::DrawCommand::Item`] icon, the count (shadow first)
/// when `item.count > 1`, then the durability bar when the item is damaged.
pub fn draw_item(draw: &mut DrawList, atlas: &FontAtlas, cell: Rect, item: &SlotItem) {
    let scale = cell.width / ITEM_GUI_SIZE;
    draw.item(item.key, cell, item.glint);
    if item.count > 1 {
        draw_count(draw, atlas, cell, scale, item.count);
    }
    if let Some(bar) = durability_bar(item.damage, item.max_durability) {
        draw_bar(draw, cell, scale, bar);
    }
}

/// Draws the stack count right-aligned at the bottom of the cell.
fn draw_count(draw: &mut DrawList, atlas: &FontAtlas, cell: Rect, scale: f32, count: u8) {
    let label = count.to_string();
    let width = atlas.measure(&label, scale);
    // Vanilla: x + 19 - 2 - width, y + 6 + 3 (GUI pixels).
    let x = cell.x + 17.0 * scale - width;
    let y = cell.y + 9.0 * scale;
    draw.text(
        [x + scale, y + scale],
        label.clone(),
        TextStyle::new(scale, COUNT_SHADOW),
    );
    draw.text([x, y], label, TextStyle::new(scale, COUNT_COLOR));
}

/// Draws the black frame, the dimmed strip and the coloured durability bar.
fn draw_bar(draw: &mut DrawList, cell: Rect, scale: f32, bar: DurabilityBar) {
    let x = cell.x + 2.0 * scale;
    let y = cell.y + 13.0 * scale;
    let strip = |width: f32, height: f32| Rect::new(x, y, width * scale, height * scale);
    draw.rect(strip(BAR_WIDTH, 2.0), [0.0, 0.0, 0.0, 1.0]);
    draw.rect(strip(12.0, 1.0), rgb_color(bar.backdrop_rgb()));
    draw.rect(strip(f32::from(bar.width_px), 1.0), rgb_color(bar.rgb));
}

/// Converts 8-bit RGB into an opaque [`Color`].
fn rgb_color([r, g, b]: [u8; 3]) -> Color {
    [
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        1.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DrawCommand;

    fn item(count: u8, damage: i16, max_durability: i16, glint: bool) -> SlotItem {
        SlotItem {
            key: 42,
            count,
            damage,
            max_durability,
            glint,
        }
    }

    fn draw(cell: Rect, item: &SlotItem) -> Vec<DrawCommand> {
        let mut list = DrawList::new();
        draw_item(&mut list, &FontAtlas::fallback(), cell, item);
        list.commands().to_vec()
    }

    #[test]
    fn bar_values_follow_the_java_formula() {
        // damage 1/4: width round(13 - 3.25) = 10, g = round(255 - 63.75) = 191.
        let bar = durability_bar(1, 4).expect("damaged");
        assert_eq!((bar.width_px, bar.rgb), (10, [64, 191, 0]));
        // half: width round(6.5) = 7 (half rounds up), g = round(127.5) = 128.
        let bar = durability_bar(50, 100).expect("damaged");
        assert_eq!((bar.width_px, bar.rgb), (7, [127, 128, 0]));
        // nearly broken: width round(0.13) = 0, g = round(2.55) = 3.
        let bar = durability_bar(99, 100).expect("damaged");
        assert_eq!((bar.width_px, bar.rgb), (0, [252, 3, 0]));
        // fully broken
        let bar = durability_bar(100, 100).expect("damaged");
        assert_eq!((bar.width_px, bar.rgb), (0, [255, 0, 0]));
    }

    #[test]
    fn bar_is_absent_for_undamaged_or_unbreakable_items() {
        assert_eq!(durability_bar(0, 100), None);
        assert_eq!(durability_bar(5, 0), None);
        assert_eq!(durability_bar(-3, 100), None);
    }

    #[test]
    fn damage_above_the_maximum_is_clamped() {
        assert_eq!(durability_bar(500, 100), durability_bar(100, 100));
    }

    #[test]
    fn a_plain_item_emits_only_the_icon() {
        let cell = Rect::new(10.0, 20.0, 32.0, 32.0);
        assert_eq!(
            draw(cell, &item(1, 0, 0, false)),
            [DrawCommand::Item {
                key: 42,
                dst: cell,
                glint: false
            }]
        );
    }

    #[test]
    fn glint_is_forwarded() {
        let commands = draw(Rect::new(0.0, 0.0, 16.0, 16.0), &item(1, 0, 0, true));
        assert!(matches!(commands[0], DrawCommand::Item { glint: true, .. }));
    }

    #[test]
    fn the_count_is_drawn_with_a_shadow_and_hidden_for_one() {
        let atlas = FontAtlas::fallback();
        let cell = Rect::new(0.0, 0.0, 16.0, 16.0);
        let commands = draw(cell, &item(64, 0, 0, false));
        assert_eq!(commands.len(), 3);
        let width = atlas.measure("64", 1.0);
        match (&commands[1], &commands[2]) {
            (
                DrawCommand::Text {
                    position: shadow,
                    style: shadow_style,
                    ..
                },
                DrawCommand::Text {
                    position, style, ..
                },
            ) => {
                assert_eq!(*position, [17.0 - width, 9.0]);
                assert_eq!(*shadow, [18.0 - width, 10.0]);
                assert_eq!(style.color, COUNT_COLOR);
                assert_eq!(shadow_style.color, COUNT_SHADOW);
            }
            other => panic!("expected shadow and text, got {other:?}"),
        }
    }

    #[test]
    fn a_damaged_item_draws_the_bar_after_the_count() {
        let commands = draw(Rect::new(0.0, 0.0, 16.0, 16.0), &item(2, 50, 100, false));
        let kinds: Vec<_> = commands
            .iter()
            .map(|command| match command {
                DrawCommand::Item { .. } => 'i',
                DrawCommand::Text { .. } => 't',
                DrawCommand::Rect { .. } => 'r',
                _ => '?',
            })
            .collect();
        assert_eq!(kinds, ['i', 't', 't', 'r', 'r', 'r']);
        assert_eq!(
            commands[3],
            DrawCommand::Rect {
                rect: Rect::new(2.0, 13.0, 13.0, 2.0),
                color: [0.0, 0.0, 0.0, 1.0]
            }
        );
        assert!(matches!(
            commands[5],
            DrawCommand::Rect { rect, .. } if rect == Rect::new(2.0, 13.0, 7.0, 1.0)
        ));
    }

    #[test]
    fn geometry_scales_with_the_cell() {
        let cell = Rect::new(100.0, 50.0, 48.0, 48.0);
        let commands = draw(cell, &item(1, 50, 100, false));
        assert!(matches!(
            commands[1],
            DrawCommand::Rect { rect, .. } if rect == Rect::new(106.0, 89.0, 39.0, 6.0)
        ));
        assert!(matches!(
            commands[3],
            DrawCommand::Rect { rect, .. } if rect == Rect::new(106.0, 89.0, 21.0, 3.0)
        ));
    }
}

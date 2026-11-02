// SPDX-License-Identifier: MIT OR Apache-2.0

//! The bar above the hotbar: experience with its level number, or the horse
//! jump bar (MCP `GuiIngame.renderExpBar`, `renderHorseJumpBar`).

use umicraft_game::hud::{BottomBar, LEVEL_COLOR};
use umicraft_ui::{FontAtlas, SpriteSheet, TextStyle};

use super::icons::{Canvas, Region};
use crate::hud::HudState;

/// Width of the bar texture.
const BAR_WIDTH: u32 = 182;
/// The empty experience bar.
const XP_BACK: Region = (0, 64, BAR_WIDTH, 5);
/// Top of the filled experience bar in `icons.png`.
const XP_FILL_V: u32 = 69;
/// The empty jump bar.
const JUMP_BACK: Region = (0, 84, BAR_WIDTH, 5);
/// Top of the filled jump bar in `icons.png`.
const JUMP_FILL_V: u32 = 89;

/// Draws the bar selected by `state.bar`.
pub(super) fn draw_bottom_bar(
    canvas: &mut Canvas<'_>,
    state: &HudState,
    font: &FontAtlas,
    icons: &SpriteSheet,
) {
    let left = canvas.width / 2 - 91;
    let top = canvas.height - 32 + 3;
    match state.bar {
        BottomBar::None => {}
        BottomBar::Experience { fill, level } => {
            draw_bar(canvas, icons, XP_BACK, XP_FILL_V, fill, (left, top));
            if let Some(level) = level {
                draw_level(canvas, font, level);
            }
        }
        BottomBar::HorseJump { fill } => {
            draw_bar(canvas, icons, JUMP_BACK, JUMP_FILL_V, fill, (left, top));
        }
    }
}

/// The empty bar and its filled part, clamped to the texture: a full bar
/// computes 183 pixels, one more than the texture holds.
fn draw_bar(
    canvas: &mut Canvas<'_>,
    icons: &SpriteSheet,
    back: Region,
    fill_v: u32,
    fill: u32,
    (x, y): (i32, i32),
) {
    canvas.sprite(icons, back, x, y);
    let fill = fill.min(BAR_WIDTH);
    if fill > 0 {
        canvas.sprite(icons, (0, fill_v, fill, 5), x, y);
    }
}

/// The level number: black on four sides, then green, centred
/// (`renderExpBar`: `drawString` four times at +-1 pixel, then in `8453920`).
fn draw_level(canvas: &mut Canvas<'_>, font: &FontAtlas, level: i32) {
    let text = level.to_string();
    let x = (canvas.width - font.measure(&text, 1.0) as i32) / 2;
    let y = canvas.height - 31 - 4;
    let scale = canvas.scale;
    let black = TextStyle::new(scale, [0.0, 0.0, 0.0, 1.0]);
    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let at = [(x + dx) as f32 * scale, (y + dy) as f32 * scale];
        canvas.list().text(at, &text, black);
    }
    let [r, g, b] = [16, 8, 0].map(|shift| ((LEVEL_COLOR >> shift) & 0xFF) as f32 / 255.0);
    let green = TextStyle::new(scale, [r, g, b, 1.0]);
    canvas
        .list()
        .text([x as f32 * scale, y as f32 * scale], &text, green);
}

#[cfg(test)]
mod tests {
    use umicraft_ui::{DrawCommand, DrawList};

    use super::*;

    fn draw(bar: BottomBar) -> DrawList {
        let mut list = DrawList::new();
        let sheet = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 256, 256);
        let state = HudState {
            bar,
            ..HudState::default()
        };
        let font = FontAtlas::fallback();
        let mut canvas = Canvas::new(&mut list, (640, 480));
        draw_bottom_bar(&mut canvas, &state, &font, &sheet);
        list
    }

    fn images(list: &DrawList) -> Vec<f32> {
        list.commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Image { dst, .. } => Some(dst.width),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_experience_bar_fill_is_clamped_to_the_texture() {
        // 640x480 is scale 2: 182 units = 364 pixels.
        let list = draw(BottomBar::Experience {
            fill: 183,
            level: None,
        });
        assert_eq!(images(&list), [364.0, 364.0]);
    }

    #[test]
    fn an_empty_bar_draws_only_the_background() {
        let list = draw(BottomBar::Experience {
            fill: 0,
            level: None,
        });
        assert_eq!(images(&list).len(), 1);
    }

    #[test]
    fn the_level_is_four_black_copies_and_one_green() {
        let list = draw(BottomBar::Experience {
            fill: 0,
            level: Some(12),
        });
        let colors: Vec<_> = list
            .commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Text { style, text, .. } if text == "12" => Some(style.color),
                _ => None,
            })
            .collect();
        assert_eq!(colors.len(), 5);
        assert!(colors[..4].iter().all(|c| c[..3] == [0.0, 0.0, 0.0]));
        assert_eq!(colors[4][0], 128.0 / 255.0);
        assert_eq!(colors[4][1], 1.0);
        assert_eq!(colors[4][2], 32.0 / 255.0);
    }

    #[test]
    fn nothing_is_drawn_in_creative() {
        assert!(draw(BottomBar::None).is_empty());
    }

    #[test]
    fn the_jump_bar_sits_where_the_experience_bar_does() {
        let top = |bar| match &draw(bar).commands()[0] {
            DrawCommand::Image { dst, .. } => dst.y,
            other => panic!("{other:?}"),
        };
        assert_eq!(
            top(BottomBar::HorseJump { fill: 10 }),
            top(BottomBar::Experience {
                fill: 10,
                level: None
            })
        );
    }
}

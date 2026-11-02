// SPDX-License-Identifier: MIT OR Apache-2.0

//! The scoreboard sidebar on the right edge (`GuiIngame.renderScoreboard`).
//!
//! The lines are laid out in whole GUI units: the block is vertically
//! centred on `height / 2 + lines * 9 / 3`, three units from the right edge,
//! and as wide as the widest of the title and the `name: score` lines. The
//! text is opaque: the reference draws it with alpha 0x20 but `drawRect` has
//! just disabled blending.

use umicraft_ui::{DrawList, FontAtlas, Rect};

use super::runs::{draw_runs, width};
use crate::hud::{ChatRunView, HudState, SidebarView};

/// The height of a text line (`FontRenderer.FONT_HEIGHT`).
const ROW_HEIGHT: i32 = 9;
/// The distance of the block from the right edge.
const MARGIN: i32 = 3;
/// `1342177280`, the `0x50000000` band behind a row and under the title.
const ROW_BAND: [f32; 4] = [0.0, 0.0, 0.0, 80.0 / 255.0];
/// `1610612736`, the `0x60000000` band behind the title.
const TITLE_BAND: [f32; 4] = [0.0, 0.0, 0.0, 96.0 / 255.0];
/// `EnumChatFormatting.RED` (`§c`), the colour of the scores.
const RED: [u8; 3] = [0xFF, 0x55, 0x55];

/// One score as a red run.
fn score_runs(score: i32) -> Vec<ChatRunView> {
    vec![ChatRunView {
        text: score.to_string(),
        color: RED,
        bold: false,
        italic: false,
        underlined: false,
        strikethrough: false,
        obfuscated: false,
    }]
}

/// Draws the sidebar; nothing when it has no rows, as the title is drawn
/// together with the last row.
pub(super) fn draw_sidebar(
    list: &mut DrawList,
    state: &HudState,
    sidebar: &SidebarView,
    (gw, gh): (f32, f32),
    sf: f32,
    font: &FontAtlas,
) {
    let count = sidebar.lines.len() as i32;
    if count == 0 {
        return;
    }
    let colon = font.measure(": ", 1.0);
    let widest = sidebar
        .lines
        .iter()
        .map(|line| width(font, &line.name) + colon + width(font, &score_runs(line.score)))
        .fold(width(font, &sidebar.title), f32::max)
        .ceil() as i32;

    let screen_w = gw as i32;
    let centre = gh as i32 / 2 + count * ROW_HEIGHT / 3;
    let left = screen_w - widest - MARGIN;
    let right = screen_w - MARGIN + 2;
    let unit = |value: i32| value as f32 * sf;
    let band = |list: &mut DrawList, top: i32, height: i32, color| {
        let rect = Rect::new(
            unit(left - 2),
            unit(top),
            unit(right - left + 2),
            unit(height),
        );
        list.rect(rect, color);
    };

    for (index, line) in sidebar.lines.iter().enumerate() {
        let top = centre - (count - index as i32) * ROW_HEIGHT;
        band(list, top, ROW_HEIGHT, ROW_BAND);
        let at = |x: i32| [unit(x), unit(top)];
        draw_runs(list, font, state, at(left), &line.name, sf, 1.0, false);
        let score = score_runs(line.score);
        let score_x = right - width(font, &score).ceil() as i32;
        draw_runs(list, font, state, at(score_x), &score, sf, 1.0, false);

        if index == 0 {
            band(list, top - ROW_HEIGHT - 1, ROW_HEIGHT, TITLE_BAND);
            band(list, top - 1, 1, ROW_BAND);
            let title_x = left + widest / 2 - width(font, &sidebar.title) as i32 / 2;
            let title_at = [unit(title_x), unit(top - ROW_HEIGHT)];
            draw_runs(list, font, state, title_at, &sidebar.title, sf, 1.0, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::runs::run;
    use super::*;
    use crate::hud::SidebarLineView;
    use umicraft_ui::DrawCommand;

    fn view(rows: &[(&str, i32)]) -> SidebarView {
        SidebarView {
            title: vec![run("Title")],
            lines: rows
                .iter()
                .map(|(name, score)| SidebarLineView {
                    name: vec![run(name)],
                    score: *score,
                })
                .collect(),
        }
    }

    fn draw(view: &SidebarView) -> DrawList {
        let mut list = DrawList::new();
        let font = FontAtlas::fallback();
        draw_sidebar(
            &mut list,
            &HudState::default(),
            view,
            (320.0, 240.0),
            1.0,
            &font,
        );
        list
    }

    fn rects(list: &DrawList) -> Vec<(Rect, [f32; 4])> {
        list.commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Rect { rect, color } => Some((*rect, *color)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_sidebar_without_rows_draws_nothing() {
        assert!(draw(&view(&[])).is_empty());
    }

    #[test]
    fn the_bands_end_one_unit_from_the_right_edge() {
        let list = draw(&view(&[("Bob", 9), ("Alex", 4)]));
        for (rect, _) in rects(&list) {
            assert_eq!(rect.x + rect.width, 319.0);
        }
    }

    #[test]
    fn two_rows_are_centred_on_half_the_height_plus_a_third() {
        // centre = 120 + 18 / 3 = 126; the bottom row spans 117..126.
        let list = draw(&view(&[("Bob", 9), ("Alex", 4)]));
        let bands = rects(&list);
        let bottom = bands.iter().find(|(rect, _)| rect.y == 117.0).expect("row");
        assert_eq!(bottom.0.height, 9.0);
        let title = bands
            .iter()
            .find(|(rect, _)| rect.y == 98.0)
            .expect("title");
        assert!((title.1[3] - 96.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn the_scores_are_red_and_the_names_white() {
        let list = draw(&view(&[("Bob", 9)]));
        let colours: Vec<([f32; 4], String)> = list
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, style, .. } => Some((style.color, text.clone())),
                _ => None,
            })
            .collect();
        let score = colours.iter().find(|(_, text)| text == "9").expect("score");
        assert!(score.0[0] > 0.99 && score.0[1] < 0.4);
        let name = colours
            .iter()
            .find(|(_, text)| text == "Bob")
            .expect("name");
        assert_eq!(name.0, [1.0; 4]);
    }
}

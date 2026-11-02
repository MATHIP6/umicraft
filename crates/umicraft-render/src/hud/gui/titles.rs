// SPDX-License-Identifier: MIT OR Apache-2.0

//! The title and the subtitle in the middle of the screen
//! (`GuiIngame.renderGameOverlay`, `field_175195_w` branch).
//!
//! Both are drawn with a shadow, centred on the screen centre: the title at
//! 4x scale from `y = -10`, the subtitle at 2x from `y = 5`, in the scaled
//! units of their own scale.

use umicraft_ui::{DrawList, FontAtlas};

use super::runs::{draw_runs, width};
use crate::hud::{ChatRunView, HudState, TitleView};

/// The scale of the title.
const TITLE_SCALE: f32 = 4.0;
/// The offset of the title above the centre, in its own units.
const TITLE_Y: f32 = -10.0;
/// The scale of the subtitle.
const SUBTITLE_SCALE: f32 = 2.0;
/// The offset of the subtitle below the centre, in its own units.
const SUBTITLE_Y: f32 = 5.0;

/// Draws one line centred on `(centre_x, centre_y)` GUI units.
fn draw_centred(
    list: &mut DrawList,
    state: &HudState,
    font: &FontAtlas,
    runs: &[ChatRunView],
    centre: [f32; 2],
    (scale, y, alpha): (f32, f32, f32),
    sf: f32,
) {
    if runs.is_empty() {
        return;
    }
    let x = -((width(font, runs) as i32 / 2) as f32);
    let at = [(centre[0] + x * scale) * sf, (centre[1] + y * scale) * sf];
    draw_runs(list, font, state, at, runs, scale * sf, alpha, true);
}

/// Draws the title and the subtitle.
pub(super) fn draw_title(
    list: &mut DrawList,
    state: &HudState,
    title: &TitleView,
    (gw, gh): (f32, f32),
    sf: f32,
    font: &FontAtlas,
) {
    let centre = [(gw as i32 / 2) as f32, (gh as i32 / 2) as f32];
    let alpha = title.alpha.clamp(0.0, 1.0);
    let lines = [
        (&title.title, (TITLE_SCALE, TITLE_Y, alpha)),
        (&title.subtitle, (SUBTITLE_SCALE, SUBTITLE_Y, alpha)),
    ];
    for (runs, placement) in lines {
        draw_centred(list, state, font, runs, centre, placement, sf);
    }
}

#[cfg(test)]
mod tests {
    use super::super::runs::run;
    use super::*;
    use umicraft_ui::DrawCommand;

    fn texts(list: &DrawList) -> Vec<([f32; 2], f32, [f32; 4])> {
        list.commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text {
                    position, style, ..
                } => Some((*position, style.scale, style.color)),
                _ => None,
            })
            .collect()
    }

    fn draw(title: &TitleView) -> DrawList {
        let mut list = DrawList::new();
        let font = FontAtlas::fallback();
        draw_title(
            &mut list,
            &HudState::default(),
            title,
            (320.0, 240.0),
            1.0,
            &font,
        );
        list
    }

    #[test]
    fn the_title_is_four_times_and_the_subtitle_twice_as_large() {
        let list = draw(&TitleView {
            title: vec![run("Big")],
            subtitle: vec![run("small")],
            alpha: 1.0,
        });
        let scales: Vec<f32> = texts(&list).iter().map(|(_, scale, _)| *scale).collect();
        // Shadow then text for each.
        assert_eq!(scales, [4.0, 4.0, 2.0, 2.0]);
    }

    #[test]
    fn the_title_sits_forty_pixels_above_the_centre() {
        let list = draw(&TitleView {
            title: vec![run("Big")],
            subtitle: vec![],
            alpha: 1.0,
        });
        let (position, _, _) = texts(&list)[1];
        assert_eq!(position[1], 120.0 - 40.0);
    }

    #[test]
    fn the_subtitle_sits_ten_pixels_below_the_centre() {
        let list = draw(&TitleView {
            title: vec![],
            subtitle: vec![run("s")],
            alpha: 1.0,
        });
        let (position, _, _) = texts(&list)[1];
        assert_eq!(position[1], 120.0 + 10.0);
    }

    #[test]
    fn the_alpha_reaches_the_text() {
        let list = draw(&TitleView {
            title: vec![run("x")],
            subtitle: vec![],
            alpha: 0.5,
        });
        assert!(texts(&list).iter().all(|(_, _, color)| color[3] == 0.5));
    }
}

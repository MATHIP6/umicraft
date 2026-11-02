// SPDX-License-Identifier: MIT OR Apache-2.0

//! The action bar above the hotbar (`GuiIngame.renderGameOverlay`,
//! `overlayMessage` branch): centred text without a shadow, 72 units above
//! the bottom edge (`translate(width / 2, height - 68)`, then `y = -4`).

use umicraft_ui::{DrawList, FontAtlas};

use super::runs::{draw_runs, width};
use crate::hud::{ActionBarView, HudState};

/// The distance of the top of the text from the bottom edge.
const ABOVE_BOTTOM: f32 = 72.0;

/// Draws the action bar message.
pub(super) fn draw_action_bar(
    list: &mut DrawList,
    state: &HudState,
    bar: &ActionBarView,
    (gw, gh): (f32, f32),
    sf: f32,
    font: &FontAtlas,
) {
    let x = (gw as i32 / 2) as f32 - (width(font, &bar.text) as i32 / 2) as f32;
    let y = (gh as i32) as f32 - ABOVE_BOTTOM;
    draw_runs(
        list,
        font,
        state,
        [x * sf, y * sf],
        &bar.text,
        sf,
        bar.alpha.clamp(0.0, 1.0),
        false,
    );
}

#[cfg(test)]
mod tests {
    use super::super::runs::run;
    use super::*;
    use umicraft_ui::DrawCommand;

    #[test]
    fn the_message_has_no_shadow_and_sits_above_the_hotbar() {
        let mut list = DrawList::new();
        let bar = ActionBarView {
            text: vec![run("Go")],
            alpha: 0.5,
        };
        let font = FontAtlas::fallback();
        draw_action_bar(
            &mut list,
            &HudState::default(),
            &bar,
            (320.0, 240.0),
            2.0,
            &font,
        );
        let texts: Vec<_> = list
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text {
                    position, style, ..
                } => Some((*position, style.color[3])),
                _ => None,
            })
            .collect();
        assert_eq!(texts.len(), 1, "no shadow");
        assert_eq!(texts[0].0[1], (240.0 - 72.0) * 2.0);
        assert_eq!(texts[0].1, 0.5);
    }
}

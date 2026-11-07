// SPDX-License-Identifier: MIT OR Apache-2.0

//! The name of the held item above the status rows (MCP
//! `GuiIngame.func_181551_a`).

use umicraft_game::chat::local;
use umicraft_game::hud::HeldNameView;
use umicraft_ui::styled_text::{draw_styled, styled_width, StyledRun};
use umicraft_ui::FontAtlas;

use super::icons::Canvas;

/// Draws the name centred at `height - 59` with a shadow, white at the alpha
/// of the view. Vanilla prefixes the name with the italic code when the
/// player named the stack; the font draws it upright (see
/// [`umicraft_ui::styled_text`]).
pub(super) fn draw_held_name(canvas: &mut Canvas<'_>, name: &HeldNameView, font: &FontAtlas) {
    // The name of a stack keeps its `§` codes (a renamed or plugin item).
    let alpha = f32::from(name.alpha) / 255.0;
    let runs: Vec<StyledRun> = local(&name.text)
        .into_iter()
        .map(|run| {
            let [r, g, b] = run.style.color.map_or([255; 3], |color| color.rgb());
            StyledRun {
                bold: run.style.bold,
                italic: run.style.italic || name.italic,
                underlined: run.style.underlined,
                strikethrough: run.style.strikethrough,
                obfuscated: run.style.obfuscated,
                ..StyledRun::plain(
                    run.text,
                    [
                        f32::from(r) / 255.0,
                        f32::from(g) / 255.0,
                        f32::from(b) / 255.0,
                        alpha,
                    ],
                )
            }
        })
        .collect();
    let width = styled_width(font, &runs, 1.0) as i32;
    let x = (canvas.width - width) / 2;
    let y = canvas.height - 59 + name.dy;
    let scale = canvas.scale;
    draw_styled(
        canvas.list(),
        font,
        [x as f32 * scale, y as f32 * scale],
        &runs,
        scale,
        true,
        0,
    );
}

#[cfg(test)]
mod tests {
    use umicraft_ui::{DrawCommand, DrawList};

    use super::*;

    #[test]
    fn the_name_is_drawn_with_its_alpha_and_a_shadow() {
        let mut list = DrawList::new();
        let font = FontAtlas::fallback();
        let view = HeldNameView {
            text: "Stone".to_owned(),
            italic: false,
            alpha: 128,
            dy: 0,
        };
        let mut canvas = Canvas::new(&mut list, (640, 480));
        draw_held_name(&mut canvas, &view, &font);
        let alphas: Vec<f32> = list
            .commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Text { style, .. } => Some(style.color[3]),
                _ => None,
            })
            .collect();
        assert!(alphas.len() >= 2, "shadow and text");
        assert!(alphas.iter().all(|a| (*a - 128.0 / 255.0).abs() < 1e-6));
    }

    #[test]
    fn section_codes_colour_the_name_and_are_not_drawn() {
        let mut list = DrawList::new();
        let font = FontAtlas::fallback();
        let view = HeldNameView {
            text: "§bSel §7(Clic)".to_owned(),
            italic: false,
            alpha: 255,
            dy: 0,
        };
        let mut canvas = Canvas::new(&mut list, (640, 480));
        draw_held_name(&mut canvas, &view, &font);
        let texts: Vec<String> = list
            .commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Text { text, .. } => Some(text.to_string()),
                _ => None,
            })
            .collect();
        assert!(texts.iter().all(|t| !t.contains('§')), "{texts:?}");
        assert!(texts.iter().any(|t| t == "Sel "), "{texts:?}");
    }

    #[test]
    fn creative_moves_the_name_down_by_fourteen_units() {
        let y = |dy| {
            let mut list = DrawList::new();
            let font = FontAtlas::fallback();
            let view = HeldNameView {
                text: "A".to_owned(),
                italic: false,
                alpha: 255,
                dy,
            };
            let mut canvas = Canvas::new(&mut list, (640, 480));
            draw_held_name(&mut canvas, &view, &font);
            match list.commands().last() {
                Some(DrawCommand::Text { position, .. }) => position[1],
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(y(14) - y(0), 14.0 * 2.0);
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The text caption of the server-less demos (`--block-demo`, `--entity-demo`).
//!
//! The acceptance shots show blocks and mobs, not a hotbar, so the caption is
//! only the left column of the vanilla F3 screen drawn by the textured HUD
//! renderer with no sprite sheets. This replaced the flat-colour `Overlay`
//! caption when the legacy HUD was deleted (P9-h, ADR-0036).

use umicraft_render::{draw_hud_gui, Camera, DebugF3View, HudSheets, HudState, NoIcons, Renderer};
use umicraft_ui::{DrawList, FontAtlas};

use crate::error::ClientError;
use crate::ui_font;

/// Builds the draw list of a caption: one F3-style line per entry.
#[must_use]
pub(crate) fn caption_list(lines: &[String], viewport: (u32, u32), font: &FontAtlas) -> DrawList {
    let state = HudState {
        crosshair: false,
        vitals: None,
        debug_f3: Some(DebugF3View {
            left: lines.to_vec(),
            right: Vec::new(),
        }),
        ..HudState::default()
    };
    let mut list = DrawList::new();
    draw_hud_gui(&mut list, &state, viewport, font, &HudSheets::default());
    list
}

/// Renders the scene seen by `camera` with `lines` as caption.
///
/// # Errors
/// The font atlas or the frame was rejected by the renderer.
pub(crate) fn render_captioned(
    renderer: &mut Renderer,
    camera: &Camera,
    lines: &[String],
    viewport: (u32, u32),
) -> Result<(), ClientError> {
    let installed = ui_font::install(renderer, &ui_font::load().atlas)?;
    let list = caption_list(lines, viewport, &installed.atlas);
    renderer.render_with_ui(
        camera,
        &umicraft_render::Overlay::new(viewport.0, viewport.1),
        &list,
        &mut NoIcons,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_ui::DrawCommand;

    #[test]
    fn every_caption_line_becomes_a_text_command() {
        let lines = ["one".to_owned(), "two".to_owned()];
        let list = caption_list(&lines, (640, 360), &FontAtlas::fallback());
        let texts: Vec<&str> = list
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, ["one", "two"]);
    }
}

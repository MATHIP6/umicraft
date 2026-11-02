// SPDX-License-Identifier: MIT OR Apache-2.0

//! The crosshair (MCP `GuiIngame.renderGameOverlay`): the `icons.png` cross at
//! the screen centre, blended so it inverts whatever is behind it.

use umicraft_ui::SpriteSheet;

use super::icons::{Canvas, Region};

/// The cross in `icons.png`.
const CROSSHAIR: Region = (0, 0, 16, 16);

/// Draws the crosshair at `(width / 2 - 7, height / 2 - 7)` with the blend
/// function `(ONE_MINUS_DST_COLOR, ONE_MINUS_SRC_COLOR)`.
pub(super) fn draw_crosshair(canvas: &mut Canvas<'_>, icons: &SpriteSheet) {
    let (x, y) = (canvas.width / 2 - 7, canvas.height / 2 - 7);
    canvas.sprite_inverted(icons, CROSSHAIR, x, y);
}

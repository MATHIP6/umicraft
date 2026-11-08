// SPDX-License-Identifier: MIT OR Apache-2.0

//! The footer of the modern title screen: the version at the bottom left and
//! the non-affiliation credit at the bottom right.

use crate::context::Ui;
use crate::menus::common::Gui;
use crate::modern::theme::with_opacity;
use crate::modern::{draw_text, theme, Align};
use crate::text::FontAtlas;
use umicraft_core::lang::Lang;

/// Distance of the footer texts from the window edges, in GUI units.
const FOOTER_MARGIN: f32 = 4.0;

/// The version at the bottom left and the credit at the bottom right,
/// `alpha` opaque.
pub(super) fn draw_footer(
    ui: &mut Ui,
    brand: &FontAtlas,
    gui: &Gui,
    lang: &Lang,
    version: &str,
    alpha: f32,
) {
    let size = theme::TEXT_SIZE_SMALL * gui.scale;
    let color = with_opacity(theme::TEXT_MUTED, alpha);
    let y = gui
        .rect(
            0.0,
            gui.height - FOOTER_MARGIN - theme::TEXT_SIZE_SMALL * 0.5,
            0.0,
            0.0,
        )
        .y;
    let left = gui.rect(FOOTER_MARGIN, 0.0, 0.0, 0.0).x;
    let right = gui.rect(gui.width - FOOTER_MARGIN, 0.0, 0.0, 0.0).x;
    draw_text(ui, brand, version, (left, y), (size, color, Align::Left));
    let credit = lang.tr("umicraft.title.credit");
    draw_text(ui, brand, credit, (right, y), (size, color, Align::Right));
}

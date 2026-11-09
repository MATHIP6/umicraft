// SPDX-License-Identifier: MIT OR Apache-2.0

//! The selection frame drawn around a widget of the HUD layer, with the
//! handle of its corner.

use crate::context::Ui;
use crate::draw::Color;
use crate::geom::Rect;
use crate::menus::common::Gui;
use crate::modern::theme::{with_opacity, RADIUS, TEXT, TEXT_SIZE_BODY};
use crate::modern::{draw_text, measure_text, Align};
use crate::text::FontAtlas;

use super::drag::{clamp_into, handle_rect};

/// Thickness of the frame lines, in physical pixels: thin enough to stay out of
/// the way, thick enough to read on the sky.
const EDGE_PIXELS: f32 = 2.0;
/// The plate behind the texts of the layer, so they read on any scenery.
pub(super) const LABEL_FILL: Color = [0.05, 0.06, 0.08, 0.72];
/// Space between a label and the edge of its plate, in GUI units.
const LABEL_PAD: f32 = 3.0;

/// Air between the text of a widget and its frame, in GUI units.
pub(super) const MARGIN: f32 = 1.0;

/// The frame of the widget whose text covers `rect` on a screen of `screen` GUI
/// units: a little larger than the text on every side, and taller by the strip the
/// gear sits in, under the text or, at the bottom of the screen, above it. It is
/// what the pointer grabs and what is drawn; the placement of the widget stays that
/// of its text. It never leaves the screen.
pub(super) fn frame_rect(rect: Rect, screen: [f32; 2]) -> Rect {
    let strip = super::gear::strip(rect);
    let (above, below) = if super::gear::strip_below(rect, screen) {
        (MARGIN, MARGIN + strip)
    } else {
        (MARGIN + strip, MARGIN)
    };
    let frame = Rect::new(
        rect.x - MARGIN,
        rect.y - above,
        rect.width + 2.0 * MARGIN,
        rect.height + above + below,
    );
    // A widget against an edge of the screen keeps its whole frame, and so the
    // corner handle and the gear, in view.
    clamp_into(frame, screen)
}

/// How a frame is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FrameKind {
    /// The pointer is over the widget.
    Hovered,
    /// The widget is the one the bar acts on.
    Selected,
}

impl FrameKind {
    fn color(self) -> Color {
        match self {
            Self::Hovered => with_opacity(TEXT, 0.55),
            Self::Selected => TEXT,
        }
    }
}

/// Draws the frame of `rect` (GUI units) and its corner handle.
pub(super) fn draw_frame(ui: &mut Ui, gui: &Gui, rect: Rect, kind: FrameKind) {
    let color = kind.color();
    let edge = EDGE_PIXELS / gui.scale;
    let outer = Rect::new(
        rect.x - edge,
        rect.y - edge,
        rect.width + 2.0 * edge,
        rect.height + 2.0 * edge,
    );
    let strips = [
        Rect::new(outer.x, outer.y, outer.width, edge),
        Rect::new(outer.x, outer.bottom() - edge, outer.width, edge),
        Rect::new(outer.x, outer.y + edge, edge, outer.height - 2.0 * edge),
        Rect::new(
            outer.right() - edge,
            outer.y + edge,
            edge,
            outer.height - 2.0 * edge,
        ),
    ];
    for strip in strips {
        ui.draw()
            .rect(gui.rect(strip.x, strip.y, strip.width, strip.height), color);
    }
    let handle = handle_rect(rect);
    ui.draw().rect(
        gui.rect(handle.x, handle.y, handle.width, handle.height),
        color,
    );
}

/// Draws `text` centred on `at` (physical pixels) over a dark plate.
pub(super) fn draw_label(ui: &mut Ui, brand: &FontAtlas, gui: &Gui, text: &str, at: [f32; 2]) {
    let size = TEXT_SIZE_BODY * gui.scale;
    let width = measure_text(brand, text, size) + 2.0 * LABEL_PAD * gui.scale;
    let height = size + 2.0 * LABEL_PAD * gui.scale;
    let plate = Rect::new(at[0] - width * 0.5, at[1] - height * 0.5, width, height);
    ui.draw()
        .rounded_rect(plate, RADIUS * gui.scale, LABEL_FILL);
    draw_text(ui, brand, text, (at[0], at[1]), (size, TEXT, Align::Center));
}

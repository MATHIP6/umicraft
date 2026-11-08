// SPDX-License-Identifier: MIT OR Apache-2.0

//! The alignment guides drawn while a widget is dragged: a line where an edge
//! or the centre of the widget sits on an edge or the centre of the screen,
//! which is where `snap_rect` pulls it.

use crate::context::Ui;
use crate::draw::Color;
use crate::geom::Rect;
use crate::menus::common::Gui;

/// How close an edge must be to a guide line to count as on it, in GUI units.
const TOLERANCE: f32 = 0.01;
/// The colour of the guide lines: amber, which reads on the sky and on grass.
const COLOR: Color = [1.0, 0.78, 0.25, 0.95];
/// Thickness of the guide lines, in physical pixels.
const PIXELS: f32 = 2.0;

/// A line across the whole screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Guide {
    /// A vertical line at this x, in GUI units.
    Vertical(f32),
    /// A horizontal line at this y, in GUI units.
    Horizontal(f32),
}

/// The guides `rect` lies on: its left edge, centre or right edge on the
/// matching line of the screen, and the same for the top, middle and bottom.
pub(super) fn guides_for(rect: Rect, screen: [f32; 2]) -> Vec<Guide> {
    let mut guides = Vec::new();
    let horizontal = [rect.x, rect.x + rect.width * 0.5, rect.right()];
    let vertical = [rect.y, rect.y + rect.height * 0.5, rect.bottom()];
    for (axis, edges, total) in [(true, horizontal, screen[0]), (false, vertical, screen[1])] {
        for (edge, line) in edges.into_iter().zip([0.0, total * 0.5, total]) {
            if (edge - line).abs() <= TOLERANCE {
                guides.push(if axis {
                    Guide::Vertical(line)
                } else {
                    Guide::Horizontal(line)
                });
            }
        }
    }
    guides
}

/// Draws `guides` as one-pixel lines over the screen.
pub(super) fn draw_guides(ui: &mut Ui, gui: &Gui, guides: &[Guide], screen: [f32; 2]) {
    let thickness = PIXELS / gui.scale;
    for guide in guides {
        let rect = match *guide {
            Guide::Vertical(x) => {
                gui.rect((x - thickness * 0.5).max(0.0), 0.0, thickness, screen[1])
            }
            Guide::Horizontal(y) => {
                gui.rect(0.0, (y - thickness * 0.5).max(0.0), screen[0], thickness)
            }
        };
        ui.draw().rect(rect, COLOR);
    }
}

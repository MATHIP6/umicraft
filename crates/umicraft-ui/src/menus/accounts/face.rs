// SPDX-License-Identifier: MIT OR Apache-2.0

//! The head of an account row: the 8x8 face of the player's skin.
//!
//! The GUI draw list cannot sample a skin texture, so the face (RGBA, row by
//! row, as the tab list receives it) is drawn as one rectangle per run of equal
//! pixels. A row without a face keeps a flat skin-tone square.

use crate::context::Ui;
use crate::draw::Color;
use crate::menus::common::Gui;

/// Side of a face in pixels.
const FACE_SIDE: usize = 8;
/// Bytes of a face: 8x8 pixels of RGBA (256).
const FACE_BYTES: usize = FACE_SIDE * FACE_SIDE * 4;
/// The fill of a head without a face.
const PLACEHOLDER: Color = [0.72, 0.52, 0.40, 1.0];

/// Draws `face` in the `side` GUI pixel square at `at`, or the flat
/// placeholder when there is none (or it is not 8x8 RGBA).
pub(super) fn draw_face(ui: &mut Ui, gui: &Gui, face: Option<&[u8]>, at: [f32; 2], side: f32) {
    let [x, y] = at;
    let Some(face) = face.filter(|face| face.len() == FACE_BYTES) else {
        ui.draw().rect(gui.rect(x, y, side, side), PLACEHOLDER);
        return;
    };
    let step = side / FACE_SIDE as f32;
    let pixel = |column: usize, row: usize| &face[(row * FACE_SIDE + column) * 4..][..4];
    for row in 0..FACE_SIDE {
        let mut start = 0;
        while start < FACE_SIDE {
            let colour = pixel(start, row);
            let mut end = start + 1;
            while end < FACE_SIDE && pixel(end, row) == colour {
                end += 1;
            }
            if colour[3] > 0 {
                let color = [0, 1, 2, 3].map(|c| f32::from(colour[c]) / 255.0);
                let left = x + start as f32 * step;
                let top = y + row as f32 * step;
                let width = (end - start) as f32 * step;
                ui.draw().rect(gui.rect(left, top, width, step), color);
            }
            start = end;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::DrawCommand;
    use crate::geom::Rect;
    use crate::input::UiInput;
    use crate::menus::common::testing::{frame, VIEWPORT};

    fn rects(face: Option<&[u8]>) -> Vec<(Rect, Color)> {
        frame(UiInput::default(), |ui| {
            let gui = Gui::new(VIEWPORT);
            draw_face(ui, &gui, face, [10.0, 10.0], 32.0);
            ui.draw()
                .commands()
                .iter()
                .filter_map(|c| match c {
                    DrawCommand::Rect { rect, color } => Some((*rect, *color)),
                    _ => None,
                })
                .collect()
        })
    }

    /// A face whose left half is red and right half blue.
    fn two_colours() -> Vec<u8> {
        (0..64)
            .flat_map(|i| {
                if i % 8 < 4 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                }
            })
            .collect()
    }

    #[test]
    fn no_face_draws_one_flat_square() {
        assert_eq!(
            rects(None),
            vec![(Gui::new(VIEWPORT).rect(10.0, 10.0, 32.0, 32.0), PLACEHOLDER)]
        );
    }

    #[test]
    fn a_wrongly_sized_face_falls_back_to_the_flat_square() {
        assert_eq!(rects(Some(&[0; 12])).len(), 1);
    }

    #[test]
    fn equal_pixels_of_a_row_are_one_rectangle() {
        let drawn = rects(Some(&two_colours()));
        assert_eq!(drawn.len(), 16, "two runs in each of the eight rows");
        let colours: Vec<Color> = drawn.iter().map(|(_, c)| *c).collect();
        assert!(colours.contains(&[1.0, 0.0, 0.0, 1.0]));
        assert!(colours.contains(&[0.0, 0.0, 1.0, 1.0]));
    }

    #[test]
    fn transparent_pixels_are_skipped() {
        let mut face = two_colours();
        for pixel in face.chunks_exact_mut(4).take(8) {
            pixel[3] = 0;
        }
        assert_eq!(rects(Some(&face)).len(), 14);
    }
}

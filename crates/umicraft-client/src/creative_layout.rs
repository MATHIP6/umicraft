// SPDX-License-Identifier: MIT OR Apache-2.0

//! Placement of the creative window on the screen.
//!
//! Vanilla works in "scaled" GUI pixels: `ScaledResolution` divides the
//! display by the GUI scale (rounding up), `GuiContainer.initGui` centres the
//! 195 by 136 window with integer division and the mouse is converted with
//! `Mouse.getX() * scaledWidth / displayWidth`. [`CreativeLayout`] reproduces
//! that arithmetic, so a click lands on the same slot as in the reference
//! client at every window size.

use umicraft_game::inventory::creative::tabs::{WINDOW_HEIGHT, WINDOW_WIDTH};
use umicraft_ui::screens::gui_scale;
use umicraft_ui::Rect;

/// The creative window placed in a display.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CreativeLayout {
    scale: u32,
    /// `guiLeft` and `guiTop` in scaled pixels.
    origin: (i32, i32),
    /// `ScaledResolution.getScaledWidth/Height`.
    scaled: (i32, i32),
    /// The display size in physical pixels.
    display: (f32, f32),
}

impl CreativeLayout {
    /// The layout for a display of `width` by `height` physical pixels.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let scale = gui_scale(width, height);
        let scaled = |pixels: u32| i32::try_from(pixels.div_ceil(scale)).unwrap_or(i32::MAX);
        let (scaled_width, scaled_height) = (scaled(width), scaled(height));
        Self {
            scale,
            origin: (
                (scaled_width - WINDOW_WIDTH) / 2,
                (scaled_height - WINDOW_HEIGHT) / 2,
            ),
            scaled: (scaled_width, scaled_height),
            display: (width as f32, height as f32),
        }
    }

    /// The GUI scale factor: physical pixels per scaled pixel.
    #[must_use]
    pub fn scale(&self) -> u32 {
        self.scale
    }

    /// A physical pointer position as scaled pixels relative to the window's
    /// top-left corner (`mouseX - guiLeft`); negative outside the window.
    #[must_use]
    pub fn local(&self, point: (f32, f32)) -> (i32, i32) {
        let gui = |value: f32, scaled: i32, display: f32| (value * scaled as f32 / display).floor();
        let x = gui(point.0, self.scaled.0, self.display.0) as i32;
        let y = gui(point.1, self.scaled.1, self.display.1) as i32;
        (x - self.origin.0, y - self.origin.1)
    }

    /// A rectangle given in scaled pixels relative to the window, in physical
    /// pixels.
    #[must_use]
    pub fn rect(&self, x: i32, y: i32, width: i32, height: i32) -> Rect {
        let scale = self.scale as f32;
        Rect::new(
            (self.origin.0 + x) as f32 * scale,
            (self.origin.1 + y) as f32 * scale,
            width as f32 * scale,
            height as f32 * scale,
        )
    }

    /// The physical top-left of a point given relative to the window.
    #[must_use]
    pub fn position(&self, x: i32, y: i32) -> [f32; 2] {
        let rect = self.rect(x, y, 0, 0);
        [rect.x, rect.y]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_is_centred_with_integer_division() {
        // 854x480 at scale 2 is 427x240 scaled pixels.
        let layout = CreativeLayout::new(854, 480);
        assert_eq!(layout.scale(), 2);
        assert_eq!(
            layout.rect(0, 0, 195, 136),
            Rect::new(232.0, 104.0, 390.0, 272.0)
        );
    }

    #[test]
    fn an_odd_width_rounds_the_scaled_width_up() {
        // ceil(853 / 2) = 427, as ScaledResolution does.
        let layout = CreativeLayout::new(853, 480);
        assert_eq!(layout.rect(0, 0, 1, 1).x, 232.0);
    }

    #[test]
    fn a_pointer_is_converted_to_window_pixels() {
        let layout = CreativeLayout::new(854, 480);
        assert_eq!(layout.local((240.0, 110.0)), (4, 3));
        assert_eq!(layout.local((232.0, 104.0)), (0, 0));
        // One physical pixel left of the window is outside it.
        assert_eq!(layout.local((231.0, 104.0)).0, -1);
    }

    #[test]
    fn local_and_position_are_inverse_on_pixel_corners() {
        let layout = CreativeLayout::new(1280, 720);
        let [x, y] = layout.position(9, 18);
        assert_eq!(layout.local((x, y)), (9, 18));
    }
}

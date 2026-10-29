// SPDX-License-Identifier: MIT OR Apache-2.0

//! The hover tooltip box (MCP `GuiScreen.drawHoveringText`).
//!
//! [`tooltip`] draws already-resolved text rows in the vanilla box: a
//! `0xF0100010` background, a purple border that fades from `0x505000FF` to
//! `0x5028007F`, three GUI pixels of padding, a two pixel gap after the
//! title row and a box placed 12 GUI pixels right of and above the pointer
//! (flipped left, or pushed up, at the screen edges). The widget knows no
//! item types: the caller turns a stack into rows of text and colour.

use crate::draw::{Color, DrawList, TextStyle};
use crate::geom::Rect;
use crate::text::FontAtlas;
use crate::Ui;

/// Gap between the pointer and the tooltip, in GUI pixels.
const POINTER_OFFSET: f32 = 12.0;
/// Extra shift to the left when the box would leave the right edge, in GUI pixels.
const FLIP_OFFSET: f32 = 28.0;
/// Space between the text and the border, in GUI pixels.
const PADDING: f32 = 3.0;
/// Height of one text row, in GUI pixels.
const LINE_HEIGHT: f32 = 10.0;
/// Height of the glyphs, in GUI pixels.
const GLYPH_HEIGHT: f32 = 8.0;
/// Extra gap after the title row, in GUI pixels.
const TITLE_GAP: f32 = 2.0;
const BACKGROUND: Color = [0.0625, 0.0, 0.0625, 0.94];
const BORDER_TOP: Color = [0.3125, 0.0, 1.0, 0.31];
const BORDER_BOTTOM: Color = [0.157, 0.0, 0.498, 0.31];

/// One row of a tooltip: resolved text and its `0xRRGGBB` colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TooltipRow {
    /// The text to draw.
    pub text: String,
    /// The text colour as `0xRRGGBB`; the shadow is derived from it.
    pub rgb: u32,
}

/// The vanilla position of a tooltip box: right of and above the pointer,
/// flipped to the left of it when it would leave the screen and pushed up
/// when it would leave the bottom. All values in physical pixels.
fn tooltip_origin(pointer: [f32; 2], size: [f32; 2], viewport: Rect, scale: f32) -> [f32; 2] {
    let mut x = pointer[0] + POINTER_OFFSET * scale;
    if x + size[0] > viewport.x + viewport.width {
        x -= (FLIP_OFFSET * scale) + size[0];
    }
    let mut y = pointer[1] - POINTER_OFFSET * scale;
    let bottom = viewport.y + viewport.height;
    if y + size[1] + 6.0 * scale > bottom {
        y = bottom - size[1] - 6.0 * scale;
    }
    [x, y]
}

/// The sRGB to linear transfer function, the inverse of what the sRGB render
/// target applies on write.
///
/// The UI pass hands vertex colours to an sRGB target as they are, so a
/// vanilla colour (written for a plain 8-bit framebuffer) has to be linearised
/// first or it comes out too light: the tooltip background 0x100010 showed as
/// a bright purple.
fn linear(channel: f32) -> f32 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

/// A vanilla colour (straight sRGB channels and alpha) as the UI pass wants it.
fn vanilla(color: Color) -> Color {
    [
        linear(color[0]),
        linear(color[1]),
        linear(color[2]),
        color[3],
    ]
}

/// The text colour as straight RGBA.
fn rgba(rgb: u32, factor: f32) -> Color {
    let channel = |shift: u32| ((rgb >> shift) & 0xFF) as f32 / 255.0 * factor;
    vanilla([channel(16), channel(8), channel(0), 1.0])
}

/// A vanilla text colour (`0xRRGGBB`) as the UI pass wants it.
pub fn text_color(rgb: u32) -> Color {
    rgba(rgb, 1.0)
}

/// Draws `rows` in a vanilla tooltip box next to `pointer`.
///
/// `scale` is the GUI scale factor and `viewport` bounds the box (all in
/// physical pixels). The first row is the title: it is followed by an extra
/// gap. Does nothing for an empty slice.
pub fn tooltip(
    ui: &mut Ui,
    atlas: &FontAtlas,
    viewport: Rect,
    scale: f32,
    pointer: [f32; 2],
    rows: &[TooltipRow],
) {
    if rows.is_empty() {
        return;
    }
    let width = rows
        .iter()
        .map(|row| atlas.measure(&row.text, scale))
        .fold(0.0, f32::max);
    let size = [width, tooltip_height(rows.len()) * scale];
    let [x, y] = tooltip_origin(pointer, size, viewport, scale);
    draw_box(ui, [x, y], size, scale);

    let mut row_y = y;
    for (index, row) in rows.iter().enumerate() {
        let shadow = TextStyle::new(scale, rgba(row.rgb, 0.25));
        let style = TextStyle::new(scale, rgba(row.rgb, 1.0));
        ui.draw()
            .text([x + scale, row_y + scale], row.text.clone(), shadow);
        ui.draw().text([x, row_y], row.text.clone(), style);
        row_y += if index == 0 {
            LINE_HEIGHT + TITLE_GAP
        } else {
            LINE_HEIGHT
        } * scale;
    }
}

/// The height of the text block in GUI pixels (`drawHoveringText`: 8 for one
/// row, `8 + 2 + 10 * (rows - 1)` for more).
fn tooltip_height(rows: usize) -> f32 {
    if rows > 1 {
        GLYPH_HEIGHT + TITLE_GAP + (rows - 1) as f32 * LINE_HEIGHT
    } else {
        GLYPH_HEIGHT
    }
}

/// Draws a vertical gradient as one strip per GUI pixel row.
///
/// The colours are interpolated as vanilla does, on the sRGB channels, and
/// each strip is then linearised for the render target.
fn gradient_column(
    draw: &mut DrawList,
    origin: [f32; 2],
    size: [f32; 2],
    top: Color,
    bottom: Color,
) {
    let rows = (size[1] / size[0]).round().max(1.0) as u32;
    let strip = size[1] / rows as f32;
    for row in 0..rows {
        let t = (row as f32 + 0.5) / rows as f32;
        let color = std::array::from_fn(|i| top[i] + (bottom[i] - top[i]) * t);
        draw.rect(
            Rect::new(origin[0], origin[1] + strip * row as f32, size[0], strip),
            vanilla(color),
        );
    }
}

/// The dark background with the purple border around a text block.
fn draw_box(ui: &mut Ui, origin: [f32; 2], size: [f32; 2], scale: f32) {
    let pad = PADDING * scale;
    let (x, y, w, h) = (origin[0], origin[1], size[0], size[1]);
    let (background, border_top, border_bottom) = (
        vanilla(BACKGROUND),
        vanilla(BORDER_TOP),
        vanilla(BORDER_BOTTOM),
    );
    let draw = ui.draw();
    // Background: the body plus a one-pixel rim on every side.
    draw.rect(
        Rect::new(
            x - pad - scale,
            y - pad,
            w + 2.0 * pad + 2.0 * scale,
            h + 2.0 * pad,
        ),
        background,
    );
    draw.rect(
        Rect::new(
            x - pad,
            y - pad - scale,
            w + 2.0 * pad,
            h + 2.0 * pad + 2.0 * scale,
        ),
        background,
    );
    // Border, inside the body: the sides fade from the top colour to the
    // bottom one (`GuiScreen.drawHoveringText` draws them with
    // `drawGradientRect`).
    let inner_h = h + 2.0 * pad - 2.0 * scale;
    let top = y - pad + scale;
    gradient_column(
        draw,
        [x - pad, top],
        [scale, inner_h],
        BORDER_TOP,
        BORDER_BOTTOM,
    );
    gradient_column(
        draw,
        [x + w + pad - scale, top],
        [scale, inner_h],
        BORDER_TOP,
        BORDER_BOTTOM,
    );
    draw.rect(
        Rect::new(x - pad, y - pad, w + 2.0 * pad, scale),
        border_top,
    );
    draw.rect(
        Rect::new(x - pad, y + h + pad - scale, w + 2.0 * pad, scale),
        border_bottom,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DrawCommand;

    #[test]
    fn vanilla_colours_are_linearised_for_the_srgb_target() {
        let background = vanilla(BACKGROUND);
        // 0x10 / 255 in sRGB is about 0.005 linear, not 0.0625.
        assert!((background[0] - 0.0052).abs() < 0.001);
        assert_eq!(background[3], 0.94);
        assert_eq!(rgba(0xFF_FF_FF, 1.0), [1.0, 1.0, 1.0, 1.0]);
        assert!(rgba(0x55_55_FF, 1.0)[0] < 0.1);
    }

    #[test]
    fn a_gradient_column_has_one_strip_per_gui_pixel_and_ends_on_the_borders() {
        let mut draw = DrawList::new();
        // Scale 2: a 2 px wide column, 20 px tall = 10 GUI pixel rows.
        gradient_column(
            &mut draw,
            [0.0, 0.0],
            [2.0, 20.0],
            BORDER_TOP,
            BORDER_BOTTOM,
        );
        assert_eq!(draw.len(), 10);
        let colors: Vec<Color> = draw
            .commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Rect { color, .. } => Some(*color),
                _ => None,
            })
            .collect();
        assert!(colors[0][2] > colors[9][2], "blue fades towards the bottom");
        assert!((colors[0][2] - vanilla(BORDER_TOP)[2]).abs() < 0.1);
    }

    #[test]
    fn the_box_flips_left_and_up_at_the_screen_edges() {
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        assert_eq!(
            tooltip_origin([100.0, 100.0], [50.0, 20.0], viewport, 2.0),
            [124.0, 76.0]
        );
        // Right edge: moved to the left of the pointer.
        assert_eq!(
            tooltip_origin([790.0, 100.0], [50.0, 20.0], viewport, 2.0)[0],
            790.0 - 56.0 - 50.0 + 24.0
        );
        // Bottom edge: pushed up.
        assert_eq!(
            tooltip_origin([100.0, 598.0], [50.0, 20.0], viewport, 2.0)[1],
            600.0 - 20.0 - 12.0
        );
    }

    #[test]
    fn the_text_block_is_taller_after_the_title_gap() {
        assert_eq!(tooltip_height(1), 8.0);
        assert_eq!(tooltip_height(2), 20.0);
        assert_eq!(tooltip_height(4), 40.0);
    }
}

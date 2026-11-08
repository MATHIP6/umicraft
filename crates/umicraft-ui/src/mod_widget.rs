// SPDX-License-Identifier: MIT OR Apache-2.0

//! The generic mod-widget layer of the HUD (ADR-0065, milestones M3 and M6).
//!
//! A built-in mod describes its widget as a few coloured text lines; the
//! client resolves the anchor and offset to a pixel position and hands this
//! layer a [`ModWidgetView`]. The renderer therefore knows nothing about
//! anchors or mods: it draws a translucent panel and the stacked lines, each
//! with the vanilla text shadow (`FontRenderer.drawStringWithShadow`: a
//! quarter-brightness copy one unit down and right).
//!
//! Sizes are in GUI units (the unit of `ScaledResolution`); the caller picks
//! the position from [`measure`] and `HudPlacement::rect`.
//!
//! The layer lives in this crate, not in the renderer, so the mods menu
//! (`menus::mods::hud_layer`) previews the widgets with exactly the code the HUD uses.

use crate::draw::{Color, DrawList, TextStyle};
use crate::geom::Rect;
use crate::text::FontAtlas;

/// The row pitch, `FontRenderer.FONT_HEIGHT`.
const LINE_HEIGHT: f32 = 9.0;
/// The margin between the panel edge and the text, in GUI units.
const PADDING: f32 = 2.0;
/// The corner radius of the panel, in GUI units.
const RADIUS: f32 = 2.0;
/// The panel colour: black at about 35 % opacity, like the chat band.
const PANEL: Color = [0.0, 0.0, 0.0, 0.35];

/// One line of a mod widget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModLine {
    /// The text, drawn as is (no `\u{a7}` codes are interpreted).
    pub text: String,
    /// The colour as `0xRRGGBB`.
    pub color: u32,
}

impl ModLine {
    /// Creates a line of `text` in `color` (`0xRRGGBB`).
    #[must_use]
    pub fn new(text: impl Into<String>, color: u32) -> Self {
        Self {
            text: text.into(),
            color,
        }
    }
}

/// A mod widget whose position is already resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct ModWidgetView {
    /// The lines, top to bottom; an empty list draws nothing.
    pub lines: Vec<ModLine>,
    /// The top-left corner of the panel in GUI units.
    pub position: [f32; 2],
    /// The widget scale, `1.0` for the vanilla size.
    pub scale: f32,
}

/// The size of the panel for `lines` at `scale`, in GUI units.
///
/// The result already includes `scale`, so it is what the placement needs to
/// resolve a rectangle against the screen; pass `1.0` for the unscaled size.
/// An empty list measures `(0.0, 0.0)`.
#[must_use]
pub fn measure(font: &FontAtlas, lines: &[ModLine], scale: f32) -> (f32, f32) {
    if lines.is_empty() {
        return (0.0, 0.0);
    }
    let widest = lines
        .iter()
        .map(|line| font.measure(&line.text, 1.0))
        .fold(0.0_f32, f32::max);
    let width = widest + 2.0 * PADDING;
    let height = LINE_HEIGHT * lines.len() as f32 + 2.0 * PADDING - 1.0;
    (width * scale, height * scale)
}

/// Draws every widget of `widgets` into `list`; `sf` is the GUI scale factor
/// (pixels per GUI unit).
pub fn draw_mod_widgets(list: &mut DrawList, widgets: &[ModWidgetView], sf: f32, font: &FontAtlas) {
    for widget in widgets {
        draw_widget(list, widget, sf, font);
    }
}

/// Draws the panel, then the lines with their shadows.
fn draw_widget(list: &mut DrawList, widget: &ModWidgetView, sf: f32, font: &FontAtlas) {
    if widget.lines.is_empty() || widget.scale <= 0.0 {
        return;
    }
    let (width, height) = measure(font, &widget.lines, widget.scale);
    let [x, y] = widget.position;
    list.rounded_rect(
        Rect::new(x * sf, y * sf, width * sf, height * sf),
        RADIUS * widget.scale * sf,
        PANEL,
    );
    let unit = widget.scale * sf;
    let left = (x + PADDING * widget.scale) * sf;
    let top = (y + PADDING * widget.scale) * sf;
    for (row, line) in widget.lines.iter().enumerate() {
        let at = [left, top + LINE_HEIGHT * row as f32 * unit];
        let ink = rgb(line.color);
        list.text(
            [at[0] + unit, at[1] + unit],
            &line.text,
            TextStyle::new(unit, shadow_of(ink)),
        );
        list.text(at, &line.text, TextStyle::new(unit, ink));
    }
}

/// Converts `0xRRGGBB` to an opaque colour.
fn rgb(value: u32) -> Color {
    let channel = |shift: u32| ((value >> shift) & 0xFF) as f32 / 255.0;
    [channel(16), channel(8), channel(0), 1.0]
}

/// The shadow colour: a quarter of the brightness.
fn shadow_of([r, g, b, a]: Color) -> Color {
    [r / 4.0, g / 4.0, b / 4.0, a]
}

#[cfg(test)]
mod tests {
    use crate::draw::DrawCommand;

    use super::*;

    fn widget(scale: f32) -> ModWidgetView {
        ModWidgetView {
            lines: vec![
                ModLine::new("FPS: 60", 0xFF8000),
                ModLine::new("x", 0xFFFFFF),
            ],
            position: [10.0, 20.0],
            scale,
        }
    }

    #[test]
    fn empty_widgets_measure_zero_and_draw_nothing() {
        let font = FontAtlas::fallback();
        assert_eq!(measure(&font, &[], 2.0), (0.0, 0.0));
        let mut list = DrawList::new();
        let empty = ModWidgetView {
            lines: Vec::new(),
            ..widget(1.0)
        };
        draw_mod_widgets(&mut list, &[empty], 2.0, &font);
        assert!(list.is_empty());
    }

    #[test]
    fn size_follows_the_widest_line_and_the_row_count() {
        let font = FontAtlas::fallback();
        let w = widget(1.0);
        let (width, height) = measure(&font, &w.lines, 1.0);
        assert_eq!(width, font.measure("FPS: 60", 1.0) + 2.0 * PADDING);
        assert_eq!(height, 2.0 * LINE_HEIGHT + 2.0 * PADDING - 1.0);
        let (sw, sh) = measure(&font, &w.lines, 1.5);
        assert_eq!((sw, sh), (width * 1.5, height * 1.5));
    }

    #[test]
    fn a_widget_emits_a_panel_then_a_shadow_and_text_per_line() {
        let font = FontAtlas::fallback();
        let mut list = DrawList::new();
        draw_mod_widgets(&mut list, &[widget(2.0)], 3.0, &font);
        let commands = list.commands();
        assert_eq!(commands.len(), 1 + 2 * 2);
        let (width, height) = measure(&font, &widget(2.0).lines, 2.0);
        match &commands[0] {
            DrawCommand::RoundedRect { rect, .. } => {
                assert_eq!(*rect, Rect::new(30.0, 60.0, width * 3.0, height * 3.0));
            }
            other => panic!("expected the panel, got {other:?}"),
        }
        let (
            DrawCommand::Text {
                position: shadow,
                style: s,
                ..
            },
            DrawCommand::Text {
                position: text,
                style: t,
                text: body,
            },
        ) = (&commands[1], &commands[2])
        else {
            panic!("expected shadow then text");
        };
        assert_eq!(body, "FPS: 60");
        assert_eq!(t.scale, 6.0);
        assert_eq!([shadow[0] - text[0], shadow[1] - text[1]], [6.0, 6.0]);
        assert_eq!(text[0], (10.0 + 4.0) * 3.0);
        assert_eq!(t.color, [1.0, 128.0 / 255.0, 0.0, 1.0]);
        assert_eq!(s.color[0], t.color[0] / 4.0);
        let DrawCommand::Text {
            position: second, ..
        } = &commands[4]
        else {
            panic!("expected the second line");
        };
        assert_eq!(second[1] - text[1], LINE_HEIGHT * 6.0);
    }
}

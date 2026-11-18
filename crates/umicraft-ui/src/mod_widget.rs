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
/// The row pitch of a line with a picture or a detail, and the width of the picture.
const ICON_ROW: f32 = 18.0;
/// The gap between a picture and its text.
const ICON_GAP: f32 = 2.0;
/// The panel colour: black at about 35 % opacity, like the chat band.
const PANEL: Color = [0.0, 0.0, 0.0, 0.35];
/// A box that is not lit.
const CELL_IDLE: Color = [0.0, 0.0, 0.0, 0.5];
/// A lit box (its key is held).
const CELL_LIT: Color = [1.0, 1.0, 1.0, 0.7];
/// The text of a lit box.
const CELL_LIT_TEXT: u32 = 0x00_0000;

/// The shadow of the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShadowStyle {
    /// A quarter of the brightness of the text, `drawStringWithShadow`.
    #[default]
    Vanilla,
    /// One colour, `0xRRGGBB`.
    Color(u32),
    /// No shadow.
    Off,
}

/// How a widget looks around its text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WidgetStyle {
    /// The panel behind the lines (straight-alpha RGBA), or `None` for none.
    pub background: Option<Color>,
    /// The corner radius of the panel, in GUI units.
    pub radius: f32,
    /// The margin between the panel edge and the text, in GUI units.
    pub padding: f32,
    /// The shadow of the text.
    pub shadow: ShadowStyle,
}

impl Default for WidgetStyle {
    /// A black panel at 35 % with 2 units of margin and corner, vanilla shadow.
    fn default() -> Self {
        Self {
            background: Some(PANEL),
            radius: 2.0,
            padding: 2.0,
            shadow: ShadowStyle::Vanilla,
        }
    }
}

/// The picture before a line, already resolved by the client: the widget layer
/// never knows items or sprite sheets, only what to draw.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ModIcon {
    /// No picture.
    #[default]
    None,
    /// The item icon registered under `key` (the opaque key of the renderer).
    Item {
        /// The icon key.
        key: u64,
        /// Whether the enchantment glint is drawn over it.
        glint: bool,
    },
    /// A rectangle of the GUI atlas, as `[u_min, v_min, u_max, v_max]`.
    Image {
        /// The source rectangle in atlas coordinates.
        src: [f32; 4],
    },
}

/// One line of a mod widget.
#[derive(Debug, Clone, PartialEq)]
pub struct ModLine {
    /// The text, drawn as is (no `\u{a7}` codes are interpreted).
    pub text: String,
    /// The colour as `0xRRGGBB`.
    pub color: u32,
    /// The picture before the text.
    pub icon: ModIcon,
    /// A second text under the first, empty for none.
    pub detail: String,
    /// The colour of the detail as `0xRRGGBB`.
    pub detail_color: u32,
}

impl ModLine {
    /// Creates a line of `text` in `color` (`0xRRGGBB`), without picture or detail.
    #[must_use]
    pub fn new(text: impl Into<String>, color: u32) -> Self {
        Self {
            text: text.into(),
            color,
            icon: ModIcon::None,
            detail: String::new(),
            detail_color: color,
        }
    }

    /// Whether the line is as tall as an icon: it has a picture or a detail.
    fn is_tall(&self) -> bool {
        self.icon != ModIcon::None || !self.detail.is_empty()
    }

    /// The height of the row, in GUI units.
    fn height(&self) -> f32 {
        if self.is_tall() {
            ICON_ROW
        } else {
            LINE_HEIGHT
        }
    }

    /// The width of the row, in GUI units.
    fn width(&self, font: &FontAtlas) -> f32 {
        let text = font
            .measure(&self.text, 1.0)
            .max(font.measure(&self.detail, 1.0));
        if self.icon == ModIcon::None {
            text
        } else {
            ICON_ROW + ICON_GAP + text
        }
    }
}

/// A box of a widget at a fixed place, such as a key of the keystrokes.
#[derive(Debug, Clone, PartialEq)]
pub struct ModCell {
    /// `[x, y, width, height]` in GUI units from the top-left of the content.
    pub rect: [f32; 4],
    /// Whether the box is lit.
    pub pressed: bool,
    /// The colour of the text as `0xRRGGBB`, while the box is not lit.
    pub color: u32,
    /// The text, centred in the box.
    pub text: String,
}

/// A text centered on a point of a widget, with its own opacity and size.
#[derive(Debug, Clone, PartialEq)]
pub struct ModLabel {
    /// `[x, y]` in GUI units from the top-left of the content: the center of
    /// the text and its top edge.
    pub at: [f32; 2],
    /// The text.
    pub text: String,
    /// The colour as `0xRRGGBB`.
    pub color: u32,
    /// Opacity, `0.0` to `1.0`; the shadow fades with it.
    pub alpha: f32,
    /// Size relative to the normal text.
    pub scale: f32,
}

/// A small filled rectangle of a widget.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModTick {
    /// `[x, y, width, height]` in GUI units from the top-left of the content.
    pub rect: [f32; 4],
    /// The colour as `0xRRGGBB`.
    pub color: u32,
    /// Opacity, `0.0` to `1.0`.
    pub alpha: f32,
}

/// A mod widget whose position is already resolved.
///
/// A widget holds stacked lines, fixed boxes or freely placed labels and ticks;
/// a mod uses one kind.
#[derive(Debug, Clone, PartialEq)]
pub struct ModWidgetView {
    /// The lines, top to bottom.
    pub lines: Vec<ModLine>,
    /// The boxes, placed by their own rectangle.
    pub cells: Vec<ModCell>,
    /// The free texts, placed by their center.
    pub labels: Vec<ModLabel>,
    /// The small rectangles.
    pub ticks: Vec<ModTick>,
    /// The size `[width, height]` in GUI units the content keeps at least.
    pub reserve: [f32; 2],
    /// Whether the rows are mirrored (text, then picture, against the right
    /// edge), which keeps the pictures at the edge of the screen on its right half.
    pub mirrored: bool,
    /// The top-left corner of the panel in GUI units.
    pub position: [f32; 2],
    /// The widget scale, `1.0` for the vanilla size.
    pub scale: f32,
    /// The look of the panel and of the text shadow.
    pub style: WidgetStyle,
}

impl Default for ModWidgetView {
    /// An empty widget at the origin, at the vanilla scale.
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            cells: Vec::new(),
            labels: Vec::new(),
            ticks: Vec::new(),
            reserve: [0.0; 2],
            mirrored: false,
            position: [0.0; 2],
            scale: 1.0,
            style: WidgetStyle::default(),
        }
    }
}

impl ModWidgetView {
    /// Whether there is nothing to draw.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
            && self.cells.is_empty()
            && self.labels.is_empty()
            && self.ticks.is_empty()
            && self.reserve == [0.0; 2]
    }

    /// The size of the panel at its scale, in GUI units; see [`measure`].
    #[must_use]
    pub fn size(&self, font: &FontAtlas) -> (f32, f32) {
        measure_view(font, self)
    }
}

/// The size of the content (without the padding) of a widget, in GUI units at
/// scale 1: the largest of its lines, boxes, labels, ticks and reserved size.
fn content_size(font: &FontAtlas, widget: &ModWidgetView) -> (f32, f32) {
    let lines = &widget.lines;
    let rows = lines.iter().map(|line| line.height()).sum::<f32>();
    // The last plain row has no gap below its glyphs; a tall row has no gap either.
    let trailing = lines
        .last()
        .map_or(0.0, |line| if line.is_tall() { 0.0 } else { 1.0 });
    let lines_width = lines
        .iter()
        .map(|line| line.width(font))
        .fold(0.0_f32, f32::max);
    let (width, height) =
        widget
            .cells
            .iter()
            .fold((lines_width, (rows - trailing).max(0.0)), |(w, h), cell| {
                let [x, y, width, height] = cell.rect;
                (w.max(x + width), h.max(y + height))
            });
    let (width, height) = widget.ticks.iter().fold((width, height), |(w, h), tick| {
        let [x, y, width, height] = tick.rect;
        (w.max(x + width), h.max(y + height))
    });
    let (width, height) = widget.labels.iter().fold((width, height), |(w, h), label| {
        let text_width = font.measure(&label.text, 1.0) * label.scale;
        (
            w.max(label.at[0] + text_width / 2.0),
            h.max(label.at[1] + LINE_HEIGHT * label.scale),
        )
    });
    (width.max(widget.reserve[0]), height.max(widget.reserve[1]))
}

/// The size of the panel for `lines` at `scale`, in GUI units.
///
/// The result already includes `scale`, so it is what the placement needs to
/// resolve a rectangle against the screen; pass `1.0` for the unscaled size.
/// An empty list measures `(0.0, 0.0)`.
#[must_use]
pub fn measure(font: &FontAtlas, lines: &[ModLine], scale: f32, style: &WidgetStyle) -> (f32, f32) {
    measure_view(
        font,
        &ModWidgetView {
            lines: lines.to_vec(),
            scale,
            style: *style,
            ..ModWidgetView::default()
        },
    )
}

/// The size of the panel of `widget` at its scale and with its style, in GUI
/// units, `(0.0, 0.0)` for an empty widget.
#[must_use]
pub fn measure_view(font: &FontAtlas, widget: &ModWidgetView) -> (f32, f32) {
    if widget.is_empty() {
        return (0.0, 0.0);
    }
    let (width, height) = content_size(font, widget);
    let padding = 2.0 * widget.style.padding;
    (
        (width + padding) * widget.scale,
        (height + padding) * widget.scale,
    )
}

/// Draws every widget of `widgets` into `list`; `sf` is the GUI scale factor
/// (pixels per GUI unit).
pub fn draw_mod_widgets(list: &mut DrawList, widgets: &[ModWidgetView], sf: f32, font: &FontAtlas) {
    for widget in widgets {
        draw_widget(list, widget, sf, font);
    }
}

/// Where a widget is drawn: the pixel of its content's top-left corner and the
/// size of a GUI unit in pixels.
#[derive(Clone, Copy)]
struct Origin {
    left: f32,
    top: f32,
    unit: f32,
}

impl Origin {
    /// The pixel position of a point given in GUI units from the content corner.
    fn at(self, x: f32, y: f32) -> [f32; 2] {
        [self.left + x * self.unit, self.top + y * self.unit]
    }

    fn rect(self, x: f32, y: f32, width: f32, height: f32) -> Rect {
        let [left, top] = self.at(x, y);
        Rect::new(left, top, width * self.unit, height * self.unit)
    }
}

/// Draws the panel, then the boxes and the lines.
fn draw_widget(list: &mut DrawList, widget: &ModWidgetView, sf: f32, font: &FontAtlas) {
    if widget.is_empty() || widget.scale <= 0.0 {
        return;
    }
    let style = &widget.style;
    let (width, height) = measure_view(font, widget);
    let [x, y] = widget.position;
    if let Some(background) = style.background.filter(|colour| colour[3] > 0.0) {
        let panel = Rect::new(x * sf, y * sf, width * sf, height * sf);
        fill(list, panel, style.radius * widget.scale * sf, background);
    }
    let unit = widget.scale * sf;
    let origin = Origin {
        left: (x + style.padding * widget.scale) * sf,
        top: (y + style.padding * widget.scale) * sf,
        unit,
    };
    let inner_width = width / widget.scale - 2.0 * style.padding;
    for cell in &widget.cells {
        draw_cell(list, cell, origin, style, font);
    }
    for tick in &widget.ticks {
        let [x, y, width, height] = tick.rect;
        let mut colour = rgb(tick.color);
        colour[3] = tick.alpha;
        list.rect(origin.rect(x, y, width, height), colour);
    }
    for label in &widget.labels {
        let text_width = font.measure(&label.text, 1.0) * label.scale;
        let at = origin.at(label.at[0] - text_width / 2.0, label.at[1]);
        let mut ink = rgb(label.color);
        ink[3] = label.alpha;
        draw_text(
            list,
            at,
            &label.text,
            ink,
            style.shadow,
            origin.unit * label.scale,
        );
    }
    let mut row_top = 0.0;
    for line in &widget.lines {
        draw_line(
            list,
            line,
            (row_top, inner_width),
            (origin, widget.mirrored),
            style,
            font,
        );
        row_top += line.height();
    }
}

/// Fills `rect`, rounded when `radius` is positive.
fn fill(list: &mut DrawList, rect: Rect, radius: f32, colour: Color) {
    if radius > 0.0 {
        list.rounded_rect(rect, radius, colour);
    } else {
        list.rect(rect, colour);
    }
}

/// Draws one box: lit boxes are light with dark text, the others dark.
fn draw_cell(
    list: &mut DrawList,
    cell: &ModCell,
    origin: Origin,
    style: &WidgetStyle,
    font: &FontAtlas,
) {
    let [x, y, width, height] = cell.rect;
    let (back, ink) = if cell.pressed {
        (CELL_LIT, rgb(CELL_LIT_TEXT))
    } else {
        (CELL_IDLE, rgb(cell.color))
    };
    fill(
        list,
        origin.rect(x, y, width, height),
        style.radius * origin.unit,
        back,
    );
    let text_width = font.measure(&cell.text, 1.0);
    let at = origin.at(
        x + ((width - text_width) / 2.0).floor(),
        y + ((height - LINE_HEIGHT) / 2.0).floor(),
    );
    // A shadow under dark text on a light box only blurs it.
    let shadow = if cell.pressed {
        ShadowStyle::Off
    } else {
        style.shadow
    };
    draw_text(list, at, &cell.text, ink, shadow, origin.unit);
}

/// Draws one row at `row_top` (GUI units): the picture, the text and the detail.
fn draw_line(
    list: &mut DrawList,
    line: &ModLine,
    (row_top, inner_width): (f32, f32),
    (origin, mirrored): (Origin, bool),
    style: &WidgetStyle,
    font: &FontAtlas,
) {
    let has_icon = line.icon != ModIcon::None;
    let text_width = font
        .measure(&line.text, 1.0)
        .max(font.measure(&line.detail, 1.0));
    let icon_x = if mirrored {
        inner_width - ICON_ROW
    } else {
        0.0
    };
    let text_left = match (mirrored, has_icon) {
        (false, false) => 0.0,
        (false, true) => ICON_ROW + ICON_GAP,
        (true, false) => inner_width - text_width,
        (true, true) => inner_width - ICON_ROW - ICON_GAP - text_width,
    };
    match line.icon {
        ModIcon::None => {}
        ModIcon::Item { key, glint } => {
            list.item(
                key,
                origin.rect(icon_x + 1.0, row_top + 1.0, ICON_ROW - 2.0, ICON_ROW - 2.0),
                glint,
            );
        }
        ModIcon::Image { src } => {
            list.image(
                src,
                origin.rect(icon_x, row_top, ICON_ROW, ICON_ROW),
                [1.0; 4],
            );
        }
    }
    // Mirrored text is right-aligned against the picture.
    let aligned = |text: &str| {
        let own = font.measure(text, 1.0);
        if mirrored {
            text_left + text_width - own
        } else {
            text_left
        }
    };
    let (name_y, detail_y) = if line.detail.is_empty() {
        (((line.height() - LINE_HEIGHT) / 2.0).floor(), 0.0)
    } else {
        (0.0, LINE_HEIGHT)
    };
    let ink = rgb(line.color);
    draw_text(
        list,
        origin.at(aligned(&line.text), row_top + name_y),
        &line.text,
        ink,
        style.shadow,
        origin.unit,
    );
    if !line.detail.is_empty() {
        let ink = rgb(line.detail_color);
        draw_text(
            list,
            origin.at(aligned(&line.detail), row_top + detail_y),
            &line.detail,
            ink,
            style.shadow,
            origin.unit,
        );
    }
}

/// Draws `text` at `at` with its shadow.
fn draw_text(
    list: &mut DrawList,
    at: [f32; 2],
    text: &str,
    ink: Color,
    shadow: ShadowStyle,
    unit: f32,
) {
    let shadow = match shadow {
        ShadowStyle::Vanilla => Some(shadow_of(ink)),
        ShadowStyle::Color(colour) => {
            Some([rgb(colour)[0], rgb(colour)[1], rgb(colour)[2], ink[3]])
        }
        ShadowStyle::Off => None,
    };
    if let Some(shadow) = shadow {
        list.text(
            [at[0] + unit, at[1] + unit],
            text,
            TextStyle::new(unit, shadow),
        );
    }
    list.text(at, text, TextStyle::new(unit, ink));
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
            ..ModWidgetView::default()
        }
    }

    #[test]
    fn empty_widgets_measure_zero_and_draw_nothing() {
        let font = FontAtlas::fallback();
        assert_eq!(
            measure(&font, &[], 2.0, &WidgetStyle::default()),
            (0.0, 0.0)
        );
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
        let style = WidgetStyle::default();
        let (width, height) = measure(&font, &w.lines, 1.0, &style);
        assert_eq!(width, font.measure("FPS: 60", 1.0) + 2.0 * style.padding);
        assert_eq!(height, 2.0 * LINE_HEIGHT + 2.0 * style.padding - 1.0);
        let (sw, sh) = measure(&font, &w.lines, 1.5, &style);
        assert_eq!((sw, sh), (width * 1.5, height * 1.5));
    }

    #[test]
    fn a_widget_emits_a_panel_then_a_shadow_and_text_per_line() {
        let font = FontAtlas::fallback();
        let mut list = DrawList::new();
        draw_mod_widgets(&mut list, &[widget(2.0)], 3.0, &font);
        let commands = list.commands();
        assert_eq!(commands.len(), 1 + 2 * 2);
        let (width, height) = measure(&font, &widget(2.0).lines, 2.0, &WidgetStyle::default());
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

    #[test]
    fn the_padding_changes_the_size_and_where_the_text_starts() {
        let font = FontAtlas::fallback();
        let mut w = widget(1.0);
        w.style.padding = 5.0;
        let (width, _) = measure(&font, &w.lines, 1.0, &w.style);
        assert_eq!(width, font.measure("FPS: 60", 1.0) + 10.0);
        let mut list = DrawList::new();
        draw_mod_widgets(&mut list, &[w], 1.0, &font);
        let DrawCommand::Text { position, .. } = &list.commands()[2] else {
            panic!("expected the text");
        };
        assert_eq!(position[0], 10.0 + 5.0);
    }

    #[test]
    fn no_background_draws_no_panel_and_no_shadow_draws_no_copy() {
        let font = FontAtlas::fallback();
        let mut w = widget(1.0);
        w.style.background = None;
        w.style.shadow = ShadowStyle::Off;
        let mut list = DrawList::new();
        draw_mod_widgets(&mut list, &[w], 1.0, &font);
        assert_eq!(list.commands().len(), 2, "one text per line");
        assert!(list
            .commands()
            .iter()
            .all(|command| matches!(command, DrawCommand::Text { .. })));
    }

    #[test]
    fn a_custom_shadow_has_its_own_colour_and_no_radius_gives_square_corners() {
        let font = FontAtlas::fallback();
        let mut w = widget(1.0);
        w.style.shadow = ShadowStyle::Color(0x00_00FF);
        w.style.radius = 0.0;
        let mut list = DrawList::new();
        draw_mod_widgets(&mut list, &[w], 1.0, &font);
        assert!(matches!(list.commands()[0], DrawCommand::Rect { .. }));
        let DrawCommand::Text { style, .. } = &list.commands()[1] else {
            panic!("expected the shadow");
        };
        assert_eq!(style.color, [0.0, 0.0, 1.0, 1.0]);
    }

    fn icon_line(text: &str, detail: &str) -> ModLine {
        ModLine {
            icon: ModIcon::Item {
                key: 7,
                glint: false,
            },
            detail: detail.into(),
            ..ModLine::new(text, 0xFFFFFF)
        }
    }

    fn only(lines: Vec<ModLine>, cells: Vec<ModCell>, mirrored: bool) -> ModWidgetView {
        ModWidgetView {
            lines,
            cells,
            mirrored,
            style: WidgetStyle {
                background: None,
                padding: 0.0,
                ..WidgetStyle::default()
            },
            ..ModWidgetView::default()
        }
    }

    #[test]
    fn a_row_with_a_picture_is_as_tall_as_the_picture_and_wider_by_it() {
        let font = FontAtlas::fallback();
        let style = WidgetStyle {
            padding: 0.0,
            ..WidgetStyle::default()
        };
        let plain = [ModLine::new("350", 0xFFFFFF)];
        let tall = [icon_line("350", "")];
        let (plain_width, _) = measure(&font, &plain, 1.0, &style);
        let (width, height) = measure(&font, &tall, 1.0, &style);
        assert_eq!(height, ICON_ROW);
        assert_eq!(width, plain_width + ICON_ROW + ICON_GAP);
    }

    #[test]
    fn a_detail_goes_under_the_text() {
        let font = FontAtlas::fallback();
        let mut list = DrawList::new();
        let view = only(vec![icon_line("Speed II", "1:23")], Vec::new(), false);
        draw_mod_widgets(&mut list, &[view], 1.0, &font);
        let ys: Vec<f32> = list
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { position, text, .. }
                    if text == "1:23" || text == "Speed II" =>
                {
                    Some(position[1])
                }
                _ => None,
            })
            .collect();
        // Shadow and text of each: the detail starts a row of text lower.
        assert_eq!(ys, [1.0, 0.0, 10.0, 9.0]);
    }

    #[test]
    fn the_picture_is_an_item_or_an_atlas_image() {
        let font = FontAtlas::fallback();
        let mut list = DrawList::new();
        let image = ModLine {
            icon: ModIcon::Image {
                src: [0.1, 0.2, 0.3, 0.4],
            },
            ..ModLine::new("x", 0xFFFFFF)
        };
        let view = only(vec![icon_line("a", ""), image], Vec::new(), false);
        draw_mod_widgets(&mut list, &[view], 2.0, &font);
        let commands = list.commands();
        assert!(commands.iter().any(|c| matches!(
            c,
            DrawCommand::Item { key: 7, dst, .. } if *dst == Rect::new(2.0, 2.0, 32.0, 32.0)
        )));
        assert!(commands.iter().any(|c| matches!(
            c,
            DrawCommand::Image { src, dst, .. }
                if *src == [0.1, 0.2, 0.3, 0.4] && *dst == Rect::new(0.0, 36.0, 36.0, 36.0)
        )));
    }

    #[test]
    fn a_mirrored_row_puts_the_picture_at_the_right_edge() {
        let font = FontAtlas::fallback();
        let mut list = DrawList::new();
        let lines = vec![icon_line("350", ""), icon_line("1500", "")];
        let (width, _) = measure(
            &font,
            &lines,
            1.0,
            &only(Vec::new(), Vec::new(), true).style,
        );
        draw_mod_widgets(&mut list, &[only(lines, Vec::new(), true)], 1.0, &font);
        for command in list.commands() {
            if let DrawCommand::Item { dst, .. } = command {
                assert_eq!(dst.x + dst.width + 1.0, width, "against the right edge");
            }
        }
        let first_text = list
            .commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Text { position, text, .. } if text == "350" => Some(position[0]),
                _ => None,
            })
            .next_back() // the shadow comes first, the text after it
            .expect("the first text");
        // Every text ends against its picture, so the shorter one starts further right.
        let short = font.measure("350", 1.0);
        assert_eq!(first_text, width - ICON_ROW - ICON_GAP - short);
    }

    #[test]
    fn boxes_are_measured_and_lit_when_pressed() {
        let font = FontAtlas::fallback();
        let cells = vec![
            ModCell {
                rect: [24.0, 0.0, 22.0, 22.0],
                pressed: true,
                color: 0xFFFFFF,
                text: "W".into(),
            },
            ModCell {
                rect: [0.0, 24.0, 70.0, 10.0],
                pressed: false,
                color: 0xFFFFFF,
                text: "---".into(),
            },
        ];
        let view = only(Vec::new(), cells, false);
        assert_eq!(measure_view(&font, &view), (70.0, 34.0));
        let mut list = DrawList::new();
        draw_mod_widgets(&mut list, &[view], 1.0, &font);
        let fills: Vec<Color> = list
            .commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::RoundedRect { color, .. } | DrawCommand::Rect { color, .. } => {
                    Some(*color)
                }
                _ => None,
            })
            .collect();
        assert_eq!(fills, [CELL_LIT, CELL_IDLE]);
    }

    fn marks() -> ModWidgetView {
        ModWidgetView {
            labels: vec![ModLabel {
                at: [50.0, 4.0],
                text: "S".into(),
                color: 0xFF_FFFF,
                alpha: 0.5,
                scale: 2.0,
            }],
            ticks: vec![ModTick {
                rect: [10.0, 1.0, 1.0, 3.0],
                color: 0xFF_FFFF,
                alpha: 0.25,
            }],
            reserve: [100.0, 30.0],
            style: WidgetStyle {
                background: None,
                padding: 0.0,
                ..WidgetStyle::default()
            },
            ..ModWidgetView::default()
        }
    }

    #[test]
    fn a_reserved_size_keeps_the_widget_from_shrinking() {
        let font = FontAtlas::fallback();
        let view = marks();
        assert_eq!(measure_view(&font, &view), (100.0, 30.0));
        let bigger = ModWidgetView {
            reserve: [0.0; 2],
            ..marks()
        };
        let (width, height) = measure_view(&font, &bigger);
        assert_eq!(height, 4.0 + LINE_HEIGHT * 2.0, "the label sets the height");
        assert!(
            width >= 50.0,
            "to the middle of the label and half its width"
        );
    }

    #[test]
    fn a_reserve_alone_is_not_an_empty_widget() {
        let view = ModWidgetView {
            reserve: [10.0, 10.0],
            ..ModWidgetView::default()
        };
        assert!(!view.is_empty());
        assert!(ModWidgetView::default().is_empty());
    }

    #[test]
    fn labels_are_centered_scaled_and_faded_and_ticks_are_faded_rectangles() {
        let font = FontAtlas::fallback();
        let mut list = DrawList::new();
        draw_mod_widgets(&mut list, &[marks()], 1.0, &font);
        let commands = list.commands();
        let DrawCommand::Rect { rect, color } = &commands[0] else {
            panic!("expected the tick first, got {:?}", commands[0]);
        };
        assert_eq!(*rect, Rect::new(10.0, 1.0, 1.0, 3.0));
        assert_eq!(color[3], 0.25);
        let (
            DrawCommand::Text { style: shadow, .. },
            DrawCommand::Text {
                position,
                style,
                text,
            },
        ) = (&commands[1], &commands[2])
        else {
            panic!("expected the shadow then the label");
        };
        assert_eq!(text, "S");
        assert_eq!(style.scale, 2.0);
        assert_eq!(style.color[3], 0.5);
        assert_eq!(shadow.color[3], 0.5, "the shadow fades with the text");
        let width = font.measure("S", 1.0) * 2.0;
        assert_eq!(*position, [50.0 - width / 2.0, 4.0]);
    }
}

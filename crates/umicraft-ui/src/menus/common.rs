// SPDX-License-Identifier: MIT OR Apache-2.0

//! Helpers shared by the menu screens: the GUI-unit coordinate space, centred
//! and shadowed text, word wrapping and the test harness.
//!
//! Every vanilla screen positions its widgets in "scaled" GUI units
//! (`ScaledResolution`), so one [`Gui`] value converts those units to the
//! physical pixels the draw list wants.

use crate::context::Ui;
use crate::draw::{Color, TextStyle};
use crate::geom::{Rect, ScrollArea};
use crate::id::WidgetId;
use crate::input::{InputEvent, Key, PointerButton};
use crate::legacy_text::parse_legacy;
use crate::styled_text::draw_styled;
use crate::text::FontAtlas;
use crate::widgets::{button_enabled, gui_scale};

/// Opaque white, the colour of titles (`0xFFFFFF`).
pub(crate) const WHITE: Color = [1.0, 1.0, 1.0, 1.0];
/// The grey of field labels (`0xA0A0A0`).
pub(crate) const LABEL_GRAY: Color = [0.627, 0.627, 0.627, 1.0];
/// The grey of secondary text (`0x808080`).
pub(crate) const DIM_GRAY: Color = [0.5, 0.5, 0.5, 1.0];
/// The grey of the disconnect title (`0xAAAAAA`).
pub(crate) const LIGHT_GRAY: Color = [0.667, 0.667, 0.667, 1.0];
/// The height of one text line in GUI units (`FontRenderer.FONT_HEIGHT`).
pub(crate) const LINE_HEIGHT: f32 = 9.0;

/// The vanilla GUI space of one frame: the viewport divided by the GUI scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Gui {
    /// Physical pixels per GUI unit.
    pub scale: f32,
    /// Screen width in GUI units (`ScaledResolution.getScaledWidth`).
    pub width: f32,
    /// Screen height in GUI units.
    pub height: f32,
    origin: [f32; 2],
}

/// Where and how a piece of text is drawn.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TextPlacement {
    /// Position in GUI units.
    pub at: [f32; 2],
    /// Text colour.
    pub color: Color,
    /// Multiple of the GUI scale.
    pub factor: f32,
    /// Whether `at[0]` is the centre of the text instead of its left edge.
    pub centered: bool,
}

impl TextPlacement {
    /// Left-aligned text at the GUI scale.
    pub(crate) fn left(at: [f32; 2], color: Color) -> Self {
        Self {
            at,
            color,
            factor: 1.0,
            centered: false,
        }
    }

    /// Centred text at the GUI scale.
    pub(crate) fn centered(at: [f32; 2], color: Color) -> Self {
        Self {
            centered: true,
            ..Self::left(at, color)
        }
    }
}

impl Gui {
    /// The GUI space of `viewport`; the scaled size rounds up as in
    /// `ScaledResolution`.
    pub(crate) fn new(viewport: Rect) -> Self {
        let scale = gui_scale(viewport.width as u32, viewport.height as u32) as f32;
        Self {
            scale,
            width: (viewport.width / scale).ceil(),
            height: (viewport.height / scale).ceil(),
            origin: [viewport.x, viewport.y],
        }
    }

    /// A physical position as GUI units.
    pub(crate) fn to_gui(self, position: [f32; 2]) -> [f32; 2] {
        [
            (position[0] - self.origin[0]) / self.scale,
            (position[1] - self.origin[1]) / self.scale,
        ]
    }

    /// `width / 2` with Java integer division.
    pub(crate) fn cx(&self) -> f32 {
        (self.width * 0.5).floor()
    }

    /// `height / divisor` with Java integer division.
    pub(crate) fn h_div(&self, divisor: f32) -> f32 {
        (self.height / divisor).floor()
    }

    /// A rectangle given in GUI units, as physical pixels.
    pub(crate) fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(
            self.origin[0] + x * self.scale,
            self.origin[1] + y * self.scale,
            w * self.scale,
            h * self.scale,
        )
    }

    /// Draws `text` with its shadow at the placement.
    ///
    /// The shadow is the colour at a quarter brightness, one scaled pixel
    /// down and right (`FontRenderer.renderString`).
    pub(crate) fn text(&self, ui: &mut Ui, atlas: &FontAtlas, text: &str, place: TextPlacement) {
        let scale = self.scale * place.factor;
        let width = atlas.measure(text, scale);
        let half = if place.centered { width * 0.5 } else { 0.0 };
        let x = self.origin[0] + place.at[0] * self.scale - half;
        let y = self.origin[1] + place.at[1] * self.scale;
        let c = place.color;
        let shadow = [c[0] * 0.25, c[1] * 0.25, c[2] * 0.25, c[3]];
        ui.draw()
            .text([x + scale, y + scale], text, TextStyle::new(scale, shadow));
        ui.draw().text([x, y], text, TextStyle::new(scale, c));
    }

    /// Draws `text` at GUI `at`, honouring legacy `§` codes; `base` colours
    /// the text before any code. `shadow` selects `drawStringWithShadow`
    /// over `drawString`. Returns the GUI width.
    pub(crate) fn legacy_text(
        &self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        text: &str,
        (at, base, shadow): ([f32; 2], Color, bool),
    ) -> f32 {
        let runs = parse_legacy(text, base);
        let pixel = [
            self.origin[0] + at[0] * self.scale,
            self.origin[1] + at[1] * self.scale,
        ];
        draw_styled(ui.draw(), atlas, pixel, &runs, self.scale, shadow, 0) / self.scale
    }

    /// Draws white text centred on GUI x `x` at GUI y `y`.
    pub(crate) fn title(&self, ui: &mut Ui, atlas: &FontAtlas, text: &str, x: f32, y: f32) {
        self.text(ui, atlas, text, TextPlacement::centered([x, y], WHITE));
    }

    /// A vanilla button; returns whether it was clicked.
    pub(crate) fn button(
        &self,
        ui: &mut Ui,
        id: WidgetId,
        rect: Rect,
        atlas: &FontAtlas,
        label: &str,
        enabled: bool,
    ) -> bool {
        button_enabled(ui, id, rect, atlas, label, enabled).clicked
    }
}

/// Whether Escape was pressed in this frame.
pub(crate) fn escape_pressed(ui: &Ui) -> bool {
    key_pressed(ui, Key::Escape)
}

/// Whether `key` was pressed in this frame.
pub(crate) fn key_pressed(ui: &Ui, key: Key) -> bool {
    ui.input()
        .events
        .iter()
        .any(|event| matches!(event, InputEvent::KeyPressed(k) if *k == key))
}

/// GUI units one wheel notch scrolls a list (`GuiSlot.handleMouseInput`
/// scrolls `slotHeight / 2` per notch; the slots here are 36 or 10 high, 18
/// is the server list value).
const WHEEL_STEP: f32 = 18.0;
/// Width of the list scrollbar in GUI units.
const SCROLLBAR_WIDTH: f32 = 6.0;

/// Applies this frame's wheel input to a list scroll `offset`.
///
/// `view` is the list rectangle in physical pixels, `content_height` and the
/// returned offset are in GUI units. The offset is clamped to the content.
pub(crate) fn wheel_scrolled(
    ui: &Ui,
    gui: &Gui,
    view: Rect,
    content_height: f32,
    offset: f32,
) -> f32 {
    wheel_scrolled_by(ui, gui, view, content_height, offset, WHEEL_STEP)
}

/// [`wheel_scrolled`] with the notch size of a list whose rows are `2 * step`
/// units high (`GuiSlot.handleMouseInput` scrolls `slotHeight / 2` per notch).
pub(crate) fn wheel_scrolled_by(
    ui: &Ui,
    gui: &Gui,
    view: Rect,
    content_height: f32,
    offset: f32,
    step: f32,
) -> f32 {
    let mut scroll = ScrollArea::new(Rect::new(0.0, 0.0, 1.0, view.height / gui.scale));
    scroll.set_content_height(content_height);
    scroll.set_offset(offset);
    if ui.is_hovered(view) {
        let notches: f32 = ui
            .input()
            .events
            .iter()
            .filter_map(|event| match event {
                InputEvent::Wheel { delta } => Some(*delta),
                _ => None,
            })
            .sum();
        scroll.scroll_by(notches * step);
    }
    scroll.offset()
}

/// Draws the vanilla scrollbar (`GuiSlot.drawScreen`) when the content is
/// taller than the view. `x`, `top` and `height` are GUI units.
pub(crate) fn draw_scrollbar(
    ui: &mut Ui,
    gui: &Gui,
    bar: [f32; 3],
    content_height: f32,
    offset: f32,
) {
    let [x, top, height] = bar;
    let max = (content_height - height).max(0.0);
    if max <= 0.0 {
        return;
    }
    let thumb = (height * height / content_height).clamp(32.0, height - 8.0);
    let y = (offset * (height - thumb) / max + top).max(top);
    ui.draw().rect(
        gui.rect(x, top, SCROLLBAR_WIDTH, height),
        [0.0, 0.0, 0.0, 1.0],
    );
    ui.draw()
        .rect(gui.rect(x, y, SCROLLBAR_WIDTH, thumb), [0.5, 0.5, 0.5, 1.0]);
    ui.draw().rect(
        gui.rect(x, y, SCROLLBAR_WIDTH - 1.0, thumb - 1.0),
        [0.753, 0.753, 0.753, 1.0],
    );
}

/// A drag of a list scrollbar under way: the grab, in GUI units from the top
/// of the thumb. The caller stores it next to the list offset, as a slider
/// stores its [`crate::widgets::SliderState`].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ScrollbarDrag(Option<f32>);

impl ScrollbarDrag {
    /// Whether the thumb is being dragged, so a list can ignore the press
    /// that started it.
    #[must_use]
    pub fn is_active(self) -> bool {
        self.0.is_some()
    }

    /// Moves `offset` with the pointer over the bar [`draw_scrollbar`] draws.
    ///
    /// `bar` is `[x, top, height]` in GUI units, `scale` the pixels per GUI
    /// unit, `content` the content height and `max` the largest offset. A
    /// press on the thumb keeps its grab; a press on the track centres the
    /// thumb on the pointer. The drag follows the pointer outside the bar and
    /// ends on the release.
    pub fn apply(
        &mut self,
        ui: &Ui,
        scale: f32,
        bar: [f32; 3],
        (content, max): (f32, f32),
        offset: f32,
    ) -> f32 {
        let [x, top, height] = bar;
        if max <= 0.0 || height < 40.0 {
            self.0 = None;
            return offset;
        }
        let thumb = (height * height / content).clamp(32.0, height - 8.0);
        let travel = height - thumb;
        let origin = ui.viewport();
        let to_gui = |p: [f32; 2]| [(p[0] - origin.x) / scale, (p[1] - origin.y) / scale];
        let to_offset = |y: f32, grab: f32| ((y - top - grab) / travel * max).clamp(0.0, max);
        let mut offset = offset;
        for event in &ui.input().events {
            match event {
                InputEvent::PointerPressed {
                    position,
                    button: PointerButton::Left,
                } => {
                    let p = to_gui(*position);
                    if (x..x + SCROLLBAR_WIDTH).contains(&p[0])
                        && (top..top + height).contains(&p[1])
                    {
                        let thumb_top = offset * travel / max + top;
                        let grab = if (thumb_top..thumb_top + thumb).contains(&p[1]) {
                            p[1] - thumb_top
                        } else {
                            thumb / 2.0
                        };
                        self.0 = Some(grab);
                        offset = to_offset(p[1], grab);
                    }
                }
                InputEvent::PointerMoved { position } => {
                    if let Some(grab) = self.0 {
                        offset = to_offset(to_gui(*position)[1], grab);
                    }
                }
                InputEvent::PointerReleased {
                    button: PointerButton::Left,
                    ..
                } => self.0 = None,
                _ => {}
            }
        }
        offset
    }
}

/// Draws a vertical gradient as horizontal bands.
pub(crate) fn gradient(ui: &mut Ui, rect: Rect, top: Color, bottom: Color) {
    const BANDS: u32 = 32;
    let band = rect.height / BANDS as f32;
    for index in 0..BANDS {
        let t = (index as f32 + 0.5) / BANDS as f32;
        let mix = |i: usize| top[i] + (bottom[i] - top[i]) * t;
        let y = rect.y + index as f32 * band;
        ui.draw().rect(
            Rect::new(rect.x, y, rect.width, band),
            [mix(0), mix(1), mix(2), mix(3)],
        );
    }
}

/// Removes legacy `§x` formatting codes (`EnumChatFormatting.getTextWithoutFormattingCodes`).
///
/// Server descriptions carry them; the menus draw plain text, so the codes
/// must not show as glyphs.
pub(crate) fn strip_formatting(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{a7}' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

/// Splits `text` into lines no wider than `max_width` pixels at `scale`
/// (`FontRenderer.listFormattedStringToWidth`): breaks at spaces, honours
/// `\n`, and cuts a word that is wider than a line.
pub(crate) fn wrap_lines(atlas: &FontAtlas, text: &str, max_width: f32, scale: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if atlas.measure(&candidate, scale) <= max_width {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            line = word.to_string();
            while atlas.measure(&line, scale) > max_width && line.chars().count() > 1 {
                let cut = fitting_prefix(atlas, &line, max_width, scale);
                let rest = line.split_off(cut);
                lines.push(std::mem::replace(&mut line, rest));
            }
        }
        lines.push(line);
    }
    lines
}

/// The byte length of the longest prefix of `text` that fits (at least one char).
fn fitting_prefix(atlas: &FontAtlas, text: &str, max_width: f32, scale: f32) -> usize {
    let mut end = 0;
    for (index, c) in text.char_indices() {
        let next = index + c.len_utf8();
        if end > 0 && atlas.measure(&text[..next], scale) > max_width {
            break;
        }
        end = next;
    }
    end
}

/// Shared helpers of the screen tests.
#[cfg(test)]
pub(crate) mod testing {
    use crate::context::Ui;
    use crate::geom::Rect;
    use crate::input::{InputEvent, Key, PointerButton, UiInput};

    /// The viewport of the tests: GUI scale 3, 427x240 GUI units.
    pub(crate) const VIEWPORT: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 1280.0,
        height: 720.0,
    };

    /// Runs one frame with `input` and returns what `f` returned.
    pub(crate) fn frame<R>(input: UiInput, f: impl FnOnce(&mut Ui) -> R) -> R {
        let mut ui = Ui::new(VIEWPORT);
        ui.begin_frame(VIEWPORT, input);
        let result = f(&mut ui);
        ui.end_frame();
        result
    }

    /// A left click in the middle of `rect`.
    pub(crate) fn click_at(rect: Rect) -> UiInput {
        click_point([rect.x + rect.width * 0.5, rect.y + rect.height * 0.5])
    }

    /// A left click at `position`.
    pub(crate) fn click_point(position: [f32; 2]) -> UiInput {
        UiInput::new(vec![
            InputEvent::PointerMoved { position },
            InputEvent::PointerPressed {
                position,
                button: PointerButton::Left,
            },
        ])
    }

    /// One key press.
    pub(crate) fn key(key: Key) -> UiInput {
        UiInput::new(vec![InputEvent::KeyPressed(key)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_space_follows_scaled_resolution() {
        let gui = Gui::new(testing::VIEWPORT);
        assert_eq!((gui.scale, gui.width, gui.height), (3.0, 427.0, 240.0));
        assert_eq!(gui.cx(), 213.0);
        assert_eq!(gui.h_div(4.0), 60.0);
        assert_eq!(gui.rect(1.0, 2.0, 3.0, 4.0), Rect::new(3.0, 6.0, 9.0, 12.0));
    }

    #[test]
    fn formatting_codes_are_removed() {
        assert_eq!(strip_formatting("\u{a7}aHello \u{a7}lworld"), "Hello world");
    }

    #[test]
    fn wrapping_breaks_at_spaces_and_newlines() {
        let atlas = FontAtlas::fallback();
        let one = atlas.measure("aaaa", 1.0);
        let lines = wrap_lines(&atlas, "aaaa bbbb\ncc", one + 1.0, 1.0);
        assert_eq!(lines, ["aaaa", "bbbb", "cc"]);
    }

    #[test]
    fn a_word_wider_than_a_line_is_cut() {
        let atlas = FontAtlas::fallback();
        let width = atlas.measure("aaa", 1.0);
        let lines = wrap_lines(&atlas, "aaaaaaa", width, 1.0);
        assert!(lines.len() >= 2);
        assert_eq!(lines.concat(), "aaaaaaa");
    }
}

#[cfg(test)]
mod scrollbar_drag_tests {
    use super::testing::{frame, VIEWPORT};
    use super::ScrollbarDrag;
    use crate::input::{InputEvent, PointerButton, UiInput};

    // Scale 3: bar at x = 100, top 32, height 200; content 600, so the thumb
    // is 66.7 high and the largest offset 400.
    const BAR: [f32; 3] = [100.0, 32.0, 200.0];

    fn step(drag: &mut ScrollbarDrag, event: InputEvent, offset: f32) -> f32 {
        frame(UiInput::new(vec![event]), |ui| {
            drag.apply(ui, 3.0, BAR, (600.0, 400.0), offset)
        })
    }

    fn at(y: f32) -> [f32; 2] {
        [102.0 * 3.0 + VIEWPORT.x, y * 3.0 + VIEWPORT.y]
    }

    #[test]
    fn dragging_the_thumb_moves_the_offset_until_the_release() {
        let mut drag = ScrollbarDrag::default();
        let press = InputEvent::PointerPressed {
            position: at(40.0),
            button: PointerButton::Left,
        };
        assert_eq!(step(&mut drag, press, 0.0), 0.0);
        assert!(drag.is_active());
        // The thumb travels 133.3 units for 400 of offset.
        let moved = step(
            &mut drag,
            InputEvent::PointerMoved {
                position: at(40.0 + 66.667),
            },
            0.0,
        );
        assert!((moved - 200.0).abs() < 0.1, "{moved}");
        let release = InputEvent::PointerReleased {
            position: at(100.0),
            button: PointerButton::Left,
        };
        step(&mut drag, release, moved);
        assert!(!drag.is_active());
        let after = InputEvent::PointerMoved { position: at(200.0) };
        assert_eq!(step(&mut drag, after, moved), moved);
    }

    #[test]
    fn a_press_on_the_track_centres_the_thumb_and_a_press_beside_it_does_nothing() {
        let mut drag = ScrollbarDrag::default();
        let press = InputEvent::PointerPressed {
            position: at(231.0),
            button: PointerButton::Left,
        };
        assert_eq!(step(&mut drag, press, 0.0), 400.0);
        let mut other = ScrollbarDrag::default();
        let beside = InputEvent::PointerPressed {
            position: [50.0 * 3.0, 100.0 * 3.0],
            button: PointerButton::Left,
        };
        assert_eq!(step(&mut other, beside, 7.0), 7.0);
        assert!(!other.is_active());
    }
}

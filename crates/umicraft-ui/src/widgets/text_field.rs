// SPDX-License-Identifier: MIT OR Apache-2.0

//! The single-line text field widget.
//!
//! [`TextFieldState`] owns the text, the caret and the horizontal scroll
//! offset. The caret is always a **`char` index** in `0..=char_count`, never a
//! byte offset, so an edit can split a multi-byte UTF-8 sequence only at a
//! valid boundary. Control characters (including newlines and tabs) are
//! discarded on entry: the field is single-line by construction.
//!
//! The widget takes the [`FontAtlas`] as a parameter because the UI core does
//! not own a font. The M4 ADR permits parameter refinements that do not force
//! another workstream to edit its files, and the atlas is what lets the text
//! field lay out its text, derive the caret position and map a click to a
//! character index with the same metrics the renderer uses. W5 passes the
//! atlas it loaded, or [`FontAtlas::fallback`] before the assets exist.
//!
//! The look is vanilla's `GuiTextField`: a 1 unit `0xA0A0A0` border around a
//! black fill (focused or not), white-grey text inset 4 units and a `_` caret
//! after the text, or a thin bar inside it. The widget has no clock, so the
//! caret blink (`cursorCounter / 6 % 2`) counts [`TextFieldState::tick`]
//! calls; a field nobody ticks keeps a steady caret.
//!
//! Editing uses the input events of the current frame, in arrival order:
//!
//! - [`InputEvent::Text`] inserts every non-control character at the caret,
//!   replacing the selection if there is one;
//! - Backspace removes the character before the caret and Delete the one after
//!   it, or the selection; with Control they remove a word;
//! - Home and End jump to the first and last boundary;
//! - the left and right arrows move the caret by one character, by a word with
//!   Control; Shift extends the selection instead of moving it;
//! - Control+A selects everything, Control+C copies the selection, Control+X
//!   cuts it and [`InputEvent::Paste`] (Control+V) replaces it with the
//!   clipboard text, as `GuiTextField.textboxKeyTyped` does. A word ends at a
//!   space.
//!
//! Text events while Control or Meta is held are ignored, so a shortcut never
//! inserts its letter; AltGr (Control and Alt together) still types.

use umicraft_core::text_edit::{word_boundary, Selection};

use crate::context::{Ui, UiResponse};
use crate::draw::{Color, TextStyle};
use crate::geom::{Insets, Rect};
use crate::id::WidgetId;
use crate::input::{InputEvent, Key};
use crate::text::FontAtlas;

use super::style;
use super::util;

/// Border width, in GUI units (`drawRect(x - 1, ...)`).
const BORDER_UNITS: f32 = 1.0;
/// Distance between the field edge and its text, in GUI units.
const TEXT_INSET_UNITS: f32 = 4.0;
/// Glyph height used to centre the text (`(height - 8) / 2`).
const GLYPH_UNITS: f32 = 8.0;
/// Ticks per half period of the caret blink (`cursorCounter / 6 % 2`).
const BLINK_TICKS: u32 = 6;
/// The caret glyph drawn after the text.
const CARET_GLYPH: &str = "_";

/// The editable state of one text field.
///
/// The state survives across frames; the widget itself is built every frame.
/// All indices are `char` indices. The text is kept single-line: control
/// characters are removed by [`Self::set_text`] and on insertion.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextFieldState {
    text: String,
    /// The caret and the fixed end of the selection.
    selection: Selection,
    scroll: f32,
    blink: u32,
    disabled: bool,
}

impl TextFieldState {
    /// Creates an empty field with the caret at zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a field containing `text` with the caret at the end.
    ///
    /// Control characters are removed, so the result is always single-line.
    #[must_use]
    pub fn with_text(text: impl Into<String>) -> Self {
        let text = single_line(&text.into());
        let caret = text.chars().count();
        Self {
            text,
            selection: Selection::at(caret),
            scroll: 0.0,
            blink: 0,
            disabled: false,
        }
    }

    /// Turns editing off (`GuiTextField.setEnabled(false)`): the field ignores
    /// clicks and keys, shows no caret and draws its text in the dim
    /// `0x707070`.
    pub fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
    }

    /// Whether the field is disabled.
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// Advances the caret blink by one game tick (`GuiTextField.updateCursorCounter`).
    pub fn tick(&mut self) {
        self.blink = self.blink.wrapping_add(1);
    }

    /// Whether the caret is in the visible half of its blink period.
    #[must_use]
    pub fn caret_blink_on(&self) -> bool {
        (self.blink / BLINK_TICKS).is_multiple_of(2)
    }

    /// The current text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Replaces the text, keeping the caret at its current index when possible.
    ///
    /// The caret is clamped to the new `char` count and the horizontal scroll
    /// returns to the start; the next [`text_field`] call scrolls again if the
    /// caret needs it. Control characters are removed.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = single_line(&text.into());
        self.selection
            .collapse(self.selection.caret(), self.char_count());
        self.scroll = 0.0;
    }

    /// The caret as a `char` index in `0..=char_count`.
    #[must_use]
    pub fn caret(&self) -> usize {
        self.selection.caret()
    }

    /// Moves the caret to `caret`, clamped to `0..=char_count`.
    pub fn set_caret(&mut self, caret: usize) {
        self.selection.collapse(caret, self.char_count());
    }

    /// Whether some text is selected.
    #[must_use]
    pub fn has_selection(&self) -> bool {
        self.selection.has_selection()
    }

    /// The selected text, empty when nothing is selected.
    #[must_use]
    pub fn selected_text(&self) -> String {
        let (start, end) = self.selection();
        self.text.chars().skip(start).take(end - start).collect()
    }

    /// Selects the whole text, the caret at the start
    /// (`setCursorPositionEnd` then `setSelectionPos(0)`).
    pub fn select_all(&mut self) {
        self.selection.select_all(self.char_count());
    }

    /// The selection as `char` indices, start before end.
    fn selection(&self) -> (usize, usize) {
        self.selection.range()
    }

    /// `GuiTextField.getNthWordFromPos` for one word: the start of the word
    /// before `from` (`forward == false`) or the start of the next one. A word
    /// ends at a space.
    fn word_from(&self, from: usize, forward: bool) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        word_boundary(&chars, from, forward, true)
    }

    /// The horizontal scroll offset in pixels; zero shows the start of the
    /// text.
    ///
    /// This is derived from the caret on every frame and is exposed for tests
    /// and for callers that want to inspect the field without reading the
    /// draw list.
    #[must_use]
    pub fn scroll_offset(&self) -> f32 {
        self.scroll
    }

    /// True when the field contains no character.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Number of `char`s in the text.
    fn char_count(&self) -> usize {
        self.text.chars().count()
    }

    /// Byte offset of the `char` boundary `char_index`.
    ///
    /// An index at or past the end maps to `text.len()`, which is always a
    /// valid boundary. Callers only pass indices in `0..=char_count`.
    fn byte_index(&self, char_index: usize) -> usize {
        self.text
            .char_indices()
            .nth(char_index)
            .map_or(self.text.len(), |(index, _)| index)
    }

    /// Replaces the selection (or nothing, at the caret) with the non-control
    /// characters of `typed` (`GuiTextField.writeText`).
    ///
    /// Returns true when the text changed.
    fn insert(&mut self, typed: &str) -> bool {
        let mut filtered = String::with_capacity(typed.len());
        filtered.extend(typed.chars().filter(|character| !character.is_control()));
        let (start, end) = self.selection();
        if filtered.is_empty() && start == end {
            return false;
        }
        let (from, to) = (self.byte_index(start), self.byte_index(end));
        self.text.replace_range(from..to, &filtered);
        let caret = start + filtered.chars().count();
        self.selection.collapse(caret, self.char_count());
        true
    }

    /// Removes the selection, or the character before the caret.
    ///
    /// Returns true when the text changed.
    fn backspace(&mut self) -> bool {
        if self.has_selection() {
            return self.insert("");
        }
        let caret = self.caret();
        if caret == 0 {
            return false;
        }
        self.selection.anchor_at(caret - 1);
        self.insert("")
    }

    /// Removes the selection, or the character after the caret.
    ///
    /// Returns true when the text changed.
    fn delete(&mut self) -> bool {
        if self.has_selection() {
            return self.insert("");
        }
        let caret = self.caret();
        if caret >= self.char_count() {
            return false;
        }
        self.selection.anchor_at(caret + 1);
        self.insert("")
    }

    /// Removes the selection, or the word before or after the caret
    /// (`GuiTextField.deleteWords`).
    fn delete_word(&mut self, forward: bool) -> bool {
        if !self.has_selection() {
            self.selection
                .anchor_at(self.word_from(self.caret(), forward));
        }
        self.insert("")
    }

    /// Moves the caret to `caret`, clamped, and reports whether it moved.
    fn move_caret(&mut self, caret: usize, extend: bool) -> bool {
        self.selection.move_to(caret, self.char_count(), extend)
    }
}

/// Draws a single-line text field and applies this frame's edits.
///
/// `id` must stay stable across frames and `atlas` provides the metrics used
/// for layout, caret placement and click-to-caret. The call site must place
/// `atlas` and `state` in the order shown here; this is the parameter
/// refinement the M4 ADR allows so that the client can pass the loaded font.
///
/// The function calls [`Ui::response`] exactly once, before it reads the input
/// events, so the response already reflects a click that focused the field.
/// It returns [`UiResponse::changed`] set when the text **or** the caret
/// changed in this frame; the same flag is stored with [`Ui::mark_changed`],
/// so [`crate::UiOutput::responses`] agrees with the returned value.
///
/// Edits are applied only while the field owns the focus, but a click always
/// moves the caret to the clicked character boundary. Text longer than the
/// field scrolls horizontally; the caret is kept inside the visible area.
///
/// # Example
///
/// ```
/// use umicraft_ui::widgets::{text_field, TextFieldState};
/// use umicraft_ui::{FontAtlas, Rect, Ui, UiInput};
///
/// let viewport = Rect::new(0.0, 0.0, 320.0, 180.0);
/// let mut ui = Ui::new(viewport);
/// let mut state = TextFieldState::with_text("hello");
///
/// ui.begin_frame(viewport, UiInput::default());
/// let id = ui.id("name");
/// let response = text_field(
///     &mut ui,
///     id,
///     Rect::new(10.0, 10.0, 160.0, 28.0),
///     &FontAtlas::fallback(),
///     &mut state,
/// );
/// assert!(!response.changed);
/// ui.end_frame();
/// ```
pub fn text_field(
    ui: &mut Ui,
    id: WidgetId,
    rect: Rect,
    atlas: &FontAtlas,
    state: &mut TextFieldState,
) -> UiResponse {
    text_field_scaled(ui, id, rect, atlas, state, style::TEXT_SCALE)
}

/// Like [`text_field`], with one GUI unit of the field (border, text inset,
/// glyph size) equal to `scale` pixels. The menu screens pass the GUI scale so
/// the field is 200x20 units like `GuiTextField`.
pub fn text_field_scaled(
    ui: &mut Ui,
    id: WidgetId,
    rect: Rect,
    atlas: &FontAtlas,
    state: &mut TextFieldState,
    scale: f32,
) -> UiResponse {
    let mut response = ui.response(id, rect);

    // A caller may have replaced the text or the caret between frames.
    let count = state.char_count();
    state.selection.clamp(count);

    let inner = rect.deflate(Insets {
        left: TEXT_INSET_UNITS * scale,
        right: TEXT_INSET_UNITS * scale,
        top: BORDER_UNITS * scale,
        bottom: BORDER_UNITS * scale,
    });
    let mut changed = false;

    if response.clicked && !state.disabled {
        if let Some(position) = util::last_primary_press(ui) {
            let local_x = position[0] - inner.x + state.scroll;
            let caret = atlas.x_to_char_index(&state.text, local_x, scale);
            let extend = ui.input().modifiers.shift;
            changed |= state.move_caret(caret, extend);
        }
    }

    if response.focused && !state.disabled {
        let modifiers = ui.input().modifiers;
        // Control (Command on macOS) shortcuts; AltGr reports Control and Alt
        // together and must still type its characters.
        let shortcut = (modifiers.control || modifiers.meta) && !modifiers.alt;
        let combo = shortcut && !modifiers.shift;
        let mut copied = None;
        for event in &ui.input().events {
            match event {
                // Text events may be IME or paste and can carry several
                // characters.
                InputEvent::Text(text) if !shortcut => changed |= state.insert(text),
                InputEvent::Paste(text) => changed |= state.insert(text),
                InputEvent::KeyPressed(Key::A) if combo => {
                    state.select_all();
                    changed = true;
                }
                InputEvent::KeyPressed(Key::C) if combo => {
                    copied = Some(state.selected_text());
                }
                InputEvent::KeyPressed(Key::X) if combo => {
                    copied = Some(state.selected_text());
                    changed |= state.insert("");
                }
                InputEvent::KeyPressed(Key::Backspace) if shortcut => {
                    changed |= state.delete_word(false);
                }
                InputEvent::KeyPressed(Key::Delete) if shortcut => {
                    changed |= state.delete_word(true);
                }
                InputEvent::KeyPressed(Key::Backspace) => changed |= state.backspace(),
                InputEvent::KeyPressed(Key::Delete) => changed |= state.delete(),
                InputEvent::KeyPressed(Key::Home) => {
                    changed |= state.move_caret(0, modifiers.shift);
                }
                InputEvent::KeyPressed(Key::End) => {
                    let end = state.char_count();
                    changed |= state.move_caret(end, modifiers.shift);
                }
                InputEvent::KeyPressed(key @ (Key::ArrowLeft | Key::ArrowRight)) => {
                    let forward = *key == Key::ArrowRight;
                    let target = if shortcut {
                        state.word_from(state.caret(), forward)
                    } else if forward {
                        state.caret() + 1
                    } else {
                        state.caret().saturating_sub(1)
                    };
                    changed |= state.move_caret(target, modifiers.shift);
                }
                _ => {}
            }
        }
        if let Some(text) = copied.filter(|text| !text.is_empty()) {
            ui.set_clipboard(text);
        }
    }

    let caret_x = atlas.char_index_to_x(&state.text, state.caret(), scale);
    let text_width = atlas.measure(&state.text, scale);
    // Reserve the caret glyph so the caret at the end of a full line stays
    // inside the clip instead of being cut at the right edge.
    let caret_space = (inner.width - atlas.measure(CARET_GLYPH, scale)).max(0.0);
    if caret_x < state.scroll {
        state.scroll = caret_x;
    } else if caret_x > state.scroll + caret_space {
        state.scroll = caret_x - caret_space;
    }
    state.scroll = state.scroll.clamp(0.0, (text_width - caret_space).max(0.0));

    if !rect.is_empty() {
        draw_field(
            ui,
            rect,
            inner,
            atlas,
            state,
            response.focused && !state.disabled,
            scale,
        );
    }

    if changed {
        response.changed = true;
        ui.mark_changed(id);
    }
    response
}

/// Draws the border, the fill, the text and the caret of a field
/// (`GuiTextField.drawTextBox`).
fn draw_field(
    ui: &mut Ui,
    rect: Rect,
    inner: Rect,
    atlas: &FontAtlas,
    state: &TextFieldState,
    focused: bool,
    scale: f32,
) {
    ui.draw().rect(rect, style::FIELD_BORDER);
    ui.draw().rect(
        rect.deflate(Insets::all(BORDER_UNITS * scale)),
        style::FIELD_BACKGROUND,
    );
    if inner.is_empty() {
        return;
    }
    let text_y = rect.y + (rect.height - GLYPH_UNITS * scale) * 0.5;
    let text_x = inner.x - state.scroll;
    ui.push_clip(inner);
    if state.has_selection() {
        let (start, end) = state.selection();
        let from = text_x + atlas.char_index_to_x(&state.text, start, scale);
        let to = text_x + atlas.char_index_to_x(&state.text, end, scale);
        ui.draw().rect(
            Rect::new(from, text_y - scale, to - from, (GLYPH_UNITS + 2.0) * scale),
            style::SELECTION,
        );
    }
    if !state.text.is_empty() {
        let (color, shadow) = if state.disabled {
            (
                style::TEXT_FIELD_DISABLED,
                style::TEXT_FIELD_DISABLED_SHADOW,
            )
        } else {
            (style::TEXT_FIELD, style::TEXT_FIELD_SHADOW)
        };
        draw_shadowed(ui, [text_x, text_y], &state.text, (color, shadow), scale);
    }
    if focused && state.caret_blink_on() {
        let caret_x = text_x + atlas.char_index_to_x(&state.text, state.caret(), scale);
        if state.caret() < state.text.chars().count() {
            let bar = Rect::new(
                caret_x - scale,
                text_y - scale,
                scale,
                (GLYPH_UNITS + 2.0) * scale,
            );
            ui.draw().rect(bar, style::CARET_BAR);
        } else {
            draw_shadowed(
                ui,
                [caret_x, text_y],
                CARET_GLYPH,
                (style::TEXT_FIELD, style::TEXT_FIELD_SHADOW),
                scale,
            );
        }
    }
    ui.pop_clip();
}

/// Draws `text` with the vanilla shadow (a quarter of the brightness, one
/// unit down and right, `drawStringWithShadow`).
fn draw_shadowed(
    ui: &mut Ui,
    at: [f32; 2],
    text: &str,
    (color, shadow): (Color, Color),
    scale: f32,
) {
    ui.draw().text(
        [at[0] + scale, at[1] + scale],
        text,
        TextStyle::new(scale, shadow),
    );
    ui.draw().text(at, text, TextStyle::new(scale, color));
}

/// Removes every control character, keeping the field single-line.
fn single_line(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_control())
        .collect()
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration tests for the M4 widgets and the acceptance screen.
//!
//! The tests drive the widgets exactly like the windowed and headless paths do:
//! one `Ui` frame in, one `UiOutput` out. The synthetic font is the original
//! 3×5 fallback, so no Mojang asset is read (spec §3 rules 3 and 4).

use umicraft_ui::debug_screen::DebugScreen;
use umicraft_ui::widgets::{
    button, list, panel, text_field, ListState, TextFieldState, PANEL_INSET, ROW_HEIGHT,
};
use umicraft_ui::{
    DrawCommand, FontAtlas, InputEvent, Key, PointerButton, Rect, Ui, UiInput, UiOutput,
    UiResponse, WidgetId,
};

/// The viewport used by most tests.
const VIEWPORT: Rect = Rect::new(0.0, 0.0, 640.0, 360.0);

/// A primary click at `position`.
fn click(position: [f32; 2]) -> UiInput {
    UiInput::new(vec![
        InputEvent::PointerMoved { position },
        InputEvent::PointerPressed {
            position,
            button: PointerButton::Left,
        },
    ])
}

/// A pointer movement without a press.
fn moved(position: [f32; 2]) -> UiInput {
    UiInput::new(vec![InputEvent::PointerMoved { position }])
}

/// A text insertion event.
fn typed(text: &str) -> UiInput {
    UiInput::new(vec![InputEvent::Text(text.to_owned())])
}

/// A single key press.
fn key(key: Key) -> UiInput {
    UiInput::new(vec![InputEvent::KeyPressed(key)])
}

/// A wheel event with the pointer over `position`.
fn wheel(position: [f32; 2], delta: f32) -> UiInput {
    UiInput::new(vec![
        InputEvent::PointerMoved { position },
        InputEvent::Wheel { delta },
    ])
}

/// The centre of a rectangle.
fn center(rect: Rect) -> [f32; 2] {
    [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5]
}

/// The colour of the first rectangle command that fills `rect` exactly.
fn rect_color(output: &UiOutput, rect: Rect) -> Option<[f32; 4]> {
    output
        .draw_list
        .commands()
        .iter()
        .find_map(|command| match command {
            DrawCommand::Rect {
                rect: filled,
                color,
            } if *filled == rect => Some(*color),
            _ => None,
        })
}

/// The position and scale of the first text run equal to `text`.
fn text_run(output: &UiOutput, text: &str) -> Option<([f32; 2], f32)> {
    output
        .draw_list
        .commands()
        .iter()
        .find_map(|command| match command {
            DrawCommand::Text {
                position,
                text: run,
                style,
            } if run == text => Some((*position, style.scale)),
            _ => None,
        })
}

/// The thin vertical rectangle of a caret, if the frame drew one.
fn caret_rect(output: &UiOutput) -> Option<Rect> {
    output
        .draw_list
        .commands()
        .iter()
        .find_map(|command| match command {
            DrawCommand::Rect { rect, .. } if rect.width < 4.0 && rect.height >= 8.0 => Some(*rect),
            _ => None,
        })
}

/// The rectangle of the first clip scope in the draw list.
fn clip_rect(output: &UiOutput) -> Option<Rect> {
    output
        .draw_list
        .commands()
        .iter()
        .find_map(|command| match command {
            DrawCommand::PushClip { rect } => Some(*rect),
            _ => None,
        })
}

/// Number of text commands whose content starts with `prefix`.
fn text_count(output: &UiOutput, prefix: &str) -> usize {
    output
        .draw_list
        .commands()
        .iter()
        .filter(
            |command| matches!(command, DrawCommand::Text { text, .. } if text.starts_with(prefix)),
        )
        .count()
}

/// Number of clip scopes opened in the draw list.
fn push_clips(output: &UiOutput) -> usize {
    output
        .draw_list
        .commands()
        .iter()
        .filter(|command| matches!(command, DrawCommand::PushClip { .. }))
        .count()
}

/// Number of clip scopes closed in the draw list.
fn pop_clips(output: &UiOutput) -> usize {
    output
        .draw_list
        .commands()
        .iter()
        .filter(|command| matches!(command, DrawCommand::PopClip))
        .count()
}

/// A list with `count` rows named `ROW 0`, `ROW 1`, …
fn rows(count: usize) -> Vec<String> {
    (0..count).map(|index| format!("ROW {index}")).collect()
}

// ---------------------------------------------------------------- button ----

const BUTTON_RECT: Rect = Rect::new(20.0, 20.0, 120.0, 32.0);

#[test]
fn button_reports_hover_press_and_centres_its_label() {
    let atlas = FontAtlas::fallback();
    let mut ui = Ui::new(VIEWPORT);
    let id = ui.id("button");

    ui.begin_frame(VIEWPORT, moved([30.0, 30.0]));
    let hovered = button(&mut ui, id, BUTTON_RECT, &atlas, "GO");
    assert!(hovered.hovered);
    assert!(!hovered.clicked);
    assert!(!hovered.focused);
    let output = ui.end_frame();
    assert_eq!(output.responses, vec![hovered]);
    assert!(output.focus.is_none());

    ui.begin_frame(VIEWPORT, click([40.0, 30.0]));
    let pressed = button(&mut ui, id, BUTTON_RECT, &atlas, "GO");
    assert!(pressed.hovered);
    assert!(pressed.clicked);
    assert!(pressed.focused);
    let output = ui.end_frame();
    assert_eq!(output.responses, vec![pressed]);
    assert_eq!(output.focus, Some(id));

    let (position, scale) = text_run(&output, "GO").expect("the label is drawn");
    let width = atlas.measure("GO", scale);
    let height = atlas.line_height(scale);
    assert!((position[0] + width * 0.5 - BUTTON_RECT.x - BUTTON_RECT.width * 0.5).abs() < 0.01);
    assert!((position[1] + height * 0.5 - BUTTON_RECT.y - BUTTON_RECT.height * 0.5).abs() < 0.01);
    assert_eq!(push_clips(&output), 1, "the label is clipped to the button");
    assert_eq!(pop_clips(&output), 1);
}

#[test]
fn button_draws_distinct_idle_hover_and_pressed_colours() {
    let atlas = FontAtlas::fallback();
    let mut ui = Ui::new(VIEWPORT);
    let id = ui.id("button");

    ui.begin_frame(VIEWPORT, moved([500.0, 300.0]));
    let idle = button(&mut ui, id, BUTTON_RECT, &atlas, "GO");
    assert!(!idle.hovered && !idle.clicked);
    let idle_color = rect_color(&ui.end_frame(), BUTTON_RECT).expect("button fill");

    ui.begin_frame(VIEWPORT, moved([30.0, 30.0]));
    let hover = button(&mut ui, id, BUTTON_RECT, &atlas, "GO");
    assert!(hover.hovered && !hover.clicked);
    let hover_color = rect_color(&ui.end_frame(), BUTTON_RECT).expect("button fill");

    ui.begin_frame(VIEWPORT, click([30.0, 30.0]));
    let press = button(&mut ui, id, BUTTON_RECT, &atlas, "GO");
    assert!(press.clicked);
    let press_color = rect_color(&ui.end_frame(), BUTTON_RECT).expect("button fill");

    assert_ne!(idle_color, hover_color);
    assert_ne!(idle_color, press_color);
    assert_ne!(hover_color, press_color);
}

#[test]
fn button_ignores_a_click_outside_its_rectangle() {
    let atlas = FontAtlas::fallback();
    let mut ui = Ui::new(VIEWPORT);
    let id = ui.id("button");

    ui.begin_frame(VIEWPORT, click([400.0, 300.0]));
    let response = button(&mut ui, id, BUTTON_RECT, &atlas, "GO");
    assert!(!response.hovered);
    assert!(!response.clicked);
    assert!(!response.focused);
    let output = ui.end_frame();
    assert_eq!(output.focus, None);
    assert_eq!(output.responses[0], response);
}

#[test]
fn overlapping_buttons_follow_the_first_hit_rule() {
    let atlas = FontAtlas::fallback();
    let mut ui = Ui::new(VIEWPORT);
    let first_id = ui.id("first");
    let second_id = ui.id("second");

    ui.begin_frame(VIEWPORT, click([30.0, 30.0]));
    let first = button(&mut ui, first_id, BUTTON_RECT, &atlas, "FIRST");
    let second = button(&mut ui, second_id, BUTTON_RECT, &atlas, "SECOND");
    assert!(first.clicked);
    assert!(second.hovered);
    assert!(!second.clicked);
    let output = ui.end_frame();
    assert_eq!(output.focus, Some(first_id));
    assert_eq!(output.responses.len(), 2);
}

// ------------------------------------------------------------ text field ----

const FIELD_RECT: Rect = Rect::new(20.0, 40.0, 240.0, 32.0);

fn run_field(
    ui: &mut Ui,
    id: WidgetId,
    rect: Rect,
    atlas: &FontAtlas,
    state: &mut TextFieldState,
    input: UiInput,
) -> (UiResponse, UiOutput) {
    let viewport = ui.viewport();
    ui.begin_frame(viewport, input);
    let response = text_field(ui, id, rect, atlas, state);
    let output = ui.end_frame();
    (response, output)
}

/// Clicks near the right edge of `rect`, which focuses the field. For text
/// narrower than the field the click also lands the caret at the end.
fn focus_field(
    ui: &mut Ui,
    id: WidgetId,
    rect: Rect,
    atlas: &FontAtlas,
    state: &mut TextFieldState,
) {
    let position = [rect.right() - 4.0, rect.y + rect.height * 0.5];
    let (response, _) = run_field(ui, id, rect, atlas, state, click(position));
    assert!(response.clicked, "the field must take the click");
    assert!(response.focused, "the field must take the focus");
}

#[test]
fn text_field_types_and_edits_with_the_caret() {
    let atlas = FontAtlas::fallback();
    let mut ui = Ui::new(VIEWPORT);
    let id = ui.id("field");
    let mut state = TextFieldState::new();
    focus_field(&mut ui, id, FIELD_RECT, &atlas, &mut state);

    let (response, _) = run_field(&mut ui, id, FIELD_RECT, &atlas, &mut state, typed("abc"));
    assert!(response.changed);
    assert_eq!(state.text(), "abc");
    assert_eq!(state.caret(), 3);

    let (response, output) = run_field(&mut ui, id, FIELD_RECT, &atlas, &mut state, typed("de"));
    assert!(response.changed);
    assert!(output.responses[0].changed, "the stored response agrees");
    assert_eq!(state.text(), "abcde");
    assert_eq!(state.caret(), 5);

    let (response, _) = run_field(
        &mut ui,
        id,
        FIELD_RECT,
        &atlas,
        &mut state,
        key(Key::Backspace),
    );
    assert!(response.changed);
    assert_eq!(state.text(), "abcd");
    assert_eq!(state.caret(), 4);

    // Delete at the end of the text is a no-op.
    let (response, _) = run_field(
        &mut ui,
        id,
        FIELD_RECT,
        &atlas,
        &mut state,
        key(Key::Delete),
    );
    assert!(!response.changed);
    assert_eq!(state.text(), "abcd");

    let (response, _) = run_field(
        &mut ui,
        id,
        FIELD_RECT,
        &atlas,
        &mut state,
        key(Key::ArrowLeft),
    );
    assert!(response.changed);
    assert_eq!(state.caret(), 3);

    let (response, _) = run_field(
        &mut ui,
        id,
        FIELD_RECT,
        &atlas,
        &mut state,
        key(Key::Delete),
    );
    assert!(response.changed);
    assert_eq!(state.text(), "abc");
    assert_eq!(state.caret(), 3);

    // Backspace at the start is a no-op.
    run_field(&mut ui, id, FIELD_RECT, &atlas, &mut state, key(Key::Home));
    let (response, _) = run_field(
        &mut ui,
        id,
        FIELD_RECT,
        &atlas,
        &mut state,
        key(Key::Backspace),
    );
    assert!(!response.changed);
    assert_eq!(state.text(), "abc");
    assert_eq!(state.caret(), 0);
}

#[test]
fn text_field_home_end_and_arrows_stay_in_bounds() {
    let atlas = FontAtlas::fallback();
    let mut ui = Ui::new(VIEWPORT);
    let id = ui.id("field");
    let mut state = TextFieldState::with_text("abc");
    focus_field(&mut ui, id, FIELD_RECT, &atlas, &mut state);
    assert_eq!(state.caret(), 3);

    let (response, _) = run_field(&mut ui, id, FIELD_RECT, &atlas, &mut state, key(Key::End));
    assert!(!response.changed, "End at the end does not move the caret");

    let (response, _) = run_field(&mut ui, id, FIELD_RECT, &atlas, &mut state, key(Key::Home));
    assert!(response.changed);
    assert_eq!(state.caret(), 0);

    let (response, _) = run_field(&mut ui, id, FIELD_RECT, &atlas, &mut state, key(Key::Home));
    assert!(!response.changed);

    run_field(
        &mut ui,
        id,
        FIELD_RECT,
        &atlas,
        &mut state,
        key(Key::ArrowLeft),
    );
    assert_eq!(state.caret(), 0);
    run_field(
        &mut ui,
        id,
        FIELD_RECT,
        &atlas,
        &mut state,
        key(Key::ArrowRight),
    );
    assert_eq!(state.caret(), 1);
    run_field(
        &mut ui,
        id,
        FIELD_RECT,
        &atlas,
        &mut state,
        key(Key::ArrowRight),
    );
    assert_eq!(state.caret(), 2);
    run_field(&mut ui, id, FIELD_RECT, &atlas, &mut state, key(Key::End));
    assert_eq!(state.caret(), 3);
    let (response, _) = run_field(
        &mut ui,
        id,
        FIELD_RECT,
        &atlas,
        &mut state,
        key(Key::ArrowRight),
    );
    assert!(!response.changed);
    assert_eq!(state.caret(), 3);
}

#[test]
fn text_field_edits_at_multi_byte_character_boundaries() {
    let atlas = FontAtlas::fallback();
    let mut ui = Ui::new(VIEWPORT);
    let id = ui.id("field");
    let mut state = TextFieldState::with_text("aé☃");
    focus_field(&mut ui, id, FIELD_RECT, &atlas, &mut state);
    assert_eq!(state.caret(), 3);

    run_field(
        &mut ui,
        id,
        FIELD_RECT,
        &atlas,
        &mut state,
        key(Key::ArrowLeft),
    );
    assert_eq!(state.caret(), 2, "one char, not one byte, per arrow");

    run_field(
        &mut ui,
        id,
        FIELD_RECT,
        &atlas,
        &mut state,
        key(Key::Backspace),
    );
    assert_eq!(state.text(), "a☃");
    assert_eq!(state.caret(), 1);

    run_field(
        &mut ui,
        id,
        FIELD_RECT,
        &atlas,
        &mut state,
        key(Key::Delete),
    );
    assert_eq!(state.text(), "a");
    assert_eq!(state.caret(), 1);

    run_field(&mut ui, id, FIELD_RECT, &atlas, &mut state, typed("é"));
    assert_eq!(state.text(), "aé");
    assert_eq!(state.caret(), 2);

    run_field(
        &mut ui,
        id,
        FIELD_RECT,
        &atlas,
        &mut state,
        key(Key::ArrowLeft),
    );
    run_field(
        &mut ui,
        id,
        FIELD_RECT,
        &atlas,
        &mut state,
        key(Key::Delete),
    );
    assert_eq!(state.text(), "a");
    assert_eq!(state.caret(), 1);

    state.set_caret(99);
    assert_eq!(state.caret(), 1, "set_caret clamps to the char count");
    state.set_text("héllo");
    assert_eq!(state.text(), "héllo");
    assert_eq!(state.caret(), 1);

    let stripped = TextFieldState::with_text("a\nb\tc");
    assert_eq!(stripped.text(), "abc", "the field is single-line");
    assert_eq!(stripped.caret(), 3);
}

#[test]
fn text_field_click_positions_the_caret_with_the_atlas() {
    let atlas = FontAtlas::fallback();
    let mut ui = Ui::new(VIEWPORT);
    let id = ui.id("field");
    let mut state = TextFieldState::with_text("abcd");

    // The first frame establishes the text origin and the widget scale.
    let (_, output) = run_field(
        &mut ui,
        id,
        FIELD_RECT,
        &atlas,
        &mut state,
        UiInput::default(),
    );
    let (origin, scale) = text_run(&output, "abcd").expect("the text is drawn");
    let target_x = origin[0] + atlas.char_index_to_x("abcd", 2, scale);
    let position = [target_x, FIELD_RECT.y + FIELD_RECT.height * 0.5];

    let (response, _) = run_field(&mut ui, id, FIELD_RECT, &atlas, &mut state, click(position));
    assert!(response.clicked);
    assert!(response.focused);
    assert!(response.changed, "the caret moved");
    assert_eq!(state.caret(), 2);

    // A click before the text start clamps to the first boundary.
    let (_, output) = run_field(
        &mut ui,
        id,
        FIELD_RECT,
        &atlas,
        &mut state,
        UiInput::default(),
    );
    let _ = text_run(&output, "abcd").expect("the text is drawn");
    let left = [FIELD_RECT.x + 2.0, FIELD_RECT.y + FIELD_RECT.height * 0.5];
    run_field(&mut ui, id, FIELD_RECT, &atlas, &mut state, click(left));
    assert_eq!(state.caret(), 0);
}

#[test]
fn text_field_only_edits_while_focused() {
    let atlas = FontAtlas::fallback();
    let mut ui = Ui::new(VIEWPORT);
    let first_id = ui.id("first");
    let second_id = ui.id("second");
    let first_rect = Rect::new(20.0, 20.0, 200.0, 32.0);
    let second_rect = Rect::new(20.0, 80.0, 200.0, 32.0);
    let mut first = TextFieldState::new();
    let mut second = TextFieldState::new();

    ui.begin_frame(VIEWPORT, click(center(first_rect)));
    let response = text_field(&mut ui, first_id, first_rect, &atlas, &mut first);
    text_field(&mut ui, second_id, second_rect, &atlas, &mut second);
    assert!(response.clicked && response.focused);
    ui.end_frame();

    ui.begin_frame(VIEWPORT, typed("A"));
    text_field(&mut ui, first_id, first_rect, &atlas, &mut first);
    text_field(&mut ui, second_id, second_rect, &atlas, &mut second);
    ui.end_frame();
    assert_eq!(first.text(), "A");
    assert_eq!(second.text(), "");

    ui.begin_frame(VIEWPORT, click(center(second_rect)));
    text_field(&mut ui, first_id, first_rect, &atlas, &mut first);
    text_field(&mut ui, second_id, second_rect, &atlas, &mut second);
    ui.end_frame();

    ui.begin_frame(VIEWPORT, typed("B"));
    text_field(&mut ui, first_id, first_rect, &atlas, &mut first);
    text_field(&mut ui, second_id, second_rect, &atlas, &mut second);
    ui.end_frame();
    assert_eq!(first.text(), "A");
    assert_eq!(second.text(), "B");

    // Clearing the focus stops the edits entirely.
    ui.set_focus(None);
    ui.begin_frame(VIEWPORT, typed("C"));
    text_field(&mut ui, first_id, first_rect, &atlas, &mut first);
    text_field(&mut ui, second_id, second_rect, &atlas, &mut second);
    ui.end_frame();
    assert_eq!(first.text(), "A");
    assert_eq!(second.text(), "B");
}

#[test]
fn text_field_scrolls_and_clips_long_text() {
    let atlas = FontAtlas::fallback();
    let mut ui = Ui::new(VIEWPORT);
    let id = ui.id("field");
    let long = "abcdefghijklmnopqrstuvwxyz0123456789";
    let mut state = TextFieldState::with_text(long);
    focus_field(&mut ui, id, FIELD_RECT, &atlas, &mut state);
    // A click on a long, unscrolled field lands on the visible boundary; End
    // then moves to the real end and scrolls the last characters into view.
    run_field(&mut ui, id, FIELD_RECT, &atlas, &mut state, key(Key::End));
    assert_eq!(state.caret(), long.chars().count());

    let (_, output) = run_field(
        &mut ui,
        id,
        FIELD_RECT,
        &atlas,
        &mut state,
        UiInput::default(),
    );
    let clip = clip_rect(&output).expect("the field clips its text");
    assert!(clip.x > FIELD_RECT.x && clip.right() < FIELD_RECT.right());
    assert!(clip.y > FIELD_RECT.y && clip.bottom() < FIELD_RECT.bottom());

    let (position, scale) = text_run(&output, long).expect("the text is drawn");
    assert!(
        position[0] < clip.x,
        "long text scrolls left behind the clip: {} !< {}",
        position[0],
        clip.x
    );
    assert!(state.scroll_offset() > 0.0);

    let caret = caret_rect(&output).expect("the focused field draws a caret");
    assert!(caret.x >= clip.x - 0.01, "the caret stays inside the clip");
    assert!(
        caret.right() <= clip.right() + 0.01,
        "the caret stays inside the clip"
    );
    assert!((caret.height - atlas.line_height(scale)).abs() < 0.01);

    // Home returns the view to the start of the text.
    run_field(&mut ui, id, FIELD_RECT, &atlas, &mut state, key(Key::Home));
    assert_eq!(state.caret(), 0);
    assert_eq!(state.scroll_offset(), 0.0);
}

#[test]
fn text_field_draws_the_caret_only_when_focused() {
    let atlas = FontAtlas::fallback();
    let mut ui = Ui::new(VIEWPORT);
    let id = ui.id("field");
    let mut state = TextFieldState::new();

    let (_, output) = run_field(
        &mut ui,
        id,
        FIELD_RECT,
        &atlas,
        &mut state,
        UiInput::default(),
    );
    assert!(
        caret_rect(&output).is_none(),
        "an unfocused field has no caret"
    );

    focus_field(&mut ui, id, FIELD_RECT, &atlas, &mut state);
    let (_, output) = run_field(
        &mut ui,
        id,
        FIELD_RECT,
        &atlas,
        &mut state,
        UiInput::default(),
    );
    assert!(
        caret_rect(&output).is_some(),
        "a focused empty field shows the caret"
    );

    // The press outside clears the focus at the end of the frame; the next
    // frame draws no caret.
    run_field(
        &mut ui,
        id,
        FIELD_RECT,
        &atlas,
        &mut state,
        click([500.0, 300.0]),
    );
    let (_, output) = run_field(
        &mut ui,
        id,
        FIELD_RECT,
        &atlas,
        &mut state,
        UiInput::default(),
    );
    assert!(caret_rect(&output).is_none());
}

// ------------------------------------------------------------------ list ----

const LIST_RECT: Rect = Rect::new(20.0, 20.0, 200.0, 100.0);

fn run_list(
    ui: &mut Ui,
    id: WidgetId,
    rect: Rect,
    items: &[String],
    state: &mut ListState,
    input: UiInput,
) -> (UiResponse, UiOutput) {
    let viewport = ui.viewport();
    ui.begin_frame(viewport, input);
    let response = list(ui, id, rect, items, state);
    let output = ui.end_frame();
    (response, output)
}

#[test]
fn list_scrolls_with_the_wheel_only_under_the_pointer() {
    let mut ui = Ui::new(VIEWPORT);
    let id = ui.id("list");
    let items = rows(20);
    let mut state = ListState::new();

    let (response, output) = run_list(
        &mut ui,
        id,
        LIST_RECT,
        &items,
        &mut state,
        wheel([40.0, 40.0], 1.0),
    );
    assert!(response.hovered);
    assert!(response.changed);
    assert!(output.responses[0].changed);
    assert!((state.offset() - ROW_HEIGHT).abs() < 0.01);

    let (response, _) = run_list(
        &mut ui,
        id,
        LIST_RECT,
        &items,
        &mut state,
        wheel([500.0, 300.0], 5.0),
    );
    assert!(!response.hovered);
    assert!(!response.changed);
    assert!((state.offset() - ROW_HEIGHT).abs() < 0.01);

    // Scrolling back above the top clamps at zero.
    run_list(
        &mut ui,
        id,
        LIST_RECT,
        &items,
        &mut state,
        wheel([40.0, 40.0], -100.0),
    );
    assert_eq!(state.offset(), 0.0);
}

#[test]
fn list_moves_the_selection_with_the_arrow_keys() {
    let mut ui = Ui::new(VIEWPORT);
    let id = ui.id("list");
    let items = rows(20);
    let mut state = ListState::new();

    let (response, _) = run_list(
        &mut ui,
        id,
        LIST_RECT,
        &items,
        &mut state,
        click([40.0, 30.0]),
    );
    assert!(response.clicked && response.focused);
    assert_eq!(state.selected(), Some(0));

    for _ in 0..6 {
        run_list(
            &mut ui,
            id,
            LIST_RECT,
            &items,
            &mut state,
            key(Key::ArrowDown),
        );
    }
    assert_eq!(state.selected(), Some(6));
    assert!(
        state.offset() > 0.0,
        "the selected row is scrolled into view"
    );
    assert!((state.offset() - 68.0).abs() < 0.01);

    run_list(
        &mut ui,
        id,
        LIST_RECT,
        &items,
        &mut state,
        key(Key::ArrowUp),
    );
    assert_eq!(state.selected(), Some(5));

    for _ in 0..5 {
        run_list(
            &mut ui,
            id,
            LIST_RECT,
            &items,
            &mut state,
            key(Key::ArrowDown),
        );
    }
    assert_eq!(state.selected(), Some(10));
    let top = state.selected().unwrap_or(0) as f32 * ROW_HEIGHT;
    assert!(state.offset() <= top);
    assert!(top + ROW_HEIGHT <= state.offset() + LIST_RECT.height + 0.01);

    // Up from no selection selects the last row and scrolls to it.
    let mut fresh = ListState::new();
    let (_, _) = run_list(
        &mut ui,
        id,
        LIST_RECT,
        &items,
        &mut fresh,
        click([40.0, 30.0]),
    );
    fresh.set_selected(None);
    run_list(
        &mut ui,
        id,
        LIST_RECT,
        &items,
        &mut fresh,
        key(Key::ArrowUp),
    );
    assert_eq!(fresh.selected(), Some(19));
    assert!((fresh.offset() - (20.0 * ROW_HEIGHT - LIST_RECT.height)).abs() < 0.01);
}

#[test]
fn list_selects_a_row_by_click() {
    let mut ui = Ui::new(VIEWPORT);
    let id = ui.id("list");
    let items = rows(3);
    let mut state = ListState::new();

    let row_y = LIST_RECT.y + 2.0 * ROW_HEIGHT + 5.0;
    let (response, output) = run_list(
        &mut ui,
        id,
        LIST_RECT,
        &items,
        &mut state,
        click([40.0, row_y]),
    );
    assert!(response.clicked);
    assert!(response.changed);
    assert!(output.responses[0].changed);
    assert_eq!(state.selected(), Some(2));

    // A click below the last row does not invent a selection.
    let (response, _) = run_list(
        &mut ui,
        id,
        LIST_RECT,
        &items,
        &mut state,
        click([40.0, LIST_RECT.y + 80.0]),
    );
    assert!(response.clicked);
    assert!(!response.changed);
    assert_eq!(state.selected(), Some(2));

    // A click outside the list neither selects nor clears.
    let (response, _) = run_list(
        &mut ui,
        id,
        LIST_RECT,
        &items,
        &mut state,
        click([500.0, 300.0]),
    );
    assert!(!response.clicked);
    assert!(!response.changed);
    assert_eq!(state.selected(), Some(2));
}

#[test]
fn list_clamps_the_offset_and_selection_after_item_or_size_changes() {
    let mut ui = Ui::new(VIEWPORT);
    let id = ui.id("list");
    let long = rows(20);
    let mut state = ListState::new();

    run_list(
        &mut ui,
        id,
        LIST_RECT,
        &long,
        &mut state,
        wheel([40.0, 40.0], 100.0),
    );
    let max_offset = 20.0 * ROW_HEIGHT - LIST_RECT.height;
    assert!((state.offset() - max_offset).abs() < 0.01);

    state.select(10);
    let short = rows(5);
    let (response, _) = run_list(
        &mut ui,
        id,
        LIST_RECT,
        &short,
        &mut state,
        UiInput::default(),
    );
    assert!(response.changed, "the shrink clamped the state");
    assert_eq!(state.selected(), None, "the stale selection was dropped");
    assert!((state.offset() - (5.0 * ROW_HEIGHT - LIST_RECT.height)).abs() < 0.01);

    // Growing the viewport above the content height resets the offset.
    let tall = Rect::new(LIST_RECT.x, LIST_RECT.y, LIST_RECT.width, 500.0);
    let (response, _) = run_list(&mut ui, id, tall, &short, &mut state, UiInput::default());
    assert!(response.changed);
    assert_eq!(state.offset(), 0.0);
}

#[test]
fn list_clips_its_rows_and_culls_the_rest() {
    let mut ui = Ui::new(VIEWPORT);
    let id = ui.id("list");
    let items = rows(20);
    let mut state = ListState::new();

    let (_, output) = run_list(
        &mut ui,
        id,
        LIST_RECT,
        &items,
        &mut state,
        UiInput::default(),
    );
    assert_eq!(push_clips(&output), 1);
    assert_eq!(pop_clips(&output), 1);
    assert_eq!(output.draw_list.clip_depth(), 0);

    let clip = clip_rect(&output).expect("the list pushes a clip");
    assert_eq!(clip.x, LIST_RECT.x);
    assert_eq!(clip.y, LIST_RECT.y);
    assert_eq!(clip.height, LIST_RECT.height);
    assert!(clip.width < LIST_RECT.width, "the scrollbar is excluded");
    assert_eq!(
        text_count(&output, "ROW "),
        5,
        "only visible rows are drawn"
    );
    assert!(text_run(&output, "ROW 0").is_some());
    assert!(text_run(&output, "ROW 19").is_none());

    // The clip command precedes every row text, and the pop follows them.
    let commands = output.draw_list.commands();
    let push = commands
        .iter()
        .position(|command| matches!(command, DrawCommand::PushClip { .. }))
        .unwrap_or(0);
    let pop = commands
        .iter()
        .rposition(|command| matches!(command, DrawCommand::PopClip))
        .unwrap_or(0);
    assert!(push < pop);
    assert!(commands[push + 1..pop]
        .iter()
        .all(|command| !matches!(command, DrawCommand::Text { .. })
            || matches!(command, DrawCommand::Text { text, .. } if text.starts_with("ROW "))));

    // At the bottom, only the last rows remain.
    run_list(
        &mut ui,
        id,
        LIST_RECT,
        &items,
        &mut state,
        wheel([40.0, 40.0], 100.0),
    );
    let (_, output) = run_list(
        &mut ui,
        id,
        LIST_RECT,
        &items,
        &mut state,
        UiInput::default(),
    );
    assert_eq!(text_count(&output, "ROW "), 5);
    assert!(text_run(&output, "ROW 0").is_none());
    assert!(text_run(&output, "ROW 19").is_some());
}

// ----------------------------------------------------------------- panel ----

#[test]
fn panel_fills_and_returns_its_inset_content() {
    let mut ui = Ui::new(VIEWPORT);
    let rect = Rect::new(10.0, 20.0, 100.0, 50.0);
    let color = [0.1, 0.2, 0.3, 0.9];

    ui.begin_frame(VIEWPORT, UiInput::default());
    let content = panel(&mut ui, rect, color);
    let output = ui.end_frame();

    assert_eq!(content, rect.inset(PANEL_INSET));
    assert_eq!(rect_color(&output, rect), Some(color));
    assert_eq!(output.draw_list.len(), 1);
    assert!(output.responses.is_empty());
}

// ---------------------------------------------------------- debug screen ----

/// Draws one debug screen frame and returns the output.
fn run_screen(
    ui: &mut Ui,
    screen: &mut DebugScreen,
    atlas: &FontAtlas,
    input: UiInput,
) -> UiOutput {
    let viewport = ui.viewport();
    ui.begin_frame(viewport, input);
    screen.show(ui, atlas);
    ui.end_frame()
}

#[test]
fn debug_screen_runs_the_scripted_acceptance_sequence() {
    let atlas = FontAtlas::fallback();
    let mut ui = Ui::new(VIEWPORT);
    let mut screen = DebugScreen::new();

    assert_eq!(screen.text(), "");
    assert_eq!(screen.caret(), 0);
    assert_eq!(screen.click_count(), 0);
    assert_eq!(screen.selection(), None);

    let first = run_screen(&mut ui, &mut screen, &atlas, UiInput::default());
    assert!(!first.draw_list.is_empty());
    assert_eq!(first.draw_list.clip_depth(), 0);
    assert_eq!(push_clips(&first), pop_clips(&first));
    assert!(!screen.field_rect().is_empty());
    assert!(!screen.button_rect().is_empty());
    assert!(!screen.list_rect().is_empty());
    assert!(screen.button_rect().bottom() <= screen.list_rect().y);
    assert!(screen.field_rect().bottom() <= screen.button_rect().y);

    // Focus the field and type.
    let field_rect = screen.field_rect();
    run_screen(&mut ui, &mut screen, &atlas, click(center(field_rect)));
    run_screen(&mut ui, &mut screen, &atlas, typed("hello"));
    assert_eq!(screen.text(), "hello");
    assert_eq!(screen.caret(), 5);

    // Backspace and move the caret.
    run_screen(&mut ui, &mut screen, &atlas, key(Key::Backspace));
    assert_eq!(screen.text(), "hell");
    assert_eq!(screen.caret(), 4);
    run_screen(&mut ui, &mut screen, &atlas, key(Key::ArrowLeft));
    assert_eq!(screen.caret(), 3);

    // Click the button.
    let button_rect = screen.button_rect();
    let clicked = run_screen(&mut ui, &mut screen, &atlas, click(center(button_rect)));
    assert_eq!(screen.click_count(), 1);
    assert_eq!(clicked.draw_list.clip_depth(), 0);

    // Typing while the button is focused must not edit the field.
    run_screen(&mut ui, &mut screen, &atlas, typed("zzz"));
    assert_eq!(screen.text(), "hell");
    assert_eq!(screen.caret(), 3);

    // Select a list row and scroll the list.
    let list_rect = screen.list_rect();
    let row = [list_rect.x + 20.0, list_rect.y + ROW_HEIGHT + 5.0];
    run_screen(&mut ui, &mut screen, &atlas, click(row));
    assert_eq!(screen.selection(), Some(1));

    let scrolled = run_screen(&mut ui, &mut screen, &atlas, wheel(center(list_rect), 3.0));
    assert!(screen.scroll_offset() > 0.0);

    // The list clip is balanced and wraps the row text.
    assert_eq!(push_clips(&scrolled), pop_clips(&scrolled));
    assert_eq!(scrolled.draw_list.clip_depth(), 0);
    let commands = scrolled.draw_list.commands();
    let list_clip = commands.iter().position(|command| match command {
        DrawCommand::PushClip { rect } => {
            rect.x == list_rect.x
                && rect.y == list_rect.y
                && rect.height == list_rect.height
                && rect.width < list_rect.width
        }
        _ => false,
    });
    let list_clip = list_clip.expect("the list pushes its viewport clip");
    let after = &commands[list_clip + 1..];
    let closes = after
        .iter()
        .position(|command| matches!(command, DrawCommand::PopClip))
        .expect("the list clip is popped");
    assert!(after[..closes]
        .iter()
        .any(|command| matches!(command, DrawCommand::Text { .. })));

    // A click on the empty title area does not count as a button click.
    let field_rect = screen.field_rect();
    run_screen(
        &mut ui,
        &mut screen,
        &atlas,
        click([field_rect.x + 4.0, field_rect.y - 20.0]),
    );
    assert_eq!(screen.click_count(), 1);
    assert_eq!(screen.text(), "hell");
    assert_eq!(screen.caret(), 3);
    assert_eq!(screen.selection(), Some(1));
    assert!(screen.scroll_offset() > 0.0);
}

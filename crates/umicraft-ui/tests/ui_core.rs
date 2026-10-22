// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration tests for the immediate-mode core: layout, focus, hit testing,
//! ID stability and the frame/draw-list lifecycle.

use umicraft_ui::{
    Axis, DrawCommand, InputEvent, Insets, Key, Layout, Modifiers, PointerButton, Rect, ScrollArea,
    TextStyle, Ui, UiInput, WidgetId,
};

fn viewport() -> Rect {
    Rect::new(0.0, 0.0, 640.0, 360.0)
}

fn click(position: [f32; 2]) -> UiInput {
    UiInput::new(vec![
        InputEvent::PointerMoved { position },
        InputEvent::PointerPressed {
            position,
            button: PointerButton::Left,
        },
    ])
}

fn tab(shift: bool) -> UiInput {
    UiInput {
        events: vec![InputEvent::KeyPressed(Key::Tab)],
        modifiers: Modifiers {
            shift,
            ..Modifiers::default()
        },
    }
}

#[test]
fn vertical_layout_applies_padding_rows_and_empty_children() {
    let panel = Rect::new(20.0, 30.0, 300.0, 200.0);
    let content = panel.deflate(Insets::new(8.0, 12.0, 8.0, 16.0));
    assert_eq!(content, Rect::new(28.0, 42.0, 284.0, 172.0));

    let mut column = Layout::vertical(content).with_spacing(6.0);
    assert_eq!(column.axis(), Axis::Vertical);
    assert_eq!(column.next_fixed(24.0), Rect::new(28.0, 42.0, 284.0, 24.0));
    assert_eq!(column.next(100.0, 40.0), Rect::new(28.0, 72.0, 100.0, 40.0));
    // Zero-height children neither advance the cursor nor add a gap.
    assert_eq!(column.next(500.0, 0.0), Rect::new(28.0, 112.0, 284.0, 0.0));
    assert_eq!(column.next_remaining(), Rect::new(28.0, 118.0, 284.0, 96.0));
    assert!(column.remaining().is_empty());
}

#[test]
fn horizontal_layout_produces_exact_columns() {
    let mut row = Layout::horizontal(Rect::new(10.0, 20.0, 300.0, 48.0)).with_spacing(5.0);
    assert_eq!(row.axis(), Axis::Horizontal);
    assert_eq!(row.next(80.0, 999.0), Rect::new(10.0, 20.0, 80.0, 48.0));
    assert_eq!(row.next(120.0, 24.0), Rect::new(95.0, 20.0, 120.0, 24.0));
    assert_eq!(row.remaining(), Rect::new(215.0, 20.0, 95.0, 48.0));
    assert_eq!(row.next_remaining(), Rect::new(220.0, 20.0, 90.0, 48.0));
}

#[test]
fn scroll_viewport_clamps_and_clips_rows() {
    let mut scroll = ScrollArea::new(Rect::new(0.0, 10.0, 100.0, 50.0));
    scroll.set_content_height(200.0);
    assert_eq!(scroll.max_offset(), 150.0);

    scroll.set_offset(-25.0);
    assert_eq!(scroll.offset(), 0.0);
    scroll.scroll_by(30.0);
    assert_eq!(scroll.offset(), 30.0);
    assert_eq!(scroll.content_rect(), Rect::new(0.0, -20.0, 100.0, 200.0));

    let mut rows = scroll.content_layout().with_spacing(2.0);
    let first = rows.next_fixed(20.0);
    let second = rows.next_fixed(20.0);
    let third = rows.next_fixed(20.0);
    let fourth = rows.next_fixed(20.0);
    let fifth = rows.next_fixed(20.0);
    assert_eq!(first, Rect::new(0.0, -20.0, 100.0, 20.0));
    assert_eq!(second, Rect::new(0.0, 2.0, 100.0, 20.0));
    assert_eq!(third, Rect::new(0.0, 24.0, 100.0, 20.0));
    assert_eq!(fourth, Rect::new(0.0, 46.0, 100.0, 20.0));
    assert_eq!(fifth, Rect::new(0.0, 68.0, 100.0, 20.0));

    assert!(!scroll.is_visible(first));
    assert!(scroll.is_visible(second));
    assert!(scroll.is_visible(third));
    assert!(scroll.is_visible(fourth));
    assert!(!scroll.is_visible(fifth));
    assert_eq!(
        scroll.visible_rect(fourth),
        Some(Rect::new(0.0, 46.0, 100.0, 14.0))
    );
    assert_eq!(scroll.visible_rect(fifth), None);

    scroll.set_content_height(20.0);
    assert_eq!(scroll.max_offset(), 0.0);
    assert_eq!(scroll.offset(), 0.0);
}

#[test]
fn focus_survives_frames_and_tab_wraps_both_ways() {
    let viewport = viewport();
    let mut ui = Ui::new(viewport);
    let ids = [
        WidgetId::from_label("first"),
        WidgetId::from_label("second"),
        WidgetId::from_label("third"),
    ];
    let rects = [
        Rect::new(0.0, 0.0, 100.0, 20.0),
        Rect::new(0.0, 30.0, 100.0, 20.0),
        Rect::new(0.0, 60.0, 100.0, 20.0),
    ];

    ui.begin_frame(viewport, UiInput::default());
    let first_frame = [
        ui.response(ids[0], rects[0]),
        ui.response(ids[1], rects[1]),
        ui.response(ids[2], rects[2]),
    ];
    assert!(first_frame.iter().all(|response| !response.focused));
    let output = ui.end_frame();
    assert_eq!(output.focus, None);

    ui.begin_frame(viewport, tab(false));
    let second = [
        ui.response(ids[0], rects[0]),
        ui.response(ids[1], rects[1]),
        ui.response(ids[2], rects[2]),
    ];
    assert!(second[0].focused);
    assert!(!second[1].focused);
    assert!(!second[2].focused);
    let output = ui.end_frame();
    assert_eq!(output.focus, Some(ids[0]));

    ui.begin_frame(viewport, tab(false));
    let third = [
        ui.response(ids[0], rects[0]),
        ui.response(ids[1], rects[1]),
        ui.response(ids[2], rects[2]),
    ];
    assert!(!third[0].focused);
    assert!(third[1].focused);
    ui.end_frame();

    // The focus is still on the second widget on a frame with no input.
    ui.begin_frame(viewport, UiInput::default());
    let idle = [
        ui.response(ids[0], rects[0]),
        ui.response(ids[1], rects[1]),
        ui.response(ids[2], rects[2]),
    ];
    assert!(idle[1].focused);
    ui.end_frame();

    ui.begin_frame(viewport, tab(true));
    let backward = [
        ui.response(ids[0], rects[0]),
        ui.response(ids[1], rects[1]),
        ui.response(ids[2], rects[2]),
    ];
    assert!(backward[0].focused);
    ui.end_frame();

    ui.begin_frame(viewport, tab(true));
    let wrapped = [
        ui.response(ids[0], rects[0]),
        ui.response(ids[1], rects[1]),
        ui.response(ids[2], rects[2]),
    ];
    assert!(wrapped[2].focused);
    let output = ui.end_frame();
    assert_eq!(output.focus, Some(ids[2]));
}

#[test]
fn clicks_focus_and_an_empty_click_clears() {
    let viewport = viewport();
    let mut ui = Ui::new(viewport);
    let first = WidgetId::from_label("first");
    let second = WidgetId::from_label("second");
    let first_rect = Rect::new(10.0, 10.0, 80.0, 30.0);
    let second_rect = Rect::new(10.0, 60.0, 80.0, 30.0);

    ui.begin_frame(viewport, click([first_rect.x + 2.0, first_rect.y + 2.0]));
    assert!(ui.response(first, first_rect).clicked);
    assert!(!ui.response(second, second_rect).clicked);
    let output = ui.end_frame();
    assert_eq!(output.focus, Some(first));

    ui.begin_frame(viewport, click([second_rect.x + 2.0, second_rect.y + 2.0]));
    assert!(!ui.response(first, first_rect).clicked);
    assert!(ui.response(second, second_rect).clicked);
    let output = ui.end_frame();
    assert_eq!(output.focus, Some(second));

    ui.begin_frame(viewport, click([400.0, 300.0]));
    let _ = ui.response(first, first_rect);
    let _ = ui.response(second, second_rect);
    let output = ui.end_frame();
    assert_eq!(output.focus, None);
    assert!(output.responses.iter().all(|response| !response.focused));
}

#[test]
fn programmatic_focus_applies_to_the_next_frame() {
    let viewport = viewport();
    let mut ui = Ui::new(viewport);
    let field = WidgetId::from_label("field");
    let other = WidgetId::from_label("other");
    let field_rect = Rect::new(0.0, 0.0, 100.0, 20.0);
    let other_rect = Rect::new(0.0, 40.0, 100.0, 20.0);

    ui.set_focus(Some(field));
    ui.begin_frame(viewport, UiInput::default());
    assert!(ui.response(field, field_rect).focused);
    assert!(!ui.response(other, other_rect).focused);
    ui.end_frame();

    ui.begin_frame(viewport, UiInput::default());
    assert!(ui.response(field, field_rect).focused);
    let output = ui.end_frame();
    assert_eq!(output.focus, Some(field));

    ui.set_focus(None);
    ui.begin_frame(viewport, UiInput::default());
    assert!(!ui.response(field, field_rect).focused);
    let output = ui.end_frame();
    assert_eq!(output.focus, None);
}

#[test]
fn ids_are_stable_across_frames_and_distinct_across_scopes() {
    let viewport = viewport();
    let mut ui = Ui::new(viewport);
    let mut previous: Option<(WidgetId, WidgetId)> = None;

    for _ in 0..3 {
        ui.begin_frame(viewport, UiInput::default());
        ui.push_id("panel");
        let item = ui.id("item");
        ui.push_id("row");
        let cell = ui.id("cell");
        ui.pop_id();
        ui.pop_id();

        if let Some((previous_item, previous_cell)) = previous {
            assert_eq!(item, previous_item);
            assert_eq!(cell, previous_cell);
        } else {
            previous = Some((item, cell));
        }
        assert_ne!(item, cell);
        assert_ne!(item, ui.id("item"));
        assert_eq!(cell, ui.id("panel").child("row").child("cell"));
        assert_eq!(item, WidgetId::from_label("panel").child("item"));
        ui.end_frame();
    }
}

#[test]
fn hit_testing_respects_viewport_overlap_and_clips() {
    let viewport = viewport();
    let mut ui = Ui::new(viewport);
    let first = WidgetId::from_label("first");
    let second = WidgetId::from_label("second");
    let rect = Rect::new(0.0, 0.0, 200.0, 200.0);

    ui.begin_frame(viewport, click([50.0, 50.0]));
    let hit = ui.response(first, rect);
    assert!(hit.hovered);
    assert!(hit.clicked);
    // The first widget claimed the press; the overlapping one does not.
    assert!(!ui.response(second, rect).clicked);
    ui.end_frame();

    // A press outside the viewport never reaches a widget.
    ui.begin_frame(viewport, click([700.0, 400.0]));
    let outside = Rect::new(600.0, 300.0, 100.0, 100.0);
    assert!(!ui.response(first, outside).hovered);
    assert!(!ui.response(first, outside).clicked);
    let output = ui.end_frame();
    // A press outside the viewport is not a click on the UI background.
    assert_eq!(output.focus, Some(first));

    // A press inside a widget but outside its active clip is ignored.
    ui.begin_frame(viewport, click([150.0, 50.0]));
    ui.push_clip(Rect::new(0.0, 0.0, 100.0, 100.0));
    assert!(!ui.response(first, rect).hovered);
    assert!(!ui.response(first, rect).clicked);
    ui.pop_clip();
    let hit = ui.response(second, rect);
    assert!(hit.hovered);
    assert!(hit.clicked);
    let output = ui.end_frame();
    assert_eq!(output.focus, Some(second));

    // Nested clips intersect: the inner 30..40 square is the only hit area.
    ui.begin_frame(viewport, click([35.0, 35.0]));
    ui.push_clip(Rect::new(0.0, 0.0, 40.0, 40.0));
    ui.push_clip(Rect::new(30.0, 30.0, 40.0, 40.0));
    let hit = ui.response(first, rect);
    assert!(hit.hovered);
    assert!(hit.clicked);
    ui.pop_clip();
    ui.pop_clip();
    ui.end_frame();

    // A clip pushed directly on the draw list also constrains hit testing.
    ui.begin_frame(viewport, click([35.0, 35.0]));
    ui.draw().push_clip(Rect::new(0.0, 0.0, 30.0, 30.0));
    assert!(!ui.response(first, rect).hovered);
    ui.draw().pop_clip();
    let hit = ui.response(first, rect);
    assert!(hit.hovered);
    assert!(hit.clicked);
    ui.end_frame();
}

#[test]
fn draw_list_keeps_order_and_clips_balanced() {
    let viewport = viewport();
    let mut ui = Ui::new(viewport);

    ui.begin_frame(viewport, UiInput::default());
    ui.draw()
        .rect(Rect::new(1.0, 2.0, 3.0, 4.0), [1.0, 0.0, 0.0, 1.0]);
    ui.push_clip(Rect::new(10.0, 10.0, 50.0, 50.0));
    ui.draw()
        .text([12.0, 14.0], "hello", TextStyle::new(1.0, [1.0; 4]));
    ui.pop_clip();
    let output = ui.end_frame();

    assert_eq!(output.draw_list.len(), 4);
    assert_eq!(output.draw_list.clip_depth(), 0);
    assert!(matches!(
        output.draw_list.commands()[0],
        DrawCommand::Rect { .. }
    ));
    assert!(matches!(
        output.draw_list.commands()[1],
        DrawCommand::PushClip { rect } if rect == Rect::new(10.0, 10.0, 50.0, 50.0)
    ));
    assert!(matches!(
        &output.draw_list.commands()[2],
        DrawCommand::Text { text, .. } if text == "hello"
    ));
    assert!(matches!(
        output.draw_list.commands()[3],
        DrawCommand::PopClip
    ));
}

#[test]
fn end_frame_closes_unbalanced_direct_clips() {
    let viewport = viewport();
    let mut ui = Ui::new(viewport);

    ui.begin_frame(viewport, UiInput::default());
    ui.draw().push_clip(Rect::new(0.0, 0.0, 100.0, 100.0));
    ui.draw().push_clip(Rect::new(10.0, 10.0, 20.0, 20.0));
    let output = ui.end_frame();

    let pushes = output
        .draw_list
        .commands()
        .iter()
        .filter(|command| matches!(command, DrawCommand::PushClip { .. }))
        .count();
    let pops = output
        .draw_list
        .commands()
        .iter()
        .filter(|command| matches!(command, DrawCommand::PopClip))
        .count();
    assert_eq!(pushes, 2);
    assert_eq!(pops, 2);
    assert_eq!(output.draw_list.clip_depth(), 0);

    // The next frame starts from a clean list, not from the closed scopes.
    ui.begin_frame(viewport, UiInput::default());
    assert!(ui.draw().is_empty());
    assert_eq!(ui.draw().clip_depth(), 0);
    ui.end_frame();
}

#[test]
fn frames_reset_responses_and_reuse_the_draw_list() {
    let viewport = viewport();
    let mut ui = Ui::new(viewport);
    let id = WidgetId::from_label("button");
    let rect = Rect::new(0.0, 0.0, 100.0, 20.0);

    ui.begin_frame(viewport, UiInput::default());
    ui.draw().rect(rect, [1.0; 4]);
    let first = ui.response(id, rect);
    assert!(!first.clicked);
    let first_output = ui.end_frame();
    assert_eq!(first_output.responses.len(), 1);
    assert_eq!(first_output.draw_list.len(), 1);

    ui.begin_frame(viewport, UiInput::default());
    assert!(ui.draw().is_empty());
    assert_eq!(ui.focus(), None);
    let second = ui.response(id, rect);
    assert!(!second.clicked);
    let second_output = ui.end_frame();
    assert_eq!(second_output.responses.len(), 1);
    assert!(second_output.draw_list.is_empty());
}

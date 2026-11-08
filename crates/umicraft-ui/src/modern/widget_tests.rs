// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the toggle, the slider, the tabs and the search box: their pure
//! logic first, then one frame each.

use super::search_field::{edit, MAX_CHARS};
use super::*;
use crate::geom::Rect;
use crate::input::{InputEvent, Key, PointerButton, UiInput};
use crate::text::FontAtlas;
use crate::Ui;

const VIEWPORT: Rect = Rect::new(0.0, 0.0, 1280.0, 720.0);
const RANGE: SliderRange = SliderRange {
    min: 1.0,
    max: 3.0,
    step: 0.5,
};

fn frame<R>(events: Vec<InputEvent>, f: impl FnOnce(&mut Ui) -> R) -> R {
    let mut ui = Ui::new(VIEWPORT);
    ui.begin_frame(VIEWPORT, UiInput::new(events));
    let result = f(&mut ui);
    ui.end_frame();
    result
}

fn press(position: [f32; 2]) -> Vec<InputEvent> {
    vec![
        InputEvent::PointerMoved { position },
        InputEvent::PointerPressed {
            position,
            button: PointerButton::Left,
        },
    ]
}

#[test]
fn the_knob_sits_at_the_end_of_the_track_that_matches_the_state() {
    let track = Rect::new(10.0, 20.0, 48.0, 24.0);
    let (off, on) = (knob_rect(track, false), knob_rect(track, true));
    assert!(off.x < on.x);
    assert!(off.x >= track.x && on.right() <= track.right());
    assert!((off.width - off.height).abs() < 1e-4, "the knob is square");
    assert!(off.y >= track.y && off.bottom() <= track.bottom());
}

#[test]
fn a_click_on_the_track_asks_for_the_other_state() {
    let track = Rect::new(100.0, 100.0, 48.0, 24.0);
    let inside = [track.x + 5.0, track.y + 5.0];
    let asked = |on: bool, events| {
        frame(events, |ui| {
            let id = ui.id("t");
            toggle(ui, id, track, on, 0.0)
        })
    };
    assert_eq!(asked(false, press(inside)), Some(true));
    assert_eq!(asked(true, press(inside)), Some(false));
    assert_eq!(asked(true, press([0.0, 0.0])), None);
    assert_eq!(asked(true, vec![]), None);
}

#[test]
fn a_slider_value_is_clamped_and_snapped_to_its_step() {
    assert_eq!(RANGE.snap(9.0), 3.0);
    assert_eq!(RANGE.snap(-9.0), 1.0);
    assert_eq!(RANGE.snap(1.8), 2.0);
    assert_eq!(RANGE.snap(1.7), 1.5);
    assert_eq!(RANGE.snap(f32::NAN), 1.0);
}

#[test]
fn the_pointer_position_maps_to_a_value_on_the_track() {
    assert_eq!(RANGE.value_at(100.0, 200.0, 100.0), 1.0);
    assert_eq!(RANGE.value_at(100.0, 200.0, 150.0), 2.0);
    assert_eq!(RANGE.value_at(100.0, 200.0, 999.0), 3.0);
    assert_eq!(RANGE.value_at(100.0, 200.0, -50.0), 1.0);
    assert_eq!(RANGE.fraction(2.0), 0.5);
}

#[test]
fn a_slider_follows_a_drag_even_outside_of_it_and_stops_on_release() {
    let rect = Rect::new(100.0, 100.0, 200.0, 20.0);
    let mut drag = SliderDrag::default();
    let mut run = |events: Vec<InputEvent>, value: f32| {
        frame(events, |ui| {
            let id = ui.id("s");
            modern_slider(ui, id, rect, RANGE, value, &mut drag)
        })
    };
    assert_eq!(run(press([200.0, 110.0]), 1.0), Some(2.0));
    let far = vec![InputEvent::PointerMoved {
        position: [5000.0, 500.0],
    }];
    assert_eq!(run(far, 2.0), Some(3.0));
    let release = vec![InputEvent::PointerReleased {
        position: [0.0, 0.0],
        button: PointerButton::Left,
    }];
    assert_eq!(run(release, 3.0), None);
    let moved = vec![InputEvent::PointerMoved {
        position: [100.0, 110.0],
    }];
    assert_eq!(run(moved, 3.0), None, "the drag is over");
}

#[test]
fn tabs_sit_side_by_side_and_are_found_by_position() {
    let area = Rect::new(10.0, 10.0, 300.0, 20.0);
    let rects = tab_rects(area, &[40.0, 60.0], 5.0);
    assert_eq!(rects[0], Rect::new(10.0, 10.0, 40.0, 20.0));
    assert_eq!(rects[1], Rect::new(55.0, 10.0, 60.0, 20.0));
    assert_eq!(tab_at(&rects, [12.0, 12.0]), Some(0));
    assert_eq!(
        tab_at(&rects, [52.0, 12.0]),
        None,
        "the gap belongs to none"
    );
    assert_eq!(tab_at(&rects, [100.0, 29.0]), Some(1));
    assert_eq!(tab_at(&rects, [100.0, 40.0]), None);
}

#[test]
fn a_click_on_a_tab_selects_it() {
    let area = Rect::new(100.0, 100.0, 600.0, 36.0);
    let brand = FontAtlas::fallback();
    let labels = ["All", "HUD"];
    let click_on_second = {
        let size = theme::TEXT_SIZE_SMALL * 3.0;
        let first = measure_text(&brand, "All", size) + 30.0;
        press([area.x + first + 6.0 + 8.0, area.y + 10.0])
    };
    let clicked = frame(click_on_second, |ui| {
        tabs(ui, &brand, area, &labels, 0, 3.0)
    });
    assert_eq!(clicked, Some(1));
    assert_eq!(
        frame(vec![], |ui| tabs(ui, &brand, area, &labels, 0, 3.0)),
        None
    );
}

#[test]
fn typing_appends_and_backspace_removes() {
    let mut text = String::from("fp");
    assert!(edit(&mut text, &[InputEvent::Text("s".into())]));
    assert_eq!(text, "fps");
    assert!(edit(&mut text, &[InputEvent::KeyPressed(Key::Backspace)]));
    assert_eq!(text, "fp");
    assert!(!edit(
        &mut String::new(),
        &[InputEvent::KeyPressed(Key::Backspace)]
    ));
}

#[test]
fn a_query_is_limited_and_has_no_control_characters() {
    let mut text = String::new();
    assert!(edit(&mut text, &[InputEvent::Paste("a\nb".into())]));
    assert_eq!(text, "ab");
    let long = "x".repeat(MAX_CHARS * 2);
    edit(&mut text, &[InputEvent::Text(long)]);
    assert_eq!(text.chars().count(), MAX_CHARS);
    assert!(!edit(&mut text, &[InputEvent::Text("y".into())]));
}

#[test]
fn long_text_is_cut_with_an_ellipsis_and_short_text_is_kept() {
    let brand = FontAtlas::fallback();
    let size = 21.0;
    assert_eq!(fit_text(&brand, "FPS", size, 1000.0), "FPS");
    let text = "A very long description of a mod";
    let max = measure_text(&brand, text, size) * 0.6;
    let cut = fit_text(&brand, text, size, max);
    assert!(cut.ends_with("..."), "{cut}");
    assert!(measure_text(&brand, &cut, size) <= max);
    assert_eq!(fit_text(&brand, "anything", size, 1.0), "");
}

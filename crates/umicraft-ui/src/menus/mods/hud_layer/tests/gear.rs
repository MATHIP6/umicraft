// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the gear that opens the settings of a widget's mod.

use super::*;

use crate::menus::mods::hud_layer::frame::frame_rect;
use crate::menus::mods::hud_layer::gear::gear_rect;

#[test]
fn the_gear_sits_in_a_strip_under_the_text_inside_the_frame() {
    let widget = Rect::new(100.0, 100.0, 50.0, 20.0);
    let frame = frame_rect(widget, SCREEN);
    let gear = gear_rect(widget, SCREEN);
    // The frame is a little larger than the text, and taller by the strip.
    assert!(frame.x < widget.x && frame.right() > widget.right());
    assert!(frame.y < widget.y && frame.bottom() > widget.bottom());
    // The gear never covers the text: it is under the last line, at the left.
    assert!(gear.y >= widget.bottom(), "{gear:?}");
    assert!(gear.x >= frame.x && gear.right() <= frame.right());
    assert!(gear.bottom() <= frame.bottom());
    assert!(gear.x < frame.x + gear.width, "{gear:?}");
}

#[test]
fn the_gear_is_small_and_follows_the_box_above_a_floor() {
    let one_line = Rect::new(0.0, 0.0, 40.0, 12.0);
    assert_eq!(gear_rect(one_line, SCREEN).width, 4.0, "the floor");
    let big = Rect::new(0.0, 0.0, 200.0, 100.0);
    assert!((gear_rect(big, SCREEN).width - 0.16 * 100.0).abs() < 1e-4);
    let bigger = Rect::new(0.0, 0.0, 400.0, 200.0);
    assert!((gear_rect(bigger, SCREEN).width - 2.0 * gear_rect(big, SCREEN).width).abs() < 1e-4);
    assert!(gear_rect(Rect::new(0.0, 0.0, 60.0, 40.0), SCREEN).width < 8.0);
}

#[test]
fn clicking_the_gear_of_a_hovered_widget_asks_for_its_settings() {
    let view = view();
    let gear = gear_rect(rect_of(&view, 1), SCREEN);
    let mut screen = HudLayer::default();
    // The pointer comes onto the widget first, then goes up to the gear.
    show(
        &mut screen,
        &view,
        UiInput::new(vec![moved(middle(rect_of(&view, 1)))]),
    );
    let at = middle(gear);
    let outcome = show(
        &mut screen,
        &view,
        UiInput::new(vec![moved(at), pressed(at)]),
    );
    assert_eq!(
        outcome,
        Some(HudEditorOutcome::OpenSettings { id: "cps".into() })
    );
    assert!(screen.drag.is_none(), "the press did not start a drag");
}

#[test]
fn the_gear_of_a_selected_widget_stays_while_the_pointer_is_elsewhere() {
    let view = view();
    let mut screen = HudLayer::default();
    let on = middle(rect_of(&view, 0));
    show(&mut screen, &view, UiInput::new(vec![pressed(on)]));
    show(&mut screen, &view, UiInput::new(vec![released(on)]));
    let gear = gear_rect(rect_of(&view, 0), SCREEN);
    let far = [5.0, 200.0];
    show(&mut screen, &view, UiInput::new(vec![moved(far)]));
    let at = middle(gear);
    assert_eq!(
        show(
            &mut screen,
            &view,
            UiInput::new(vec![moved(at), pressed(at), released(at)])
        ),
        Some(HudEditorOutcome::OpenSettings { id: "fps".into() })
    );
}

#[test]
fn a_widget_with_no_pointer_and_no_selection_shows_no_gear() {
    let view = view();
    let screen = HudLayer::default();
    let rects: Vec<Rect> = (0..view.widgets.len()).map(|i| rect_of(&view, i)).collect();
    assert!(screen.gear_targets(None, (&view, &rects)).is_empty());
    assert!(screen
        .gear_targets(Some([5.0, 5.0]), (&view, &rects))
        .is_empty());
}

#[test]
fn no_gear_is_pressable_while_a_widget_is_dragged() {
    let view = view();
    let start = rect_of(&view, 0);
    let grab = middle(start);
    let mut screen = HudLayer::default();
    show(
        &mut screen,
        &view,
        UiInput::new(vec![pressed(grab), moved([grab[0] + 3.0, grab[1] + 3.0])]),
    );
    let rects: Vec<Rect> = (0..view.widgets.len()).map(|i| rect_of(&view, i)).collect();
    assert!(screen.gear_targets(Some(grab), (&view, &rects)).is_empty());
}

#[test]
fn a_widget_at_the_bottom_of_the_screen_gets_its_gear_above_the_text() {
    let low = Rect::new(100.0, SCREEN[1] - 14.0, 50.0, 12.0);
    let frame = frame_rect(low, SCREEN);
    let gear = gear_rect(low, SCREEN);
    assert!(
        frame.bottom() <= SCREEN[1],
        "{frame:?} falls off the screen"
    );
    assert!(gear.bottom() <= low.y, "{gear:?} is above the text");
    assert!(gear.y >= frame.y && gear.x >= frame.x);
    // Higher up, it goes back under the text.
    let high = Rect::new(100.0, 100.0, 50.0, 12.0);
    assert!(gear_rect(high, SCREEN).y >= high.bottom());
}

#[test]
fn the_gear_above_the_text_can_be_pressed() {
    let mut view = view();
    view.widgets[0].widget.position = [100.0, SCREEN[1] - 14.0];
    let text = rect_of(&view, 0);
    let gear = gear_rect(text, SCREEN);
    let at = middle(gear);
    let mut screen = HudLayer::default();
    show(&mut screen, &view, UiInput::new(vec![moved(middle(text))]));
    assert_eq!(
        show(
            &mut screen,
            &view,
            UiInput::new(vec![moved(at), pressed(at), released(at)])
        ),
        Some(HudEditorOutcome::OpenSettings { id: "fps".into() })
    );
}

#[test]
fn the_frame_gear_and_handle_stay_on_the_screen_at_every_edge() {
    let screen = Rect::new(0.0, 0.0, SCREEN[0], SCREEN[1]);
    let inside = |rect: Rect| {
        rect.x >= screen.x - 1e-3
            && rect.y >= screen.y - 1e-3
            && rect.right() <= screen.right() + 1e-3
            && rect.bottom() <= screen.bottom() + 1e-3
    };
    let (w, h) = (50.0, 12.0);
    for (x, y) in [
        (0.0, 0.0),
        (SCREEN[0] - w, 0.0),
        (0.0, SCREEN[1] - h),
        (SCREEN[0] - w, SCREEN[1] - h),
        (SCREEN[0] / 2.0, SCREEN[1] - h),
        (SCREEN[0] - w, SCREEN[1] / 2.0),
    ] {
        let text = Rect::new(x, y, w, h);
        let frame = frame_rect(text, SCREEN);
        assert!(inside(frame), "frame of {text:?}: {frame:?}");
        assert!(inside(gear_rect(text, SCREEN)), "gear of {text:?}");
        assert!(
            inside(crate::menus::mods::hud_layer::drag::handle_rect(frame)),
            "handle of {text:?}"
        );
    }
}

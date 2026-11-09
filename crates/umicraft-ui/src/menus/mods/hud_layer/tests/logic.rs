// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the drawing-free logic: hit tests, scale steps, the drag and the
//! guides.

use super::{snap, SCREEN};
use crate::geom::Rect;
use crate::menus::mods::hud_layer::drag::{
    clamp_into, clamp_scale, handle_rect, hit_widget, on_handle, wheel_resize, Bounds, Drag, Grab,
};
use crate::menus::mods::hud_layer::guides::{guides_for, Guide};

fn bounds() -> Bounds<'static> {
    Bounds {
        screen: SCREEN,
        scale_range: (0.5, 3.0),
        snap: &snap,
    }
}

const BOX: Rect = Rect {
    x: 50.0,
    y: 40.0,
    width: 60.0,
    height: 20.0,
};

#[test]
fn the_topmost_widget_wins_a_hit() {
    let under = Rect::new(0.0, 0.0, 100.0, 100.0);
    let over = Rect::new(50.0, 50.0, 100.0, 100.0);
    assert_eq!(hit_widget(&[under, over], [60.0, 60.0], SCREEN), Some(1));
    assert_eq!(hit_widget(&[under, over], [10.0, 10.0], SCREEN), Some(0));
    assert_eq!(hit_widget(&[under, over], [300.0, 10.0], SCREEN), None);
}

#[test]
fn the_handle_sits_in_the_bottom_right_corner() {
    let handle = handle_rect(BOX);
    assert_eq!(
        (handle.right(), handle.bottom()),
        (BOX.right(), BOX.bottom())
    );
    assert!(on_handle(BOX, [BOX.right() - 1.0, BOX.bottom() - 1.0]));
    assert!(!on_handle(BOX, [BOX.x + 2.0, BOX.y + 2.0]));
    // A little outside the corner still grabs it, and hits the widget.
    assert_eq!(
        hit_widget(&[BOX], [BOX.right() + 1.0, BOX.bottom() + 1.0], SCREEN),
        Some(0)
    );
}

#[test]
fn a_scale_has_no_step_and_is_kept_in_range() {
    assert_eq!(clamp_scale(1.04, (0.5, 3.0)), 1.04);
    assert_eq!(clamp_scale(1.263, (0.5, 3.0)), 1.263);
    assert_eq!(clamp_scale(0.1, (0.5, 3.0)), 0.5);
    assert_eq!(clamp_scale(9.0, (0.5, 3.0)), 3.0);
}

#[test]
fn a_resize_takes_any_scale_between_two_steps() {
    let mut drag = Drag::begin("fps".into(), Grab::Resize(0.0), BOX, 1.0);
    drag.follow([BOX.x + 1.2345 * BOX.width, BOX.y], &bounds());
    assert!((drag.scale - 1.2345).abs() < 1e-4, "{}", drag.scale);
}

#[test]
fn a_widget_is_pushed_back_inside_the_screen() {
    let out = Rect::new(-10.0, 500.0, 60.0, 20.0);
    assert_eq!(clamp_into(out, SCREEN), Rect::new(0.0, 220.0, 60.0, 20.0));
    let huge = Rect::new(30.0, 30.0, 900.0, 20.0);
    assert_eq!(clamp_into(huge, SCREEN).x, 0.0);
}

#[test]
fn the_wheel_steps_the_scale_and_stops_at_the_ends() {
    let (rect, scale) = wheel_resize((BOX, 1.0), -1.0, &bounds()).expect("grows");
    assert!((scale - 1.05).abs() < 1e-6);
    assert_eq!((rect.x, rect.y), (BOX.x, BOX.y), "the corner stays");
    assert!((rect.width - 63.0).abs() < 1e-3);
    assert!(wheel_resize((BOX, 3.0), -1.0, &bounds()).is_none());
    assert!(wheel_resize((BOX, 0.5), 1.0, &bounds()).is_none());
}

#[test]
fn growing_at_the_screen_edge_pulls_the_widget_back_in() {
    let at_edge = Rect::new(SCREEN[0] - 60.0, 10.0, 60.0, 20.0);
    let (rect, _) = wheel_resize((at_edge, 1.0), -2.0, &bounds()).expect("grows");
    assert!((rect.right() - SCREEN[0]).abs() < 1e-3);
}

#[test]
fn a_move_keeps_the_grab_offset_and_snaps() {
    let mut drag = Drag::begin("fps".into(), Grab::Move([10.0, 5.0]), BOX, 1.0);
    drag.follow([70.0, 55.0], &bounds());
    assert_eq!((drag.live.x, drag.live.y), (60.0, 50.0));
    assert!(drag.changed());
    assert!(drag.guides.is_empty(), "far from every line");
    drag.follow([12.0, 7.0], &bounds());
    assert_eq!(
        (drag.live.x, drag.live.y),
        (0.0, 0.0),
        "snapped to the corner"
    );
    assert!(drag.guides.contains(&Guide::Vertical(0.0)));
    assert!(drag.guides.contains(&Guide::Horizontal(0.0)));
}

#[test]
fn a_resize_follows_the_pointer_from_where_the_corner_was_held() {
    let held = 2.0;
    let mut drag = Drag::begin("fps".into(), Grab::Resize(held), BOX, 1.0);
    // Pulling the corner to twice the width gives scale 2.
    drag.follow([BOX.x + 2.0 * BOX.width + held, BOX.y], &bounds());
    assert!((drag.scale - 2.0).abs() < 1e-6);
    assert!((drag.live.width - 120.0).abs() < 1e-3);
    assert_eq!((drag.live.x, drag.live.y), (BOX.x, BOX.y));
    drag.follow([0.0, 0.0], &bounds());
    assert_eq!(drag.scale, 0.5, "never below the smallest scale");
}

#[test]
fn an_untouched_drag_has_not_changed() {
    let drag = Drag::begin("fps".into(), Grab::Move([1.0, 1.0]), BOX, 1.0);
    assert!(!drag.changed());
}

#[test]
fn guides_mark_the_edges_and_the_centre_the_widget_lies_on() {
    let centred = Rect::new((SCREEN[0] - 60.0) / 2.0, 0.0, 60.0, 20.0);
    let guides = guides_for(centred, SCREEN);
    assert!(guides.contains(&Guide::Vertical(SCREEN[0] / 2.0)));
    assert!(guides.contains(&Guide::Horizontal(0.0)));
    let bottom_right = Rect::new(SCREEN[0] - 60.0, SCREEN[1] - 20.0, 60.0, 20.0);
    let guides = guides_for(bottom_right, SCREEN);
    assert!(guides.contains(&Guide::Vertical(SCREEN[0])));
    assert!(guides.contains(&Guide::Horizontal(SCREEN[1])));
    assert!(guides_for(BOX, SCREEN).is_empty());
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the wheel, the corner handle and the drop of the layer.

use super::*;

#[test]
fn the_wheel_over_a_widget_changes_its_scale_by_a_step() {
    let view = view();
    let start = rect_of(&view, 0);
    let wheel = |delta| UiInput::new(vec![moved(middle(start)), InputEvent::Wheel { delta }]);
    let Some(HudEditorOutcome::Placed { scale, rect, .. }) =
        show(&mut HudLayer::default(), &view, wheel(-1.0))
    else {
        panic!("expected a placement");
    };
    assert!((scale - 1.05).abs() < 1e-4, "{scale}");
    assert_eq!((rect.x, rect.y), (start.x, start.y));
    assert!((rect.width - start.width * 1.05).abs() < 1e-3);
    let Some(HudEditorOutcome::Placed { scale, .. }) =
        show(&mut HudLayer::default(), &view, wheel(1.0))
    else {
        panic!("expected a placement");
    };
    assert!((scale - 0.95).abs() < 1e-4);
    let away = UiInput::new(vec![moved([5.0, 5.0]), InputEvent::Wheel { delta: 1.0 }]);
    assert_eq!(show(&mut HudLayer::default(), &view, away), None);
}

#[test]
fn the_corner_handle_resizes_and_reports_on_release() {
    let view = view();
    let start = rect_of(&view, 0);
    let frame = frame_rect(start, SCREEN);
    let corner = [frame.right() - 1.0, frame.bottom() - 1.0];
    let mut screen = HudLayer::default();
    let to = [corner[0] + start.width, corner[1]];
    show(
        &mut screen,
        &view,
        UiInput::new(vec![pressed(corner), moved(to)]),
    );
    let drag = screen.drag.as_ref().expect("dragging");
    assert!(drag.scale > 1.5, "{}", drag.scale);
    let Some(HudEditorOutcome::Placed { id, scale, .. }) =
        show(&mut screen, &view, UiInput::new(vec![released(to)]))
    else {
        panic!("expected a placement");
    };
    assert_eq!(id, "fps");
    assert!(scale > 1.5);
}

#[test]
fn the_widget_is_drawn_where_it_is_dropped_in_the_frame_of_the_drop() {
    let view = view();
    let start = rect_of(&view, 0);
    let grab = middle(start);
    let mut screen = HudLayer::default();
    let to = [grab[0] + 40.0, grab[1]];
    show(
        &mut screen,
        &view,
        UiInput::new(vec![pressed(grab), moved(to)]),
    );
    let font = FontAtlas::fallback();
    let first_panel = frame(UiInput::new(vec![released(to)]), |ui| {
        let zones = HudZones {
            snap: &snap,
            blocked: &[],
            grid: None,
        };
        screen.show(ui, (&font, &font), &view, zones);
        ui.draw().commands().iter().find_map(|c| match c {
            DrawCommand::RoundedRect { rect, .. } if rect.width < 200.0 * SCALE => Some(rect.x),
            _ => None,
        })
    });
    assert_eq!(first_panel, Some((start.x + 40.0) * SCALE));
}

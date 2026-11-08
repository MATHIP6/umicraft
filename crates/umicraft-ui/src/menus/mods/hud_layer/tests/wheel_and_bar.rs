// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the wheel, the corner handle, the bar and the readout of the layer.

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
    assert!((scale - 1.1).abs() < 1e-4, "{scale}");
    assert_eq!((rect.x, rect.y), (start.x, start.y));
    assert!((rect.width - start.width * 1.1).abs() < 1e-3);
    let Some(HudEditorOutcome::Placed { scale, .. }) =
        show(&mut HudLayer::default(), &view, wheel(1.0))
    else {
        panic!("expected a placement");
    };
    assert!((scale - 0.9).abs() < 1e-4);
    let away = UiInput::new(vec![moved([5.0, 5.0]), InputEvent::Wheel { delta: 1.0 }]);
    assert_eq!(show(&mut HudLayer::default(), &view, away), None);
}

#[test]
fn the_corner_handle_resizes_and_reports_on_release() {
    let view = view();
    let start = rect_of(&view, 0);
    let corner = [start.right() - 1.0, start.bottom() - 1.0];
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
fn the_bar_resets_the_selection_or_everything() {
    let view = view();
    let bar = toolbar::layout(VIEWPORT);
    let [reset, reset_all] = bar.buttons;
    let mut screen = HudLayer::default();
    assert_eq!(
        show(&mut screen, &view, click_at(reset)),
        None,
        "no selection"
    );
    assert_eq!(
        show(&mut screen, &view, click_at(reset_all)),
        Some(HudEditorOutcome::ResetAll)
    );
    let at = middle(rect_of(&view, 1));
    show(
        &mut screen,
        &view,
        UiInput::new(vec![pressed(at), released(at)]),
    );
    assert_eq!(
        show(&mut screen, &view, click_at(reset)),
        Some(HudEditorOutcome::Reset { id: "cps".into() })
    );
}

#[test]
fn a_click_on_the_bar_does_not_start_a_drag() {
    let mut view = view();
    let bar = toolbar::layout(VIEWPORT);
    let under = middle(bar.bar);
    let gui = [under[0] / SCALE, under[1] / SCALE];
    view.widgets[0].widget.position = [gui[0] - 5.0, gui[1] - 3.0];
    let mut screen = HudLayer::default();
    show(
        &mut screen,
        &view,
        click_at(Rect::new(under[0], bar.bar.y + 1.0, 0.0, 0.0)),
    );
    assert!(screen.drag.is_none());
}

#[test]
fn the_readout_names_the_selection_and_nothing_is_said_otherwise() {
    let view = view();
    let mut screen = HudLayer::default();
    assert!(texts(&mut screen, &view)
        .iter()
        .all(|t| !t.contains(" - x")));
    let at = middle(rect_of(&view, 0));
    show(
        &mut screen,
        &view,
        UiInput::new(vec![pressed(at), released(at)]),
    );
    let names = texts(&mut screen, &view);
    assert!(names.iter().any(|t| t == "Name fps - x1.0"), "{names:?}");
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
            bar: true,
        };
        screen.show(ui, (&font, &font), &Lang::builtin(), &view, zones);
        ui.draw().commands().iter().find_map(|c| match c {
            DrawCommand::RoundedRect { rect, .. } if rect.width < 200.0 * SCALE => Some(rect.x),
            _ => None,
        })
    });
    assert_eq!(first_panel, Some((start.x + 40.0) * SCALE));
}

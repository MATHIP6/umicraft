// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the HUD layer: the mouse, the wheel, the bar and the blocked zones. The pure
//! drag logic is in `logic`.

mod gear;
mod logic;
mod wheel_and_bar;

use super::*;
use crate::draw::DrawCommand;
use crate::input::UiInput;
use crate::menus::common::testing::{click_at, frame, VIEWPORT};
use crate::mod_widget::ModLine;

/// The viewport of the tests is 427 x 240 GUI units at scale 3.
const SCREEN: [f32; 2] = [427.0, 240.0];
const SCALE: f32 = 3.0;

/// Pulls a rectangle to the top-left screen edges within 4 units, like the
/// mods crate does (which also knows the right edge and the centre).
fn snap(rect: Rect, _screen: [f32; 2]) -> Rect {
    let near = |v: f32| if v.abs() <= 4.0 { 0.0 } else { v };
    Rect::new(near(rect.x), near(rect.y), rect.width, rect.height)
}

fn widget(id: &str, position: [f32; 2], scale: f32) -> HudWidgetView {
    HudWidgetView {
        id: id.into(),
        name: format!("Name {id}"),
        widget: ModWidgetView {
            lines: vec![ModLine::new("60 FPS", 0xFFFFFF)],
            position,
            scale,
            style: Default::default(),
        },
    }
}

fn view() -> HudEditorView {
    HudEditorView {
        widgets: vec![
            widget("fps", [100.0, 100.0], 1.0),
            widget("cps", [200.0, 100.0], 1.0),
        ],
        screen: SCREEN,
        scale_range: (0.5, 3.0),
    }
}

fn rect_of(view: &HudEditorView, index: usize) -> Rect {
    widget_rect(&FontAtlas::fallback(), &view.widgets[index].widget)
}

fn px(point: [f32; 2]) -> [f32; 2] {
    [point[0] * SCALE, point[1] * SCALE]
}

/// One frame of the layer with the reset bar shown and nothing else blocked.
fn show(screen: &mut HudLayer, view: &HudEditorView, input: UiInput) -> Option<HudEditorOutcome> {
    show_blocking(screen, view, input, &[])
}

/// One frame of the layer with the reset bar shown and `blocked` protected.
fn show_blocking(
    screen: &mut HudLayer,
    view: &HudEditorView,
    input: UiInput,
    blocked: &[Rect],
) -> Option<HudEditorOutcome> {
    let font = FontAtlas::fallback();
    let zones = HudZones {
        snap: &snap,
        blocked,
        bar: true,
    };
    frame(input, |ui| {
        screen.show(ui, (&font, &font), &Lang::builtin(), view, zones)
    })
}

fn pressed(at: [f32; 2]) -> InputEvent {
    InputEvent::PointerPressed {
        position: px(at),
        button: PointerButton::Left,
    }
}

fn moved(to: [f32; 2]) -> InputEvent {
    InputEvent::PointerMoved { position: px(to) }
}

fn released(at: [f32; 2]) -> InputEvent {
    InputEvent::PointerReleased {
        position: px(at),
        button: PointerButton::Left,
    }
}

fn middle(rect: Rect) -> [f32; 2] {
    [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5]
}

fn texts(screen: &mut HudLayer, view: &HudEditorView) -> Vec<String> {
    let font = FontAtlas::fallback();
    frame(UiInput::default(), |ui| {
        let zones = HudZones {
            snap: &snap,
            blocked: &[],
            bar: true,
        };
        screen.show(ui, (&font, &font), &Lang::builtin(), view, zones);
        ui.draw()
            .commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    })
}

#[test]
fn dragging_a_widget_reports_where_it_was_dropped() {
    let view = view();
    let start = rect_of(&view, 0);
    let grab = middle(start);
    let mut screen = HudLayer::default();
    let drag = UiInput::new(vec![pressed(grab), moved([grab[0] + 30.0, grab[1] + 20.0])]);
    assert_eq!(
        show(&mut screen, &view, drag),
        None,
        "nothing leaves mid-drag"
    );
    let drop = UiInput::new(vec![released([grab[0] + 30.0, grab[1] + 20.0])]);
    assert_eq!(
        show(&mut screen, &view, drop),
        Some(HudEditorOutcome::Placed {
            id: "fps".into(),
            rect: start.translate(30.0, 20.0),
            scale: 1.0,
            screen: SCREEN,
        })
    );
}

#[test]
fn a_click_without_movement_selects_but_places_nothing() {
    let view = view();
    let at = middle(rect_of(&view, 1));
    let mut screen = HudLayer::default();
    let click = UiInput::new(vec![pressed(at), released(at)]);
    assert_eq!(show(&mut screen, &view, click), None);
    assert_eq!(screen.selected.as_deref(), Some("cps"));
    let away = UiInput::new(vec![pressed([5.0, 5.0]), released([5.0, 5.0])]);
    assert_eq!(show(&mut screen, &view, away), None);
    assert_eq!(screen.selected, None, "a click on the empty game deselects");
}

#[test]
fn the_drop_snaps_to_the_edge_and_the_guides_show_while_dragging() {
    let view = view();
    let start = rect_of(&view, 0);
    let grab = middle(start);
    let mut screen = HudLayer::default();
    // Move the widget so that its left edge is 2 units from the screen edge.
    let to = [grab[0] - start.x + 2.0, grab[1]];
    let held = UiInput::new(vec![pressed(grab), moved(to)]);
    show(&mut screen, &view, held);
    let live = screen.drag.as_ref().expect("dragging").live;
    assert_eq!(live.x, 0.0, "snapped to the left edge");
    assert!(!screen.drag.as_ref().expect("dragging").guides.is_empty());
    let drop = UiInput::new(vec![released(to)]);
    let Some(HudEditorOutcome::Placed { rect, .. }) = show(&mut screen, &view, drop) else {
        panic!("expected a placement");
    };
    assert_eq!(rect.x, 0.0);
}

#[test]
fn a_dragged_widget_never_leaves_the_screen() {
    let view = view();
    let grab = middle(rect_of(&view, 0));
    let mut screen = HudLayer::default();
    let far = UiInput::new(vec![pressed(grab), moved([5000.0, -5000.0])]);
    show(&mut screen, &view, far);
    let live = screen.drag.as_ref().expect("dragging").live;
    assert!(live.right() <= SCREEN[0] && live.y >= 0.0, "{live:?}");
}

#[test]
fn a_press_on_a_blocked_zone_never_starts_a_drag() {
    let view = view();
    let rect = rect_of(&view, 0);
    let grab = middle(rect);
    // A button of the menu sits on top of the widget.
    let button = Rect::new(
        rect.x * SCALE,
        rect.y * SCALE,
        rect.width * SCALE,
        rect.height * SCALE,
    );
    let mut screen = HudLayer::default();
    let press = UiInput::new(vec![pressed(grab), moved([grab[0] + 30.0, grab[1]])]);
    show_blocking(&mut screen, &view, press, &[button]);
    assert!(screen.drag.is_none());
    let drop = UiInput::new(vec![released([grab[0] + 30.0, grab[1]])]);
    assert_eq!(show_blocking(&mut screen, &view, drop, &[button]), None);
    // The same press with nothing in the way does grab it.
    show(&mut screen, &view, UiInput::new(vec![pressed(grab)]));
    assert!(screen.drag.is_some());
}

#[test]
fn the_wheel_over_a_blocked_zone_does_not_resize() {
    let view = view();
    let rect = rect_of(&view, 0);
    let button = Rect::new(
        rect.x * SCALE,
        rect.y * SCALE,
        rect.width * SCALE,
        rect.height * SCALE,
    );
    let wheel = UiInput::new(vec![moved(middle(rect)), InputEvent::Wheel { delta: -1.0 }]);
    let mut screen = HudLayer::default();
    assert_eq!(show_blocking(&mut screen, &view, wheel, &[button]), None);
}

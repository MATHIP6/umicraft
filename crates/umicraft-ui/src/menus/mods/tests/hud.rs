// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the HUD widgets inside the mods menu: they move with the mouse
//! anywhere but on a button or on the open panel.

use super::*;
use crate::menus::common::testing::VIEWPORT;

const SCALE: f32 = 3.0;
const SCREEN: [f32; 2] = [427.0, 240.0];

fn widget(id: &str, position: [f32; 2]) -> HudWidgetView {
    HudWidgetView {
        id: id.into(),
        name: id.into(),
        widget: ModWidgetView {
            lines: vec![ModLine::new("60 FPS", 0xFFFFFF)],
            position,
            scale: 1.0,
        },
    }
}

fn view_with(widgets: Vec<HudWidgetView>) -> ModsView {
    ModsView {
        hud: HudEditorView {
            widgets,
            screen: SCREEN,
            scale_range: (0.5, 3.0),
        },
        ..view()
    }
}

fn pressed(at: [f32; 2]) -> InputEvent {
    InputEvent::PointerPressed {
        position: at,
        button: PointerButton::Left,
    }
}

fn moved(at: [f32; 2]) -> InputEvent {
    InputEvent::PointerMoved { position: at }
}

fn released(at: [f32; 2]) -> InputEvent {
    InputEvent::PointerReleased {
        position: at,
        button: PointerButton::Left,
    }
}

/// Presses at `from`, moves by `(30, 20)` pixels and releases there.
fn drag_from(screen: &mut ModsScreen, view: &ModsView, from: [f32; 2]) -> Option<ModsOutcome> {
    let to = [from[0] + 30.0, from[1] + 20.0];
    let events = vec![moved(from), pressed(from), moved(to), released(to)];
    show_view(screen, view, UiInput::new(events))
}

/// The pixel at the middle of the widget at `position` (it is wider than 8 units).
fn inside(position: [f32; 2]) -> [f32; 2] {
    [(position[0] + 8.0) * SCALE, (position[1] + 5.0) * SCALE]
}

/// Whether the outcome moved a widget (a button released under the pointer
/// is allowed to answer, a widget is not allowed to move).
fn placed(outcome: &Option<ModsOutcome>) -> bool {
    matches!(outcome, Some(ModsOutcome::Hud(_)))
}

fn gui_position(rect: Rect) -> [f32; 2] {
    [rect.x / SCALE, rect.y / SCALE]
}

#[test]
fn a_widget_dragged_on_the_landing_page_is_placed() {
    let view = view_with(vec![widget("fps", [20.0, 20.0])]);
    let outcome = drag_from(&mut ModsScreen::new(), &view, inside([20.0, 20.0]));
    let Some(ModsOutcome::Hud(HudEditorOutcome::Placed { id, rect, .. })) = outcome else {
        panic!("expected a placement, got {outcome:?}");
    };
    assert_eq!(id, "fps");
    assert_eq!(rect.x, 30.0);
    assert!((rect.y - (20.0 + 20.0 / SCALE)).abs() < 1e-3, "{}", rect.y);
}

#[test]
fn a_drag_that_starts_on_the_mods_button_places_nothing() {
    let button = landing_layout(VIEWPORT).mods;
    let view = view_with(vec![widget("fps", gui_position(button))]);
    let from = [button.x + 4.0, button.y + 4.0];
    let outcome = drag_from(&mut ModsScreen::new(), &view, from);
    assert!(!placed(&outcome), "{outcome:?}");
}

#[test]
fn a_drag_that_starts_on_the_cross_places_nothing() {
    let cross = landing_layout(VIEWPORT).close;
    let view = view_with(vec![widget("fps", gui_position(cross))]);
    let from = [cross.x + 2.0, cross.y + 2.0];
    let outcome = drag_from(&mut ModsScreen::new(), &view, from);
    assert!(!placed(&outcome), "{outcome:?}");
}

#[test]
fn a_drag_that_starts_under_the_panel_places_nothing() {
    let panel = mods_layout(VIEWPORT).panel;
    let view = view_with(vec![widget("fps", gui_position(panel))]);
    let from = [panel.x + 10.0, panel.y + 10.0];
    assert_eq!(drag_from(&mut on_list(), &view, from), None);
}

#[test]
fn a_widget_beside_the_open_panel_is_still_movable() {
    let panel = mods_layout(VIEWPORT).panel;
    let position = [2.0, 2.0];
    assert!(inside(position)[0] < panel.x);
    let view = view_with(vec![widget("fps", position)]);
    let outcome = drag_from(&mut on_list(), &view, inside(position));
    assert!(matches!(
        outcome,
        Some(ModsOutcome::Hud(HudEditorOutcome::Placed { .. }))
    ));
}

#[test]
fn the_reset_bar_is_only_on_the_landing_page() {
    let view = view_with(vec![widget("fps", [20.0, 20.0])]);
    let landing = texts(&mut ModsScreen::new(), &view);
    assert!(has_text(&landing, "umicraft.mods.editor.reset_all"));
    let list = texts(&mut on_list(), &view);
    assert!(!has_text(&list, "umicraft.mods.editor.reset_all"));
}

#[test]
fn the_landing_says_how_to_move_widgets_or_that_none_is_on() {
    let with = texts(
        &mut ModsScreen::new(),
        &view_with(vec![widget("fps", [20.0, 20.0])]),
    );
    assert!(has_text(&with, "umicraft.mods.editor.hint"), "{with:?}");
    let none = texts(&mut ModsScreen::new(), &view());
    assert!(has_text(&none, "umicraft.mods.editor.empty"), "{none:?}");
}

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
            style: Default::default(),
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

/// A pixel inside the widget at `position`, up and right of its gear (it is wider
/// than 20 units).
fn inside(position: [f32; 2]) -> [f32; 2] {
    [(position[0] + 20.0) * SCALE, (position[1] + 3.0) * SCALE]
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
fn a_drag_that_starts_on_the_cross_places_nothing() {
    let cross = mods_layout(VIEWPORT).close;
    let view = view_with(vec![widget("fps", gui_position(cross))]);
    let from = [cross.x + 2.0, cross.y + 2.0];
    let outcome = drag_from(&mut on_list(), &view, from);
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
    let Some(ModsOutcome::Hud(HudEditorOutcome::Placed { id, rect, .. })) = outcome else {
        panic!("expected a placement, got {outcome:?}");
    };
    assert_eq!(id, "fps");
    assert_eq!(rect.x, 12.0);
    assert!((rect.y - (2.0 + 20.0 / SCALE)).abs() < 1e-3, "{}", rect.y);
}

#[test]
fn the_reset_bar_is_under_the_panel() {
    let view = view_with(vec![widget("fps", [20.0, 20.0])]);
    let list = texts(&mut on_list(), &view);
    assert!(has_text(&list, "umicraft.mods.editor.reset_all"));
}

#[test]
fn the_panel_says_how_to_move_widgets_only_when_there_is_one() {
    let with = texts(
        &mut on_list(),
        &view_with(vec![widget("fps", [20.0, 20.0])]),
    );
    assert!(has_text(&with, "umicraft.mods.editor.hint"), "{with:?}");
    let none = texts(&mut on_list(), &view());
    assert!(!has_text(&none, "umicraft.mods.editor.hint"), "{none:?}");
}

#[test]
fn the_gear_of_a_widget_opens_the_settings_of_its_mod() {
    // The widget stands left of the panel; its mod is the first card.
    let position = [2.0, 40.0];
    let view = view_with(vec![widget("fps", position)]);
    let mut screen = on_list();
    // The pointer over the widget makes the gear appear.
    show_view(
        &mut screen,
        &view,
        UiInput::new(vec![moved(inside(position))]),
    );
    let font = FontAtlas::fallback();
    let (width, height) = crate::mod_widget::measure(
        &font,
        &view.hud.widgets[0].widget.lines,
        1.0,
        &Default::default(),
    );
    // In the strip under the text, at the left.
    let text = Rect::new(position[0], position[1], width, height);
    let gear = crate::menus::mods::hud_layer::gear_rect_for_tests(text);
    let at = [
        (gear.x + gear.width * 0.5) * SCALE,
        (gear.y + gear.height * 0.5) * SCALE,
    ];
    let outcome = show_view(
        &mut screen,
        &view,
        UiInput::new(vec![moved(at), pressed(at), released(at)]),
    );
    assert_eq!(outcome, None, "the menu keeps this one for itself");
    assert_eq!(screen.page, Page::Settings("fps".into()));
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the HUD editor: the page, its dock and what it reports.

use super::super::editor::{
    action_row, dock_layout, dock_x, list_row, row_toggle, split, DockLayout, TAB_SETTINGS,
    TAB_WIDGETS,
};
use super::*;
use crate::menus::common::Gui;

const SCALE: f32 = 3.0;
const SCREEN: [f32; 2] = [427.0, 240.0];

fn widget(id: &str, position: [f32; 2]) -> HudWidgetView {
    HudWidgetView {
        id: id.into(),
        name: id.to_uppercase(),
        widget: ModWidgetView {
            lines: vec![ModLine::new("60 FPS", 0xFFFFFF)],
            position,
            scale: 1.0,
            style: Default::default(),
        },
    }
}

fn entry(id: &str, enabled: bool, locked: bool) -> HudEntryView {
    HudEntryView {
        id: id.into(),
        name: id.to_uppercase(),
        enabled,
        locked,
    }
}

/// A menu with `fps` on the left and `cps` on the right, both on, and `ping`
/// off: the dock covers as many widgets on either side, so it stays right.
fn editor_view() -> ModsView {
    ModsView {
        hud: HudEditorView {
            widgets: vec![widget("fps", [20.0, 100.0]), widget("cps", [330.0, 100.0])],
            screen: SCREEN,
            scale_range: (0.5, 3.0),
            entries: vec![
                entry("fps", true, false),
                entry("cps", true, false),
                entry("ping", false, false),
            ],
            ..HudEditorView::default()
        },
        ..view()
    }
}

fn locked(mut view: ModsView, id: &str) -> ModsView {
    for entry in &mut view.hud.entries {
        entry.locked |= entry.id == id;
    }
    view
}

fn in_editor() -> ModsScreen {
    let mut screen = on_list();
    screen.open_hud_editor(None);
    screen
}

/// The dock where it rests on the right of the test viewport.
fn dock() -> DockLayout {
    let gui = Gui::new(VIEWPORT);
    dock_layout(&gui, dock_x(gui.width, false))
}

/// The cell `(row, column)` of the actions, in a row of `columns` cells.
fn action(row: usize, column: usize, columns: usize) -> Rect {
    split(action_row(dock().footer, row, SCALE), columns, SCALE)[column]
}

fn pressed(at: [f32; 2]) -> InputEvent {
    InputEvent::PointerPressed {
        position: at,
        button: PointerButton::Left,
    }
}

fn released(at: [f32; 2]) -> InputEvent {
    InputEvent::PointerReleased {
        position: at,
        button: PointerButton::Left,
    }
}

/// Presses inside the widget at `position` (GUI units), moves by `(30, 20)`
/// pixels and releases there.
fn drag(screen: &mut ModsScreen, view: &ModsView, position: [f32; 2]) -> Option<ModsOutcome> {
    let from = [(position[0] + 20.0) * SCALE, (position[1] + 3.0) * SCALE];
    let to = [from[0] + 30.0, from[1] + 20.0];
    let events = vec![
        InputEvent::PointerMoved { position: from },
        pressed(from),
        InputEvent::PointerMoved { position: to },
        released(to),
    ];
    show_view(screen, view, UiInput::new(events))
}

fn placed_rect(outcome: Option<ModsOutcome>) -> Rect {
    match outcome {
        Some(ModsOutcome::Hud(HudEditorOutcome::Placed { rect, .. })) => rect,
        other => panic!("expected a placement, got {other:?}"),
    }
}

fn key(key: Key) -> UiInput {
    UiInput::new(vec![InputEvent::KeyPressed(key)])
}

#[test]
fn the_button_of_the_list_opens_the_editor_and_escape_leaves_it() {
    let mut screen = on_list();
    let layout = mods_layout(VIEWPORT);
    let edit = Rect::new(
        layout.close.x - 50.0 * layout.scale,
        layout.header.y,
        46.0 * layout.scale,
        layout.header.height,
    );
    assert_eq!(show_view(&mut screen, &editor_view(), click_at(edit)), None);
    assert_eq!(screen.page, Page::HudEditor);
    assert!(screen.can_go_back(), "Escape is the menu's, not the window's");
    assert_eq!(show_view(&mut screen, &editor_view(), key(Key::Escape)), None);
    assert_eq!(screen.page, Page::List);
}

#[test]
fn the_button_is_not_shown_without_a_widget_to_place() {
    let mut screen = on_list();
    assert!(!has_text(&texts(&mut screen, &view()), "umicraft.mods.editor.open"));
    assert!(has_text(
        &texts(&mut screen, &editor_view()),
        "umicraft.mods.editor.open"
    ));
}

#[test]
fn the_editor_leaves_the_whole_screen_to_the_widgets() {
    // The middle of the screen, where the panel of the list would keep the click.
    let mut view = editor_view();
    view.hud.widgets[0] = widget("fps", [180.0, 100.0]);
    let mut on_the_list = on_list();
    assert_eq!(drag(&mut on_the_list, &view, [180.0, 100.0]), None);
    let rect = placed_rect(drag(&mut in_editor(), &view, [180.0, 100.0]));
    assert_eq!((rect.x, rect.y), (190.0, 100.0 + 20.0 / SCALE));
}

#[test]
fn the_dock_keeps_its_clicks() {
    let mut screen = in_editor();
    // `cps` lies under the dock: a press there is the dock's.
    assert_eq!(drag(&mut screen, &editor_view(), [330.0, 100.0]), None);
    // The press landed on a row of the list, which is what it selected.
    assert_ne!(screen.hud.selected(), Some("cps"));
}

#[test]
fn the_dock_goes_to_the_side_the_selected_widget_is_not_on() {
    let mut screen = in_editor();
    assert!(!screen.editor.left, "as many widgets on both sides: it stays");
    screen.hud.select(Some("cps".into()));
    show_view(&mut screen, &editor_view(), UiInput::default());
    assert!(screen.editor.left, "cps is on the right");
    screen.hud.select(Some("fps".into()));
    show_view(&mut screen, &editor_view(), UiInput::default());
    assert!(!screen.editor.left);

    // Nothing selected: the side that covers fewer widgets.
    let mut alone = editor_view();
    alone.hud.widgets.truncate(1);
    let mut screen = in_editor();
    screen.editor.left = true;
    show_view(&mut screen, &alone, UiInput::default());
    assert!(!screen.editor.left, "only the left side has a widget");
}

#[test]
fn a_locked_widget_is_selected_but_never_moved() {
    let view = locked(editor_view(), "fps");
    let mut screen = in_editor();
    assert_eq!(drag(&mut screen, &view, [20.0, 100.0]), None);
    assert_eq!(screen.hud.selected(), Some("fps"));
    assert_eq!(screen.editor.tab, TAB_SETTINGS, "its settings open");
    // Neither the arrows nor the wheel move it.
    assert_eq!(show_view(&mut screen, &view, key(Key::ArrowRight)), None);
    let over = [(20.0 + 20.0) * SCALE, (100.0 + 3.0) * SCALE];
    let wheel = vec![
        InputEvent::PointerMoved { position: over },
        InputEvent::Wheel { delta: -1.0 },
    ];
    assert_eq!(show_view(&mut screen, &view, UiInput::new(wheel)), None);
}

#[test]
fn the_grid_puts_a_dragged_widget_on_its_points() {
    let mut screen = in_editor();
    screen.editor.grid = true;
    // 30 by 20 pixels is 10 by 6.67 units: (30, 106.67) rounds to (32, 104).
    let rect = placed_rect(drag(&mut screen, &editor_view(), [20.0, 100.0]));
    assert_eq!((rect.x, rect.y), (32.0, 104.0));
    let mut free = in_editor();
    let rect = placed_rect(drag(&mut free, &editor_view(), [20.0, 100.0]));
    assert_eq!(rect.x, 30.0, "no grid, no rounding");
}

#[test]
fn the_arrows_move_the_selected_widget_by_one_unit() {
    let mut screen = in_editor();
    screen.hud.select(Some("fps".into()));
    screen.editor.tab = TAB_SETTINGS;
    let rect = placed_rect(show_view(&mut screen, &editor_view(), key(Key::ArrowRight)));
    assert_eq!((rect.x, rect.y), (21.0, 100.0));
    let rect = placed_rect(show_view(&mut screen, &editor_view(), key(Key::ArrowUp)));
    assert_eq!((rect.x, rect.y), (20.0, 99.0));
    // On the list of widgets the arrows are not the widget's.
    screen.editor.tab = TAB_WIDGETS;
    assert_eq!(show_view(&mut screen, &editor_view(), key(Key::ArrowRight)), None);
}

#[test]
fn a_row_of_the_list_selects_its_widget_and_its_switch_turns_the_mod() {
    let mut screen = in_editor();
    let body = dock().body;
    let ping = list_row(body, 2, SCALE);
    let name = Rect::new(ping.x, ping.y, 30.0 * SCALE, ping.height);
    assert_eq!(show_view(&mut screen, &editor_view(), click_at(name)), None);
    assert_eq!(screen.hud.selected(), Some("ping"));
    assert_eq!(screen.editor.tab, TAB_SETTINGS);

    let mut screen = in_editor();
    let switch = row_toggle(ping, SCALE);
    assert_eq!(
        show_view(&mut screen, &editor_view(), click_at(switch)),
        Some(ModsOutcome::Toggle {
            id: "ping".into(),
            enabled: true
        })
    );
    assert_eq!(screen.hud.selected(), None, "the switch does not select");
}

#[test]
fn the_search_of_the_list_keeps_the_matching_widgets() {
    let mut screen = in_editor();
    screen.editor.query = "pi".into();
    let shown = texts(&mut screen, &editor_view());
    assert!(has_text(&shown, "PING") && !has_text(&shown, "FPS"));
    screen.editor.query = "zzz".into();
    let shown = texts(&mut screen, &editor_view());
    assert!(has_text(&shown, "umicraft.mods.editor.no_match"));
}

#[test]
fn undo_and_redo_answer_only_when_there_is_something() {
    let hud = |outcome| Some(ModsOutcome::Hud(outcome));
    let (undo, redo) = (action(0, 0, 4), action(0, 1, 4));
    let mut view = editor_view();
    assert_eq!(show_view(&mut in_editor(), &view, click_at(undo)), None);
    view.hud.history = (true, false);
    assert_eq!(
        show_view(&mut in_editor(), &view, click_at(undo)),
        hud(HudEditorOutcome::Undo)
    );
    assert_eq!(show_view(&mut in_editor(), &view, click_at(redo)), None);
    view.hud.history = (false, true);
    assert_eq!(
        show_view(&mut in_editor(), &view, click_at(redo)),
        hud(HudEditorOutcome::Redo)
    );
}

#[test]
fn the_presets_report_themselves_and_custom_waits_for_a_saved_layout() {
    let preset = |kind| Some(ModsOutcome::Hud(HudEditorOutcome::Preset(kind)));
    let cells = [
        (action(1, 0, 2), HudPresetKind::Default),
        (action(1, 1, 2), HudPresetKind::Pvp),
        (action(2, 0, 2), HudPresetKind::Minimal),
    ];
    for (rect, kind) in cells {
        let clicked = show_view(&mut in_editor(), &editor_view(), click_at(rect));
        assert_eq!(clicked, preset(kind));
    }
    let custom = action(2, 1, 2);
    let mut view = editor_view();
    assert_eq!(show_view(&mut in_editor(), &view, click_at(custom)), None);
    view.hud.has_custom = true;
    assert_eq!(
        show_view(&mut in_editor(), &view, click_at(custom)),
        preset(HudPresetKind::Custom)
    );
}

#[test]
fn save_and_cancel_report_and_go_back_to_the_list() {
    for (column, outcome) in [(1, HudEditorOutcome::Cancel), (2, HudEditorOutcome::Save)] {
        let mut screen = in_editor();
        let clicked = show_view(&mut screen, &editor_view(), click_at(action(3, column, 3)));
        assert_eq!(clicked, Some(ModsOutcome::Hud(outcome)));
        assert_eq!(screen.page, Page::List);
    }
}

#[test]
fn reset_all_asks_before_it_resets() {
    let gui = Gui::new(VIEWPORT);
    // The two buttons of the question, as `confirm_dialog` lays them out.
    let (left, top) = (
        ((gui.width - 176.0) * 0.5).floor(),
        ((gui.height - 46.0) * 0.5).floor(),
    );
    let button = |index: f32| gui.rect(left + 15.0 + index * 76.0, top + 24.0, 70.0, 14.0);

    let mut screen = in_editor();
    let reset_all = action(3, 0, 3);
    assert_eq!(show_view(&mut screen, &editor_view(), click_at(reset_all)), None);
    assert!(screen.editor.confirm, "nothing is reset yet");
    // While the question is up the widgets do not move.
    assert_eq!(drag(&mut screen, &editor_view(), [20.0, 100.0]), None);
    assert_eq!(show_view(&mut screen, &editor_view(), click_at(button(0.0))), None);
    assert!(!screen.editor.confirm, "cancelled");

    show_view(&mut screen, &editor_view(), click_at(reset_all));
    assert_eq!(
        show_view(&mut screen, &editor_view(), click_at(button(1.0))),
        Some(ModsOutcome::Hud(HudEditorOutcome::ResetAll))
    );
    assert!(!screen.editor.confirm);

    // Escape closes the question first, the editor next.
    show_view(&mut screen, &editor_view(), click_at(reset_all));
    show_view(&mut screen, &editor_view(), key(Key::Escape));
    assert!(!screen.editor.confirm);
    assert_eq!(screen.page, Page::HudEditor);
}

#[test]
fn a_widget_hidden_while_editing_is_not_drawn_and_not_grabbed() {
    let mut screen = in_editor();
    assert!(has_text(&texts(&mut screen, &editor_view()), "60 FPS"));
    screen.editor.hidden = vec!["fps".into(), "cps".into()];
    assert!(!has_text(&texts(&mut screen, &editor_view()), "60 FPS"));
    assert_eq!(drag(&mut screen, &editor_view(), [20.0, 100.0]), None);
}

#[test]
fn the_settings_tab_shows_only_what_the_widget_has() {
    let mut screen = in_editor();
    screen.hud.select(Some("fps".into()));
    screen.editor.tab = TAB_SETTINGS;
    let shown = texts(&mut screen, &editor_view());
    // `fps` has options (see `view()`), so it has a look to edit.
    assert!(has_text(&shown, "UMICRAFT.MODS.EDITOR.APPEARANCE"));
    assert!(has_text(&shown, "UMICRAFT.MODS.EDITOR.POSITION"));

    // `ping` is off and has no card here: neither a place nor a look.
    screen.hud.select(Some("ping".into()));
    let shown = texts(&mut screen, &editor_view());
    assert!(!has_text(&shown, "UMICRAFT.MODS.EDITOR.POSITION"));
    assert!(!has_text(&shown, "UMICRAFT.MODS.EDITOR.APPEARANCE"));
    assert!(has_text(&shown, "UMICRAFT.MODS.EDITOR.BEHAVIOR"));

    screen.hud.select(None);
    let shown = texts(&mut screen, &editor_view());
    assert!(has_text(&shown, "umicraft.mods.editor.none"));
}

#[test]
fn the_game_is_not_blurred_behind_the_editor() {
    let mut list = on_list();
    for _ in 0..20 {
        list.tick();
    }
    assert!(list.blur_strength(0.0) > 0.9);
    let mut screen = in_editor();
    for _ in 0..20 {
        screen.tick();
    }
    assert_eq!(screen.blur_strength(0.0), 0.0);
}

#[test]
fn the_dock_fits_narrow_and_wide_screens() {
    for (width, height) in [(640.0, 480.0), (1280.0, 720.0), (1920.0, 1080.0), (2560.0, 1080.0)] {
        let gui = Gui::new(Rect::new(0.0, 0.0, width, height));
        for left in [false, true] {
            let layout = dock_layout(&gui, dock_x(gui.width, left));
            let dock = layout.dock;
            assert!(dock.x >= 0.0 && dock.right() <= width + 0.5, "{width}x{height}");
            assert!(dock.y >= 0.0 && dock.bottom() <= height + 0.5, "{width}x{height}");
            for part in [layout.header, layout.tabs, layout.body, layout.footer] {
                assert!(part.x >= dock.x && part.right() <= dock.right() + 0.5);
                assert!(part.y >= dock.y && part.bottom() <= dock.bottom() + 0.5);
            }
            assert!(layout.body.height > 40.0 * layout.scale, "{width}x{height}");
        }
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;
use crate::draw::DrawCommand;
use crate::input::{InputEvent, UiInput};
use crate::menus::common::testing::{click_at, click_point, frame, key, VIEWPORT};

fn row(name: &str) -> ServerRow {
    ServerRow {
        name: name.into(),
        address: format!("{name}.example.org"),
        status: RowStatus::Online {
            motd: "A server".into(),
            players: 3,
            max: 20,
            version: "1.8.8".into(),
            latency_ms: 40,
            sample: vec!["Alex".into()],
        },
    }
}

fn rows(count: usize) -> Vec<ServerRow> {
    (0..count).map(|i| row(&format!("s{i}"))).collect()
}

fn run(
    screen: &mut ServerListScreen,
    input: UiInput,
    rows: &[ServerRow],
) -> Option<ServerListOutcome> {
    frame(input, |ui| {
        screen.show(ui, &FontAtlas::fallback(), &Lang::builtin(), rows, None)
    })
}

/// Pixels of a GUI position at scale 3.
fn px(x: f32, y: f32) -> [f32; 2] {
    [x * 3.0, y * 3.0]
}

/// The row's top-left corner in GUI units (cx = 213, no scroll).
fn corner(index: usize) -> [f32; 2] {
    [213.0 - 150.0, 36.0 + index as f32 * 36.0]
}

fn body_of(index: usize) -> [f32; 2] {
    let [x, y] = corner(index);
    px(x + 60.0, y + 16.0)
}

fn zone_of(index: usize, dx: f32, dy: f32) -> [f32; 2] {
    let [x, y] = corner(index);
    px(x + dx, y + dy)
}

#[test]
fn the_layout_has_the_vanilla_coordinates() {
    let layout = server_list_layout(VIEWPORT);
    // scale 3, cx = 213, height = 240
    assert_eq!(layout.list.y, 96.0);
    assert_eq!(layout.list.height, 144.0 * 3.0);
    assert_eq!(layout.join, Rect::new(59.0 * 3.0, 188.0 * 3.0, 300.0, 60.0));
    assert_eq!(layout.cancel.x, 293.0 * 3.0);
    assert_eq!(layout.edit.y, 212.0 * 3.0);
}

#[test]
fn buttons_need_a_selection_to_act_on_a_row() {
    let layout = server_list_layout(VIEWPORT);
    let rows = rows(2);
    let mut screen = ServerListScreen::new();
    assert_eq!(run(&mut screen, click_at(layout.join), &rows), None);
    assert_eq!(run(&mut screen, click_at(layout.delete), &rows), None);
    screen.set_selected(Some(1));
    for (rect, expected) in [
        (layout.join, ServerListOutcome::Join(1)),
        (layout.edit, ServerListOutcome::Edit(1)),
        (layout.delete, ServerListOutcome::Delete(1)),
    ] {
        assert_eq!(run(&mut screen, click_at(rect), &rows), Some(expected));
    }
}

#[test]
fn the_free_buttons_always_work() {
    let layout = server_list_layout(VIEWPORT);
    let mut screen = ServerListScreen::new();
    for (rect, expected) in [
        (layout.direct, ServerListOutcome::DirectConnect),
        (layout.add, ServerListOutcome::Add),
        (layout.refresh, ServerListOutcome::Refresh),
        (layout.cancel, ServerListOutcome::Cancel),
    ] {
        assert_eq!(run(&mut screen, click_at(rect), &[]), Some(expected));
    }
    assert_eq!(
        run(&mut screen, key(Key::Escape), &[]),
        Some(ServerListOutcome::Cancel)
    );
}

#[test]
fn a_click_selects_and_a_quick_second_click_joins() {
    let rows = rows(3);
    let mut screen = ServerListScreen::new();
    assert_eq!(run(&mut screen, click_point(body_of(1)), &rows), None);
    assert_eq!(screen.selected(), Some(1));
    screen.tick();
    assert_eq!(
        run(&mut screen, click_point(body_of(1)), &rows),
        Some(ServerListOutcome::Join(1))
    );
}

#[test]
fn a_slow_second_click_does_not_join() {
    let rows = rows(3);
    let mut screen = ServerListScreen::new();
    run(&mut screen, click_point(body_of(0)), &rows);
    for _ in 0..=DOUBLE_CLICK_TICKS {
        screen.tick();
    }
    assert_eq!(run(&mut screen, click_point(body_of(0)), &rows), None);
}

#[test]
fn the_icon_arrows_play_and_move() {
    let rows = rows(3);
    let mut screen = ServerListScreen::new();
    assert_eq!(
        run(&mut screen, click_point(zone_of(1, 24.0, 10.0)), &rows),
        Some(ServerListOutcome::Join(1))
    );
    assert_eq!(
        run(&mut screen, click_point(zone_of(1, 5.0, 5.0)), &rows),
        Some(ServerListOutcome::MoveUp(1))
    );
    assert_eq!(screen.selected(), Some(0));
    assert_eq!(
        run(&mut screen, click_point(zone_of(1, 5.0, 25.0)), &rows),
        Some(ServerListOutcome::MoveDown(1))
    );
    assert_eq!(screen.selected(), Some(2));
    // The first row has no up arrow: that zone is the body.
    assert_eq!(
        run(&mut screen, click_point(zone_of(0, 5.0, 5.0)), &rows),
        None
    );
}

#[test]
fn keys_move_the_selection_and_enter_joins() {
    let rows = rows(3);
    let mut screen = ServerListScreen::new();
    run(&mut screen, key(Key::ArrowDown), &rows);
    run(&mut screen, key(Key::ArrowDown), &rows);
    assert_eq!(screen.selected(), Some(1));
    run(&mut screen, key(Key::ArrowUp), &rows);
    assert_eq!(screen.selected(), Some(0));
    assert_eq!(
        run(&mut screen, key(Key::Enter), &rows),
        Some(ServerListOutcome::Join(0))
    );
}

#[test]
fn a_selection_beyond_the_rows_is_dropped() {
    let mut screen = ServerListScreen::new();
    screen.set_selected(Some(5));
    run(&mut screen, UiInput::default(), &rows(2));
    assert_eq!(screen.selected(), None);
}

#[test]
fn the_wheel_scrolls_when_the_rows_do_not_fit() {
    let wheel = UiInput::new(vec![
        InputEvent::PointerMoved {
            position: [640.0, 300.0],
        },
        InputEvent::Wheel { delta: 1.0 },
    ]);
    let mut screen = ServerListScreen::new();
    run(&mut screen, wheel.clone(), &rows(20));
    assert_eq!(screen.offset(), 18.0);
    let mut short = ServerListScreen::new();
    run(&mut short, wheel, &rows(2));
    assert_eq!(short.offset(), 0.0);
}

#[test]
fn every_status_draws_and_the_latency_icon_has_a_tooltip() {
    let others = [
        RowStatus::Pinging,
        RowStatus::Unreachable,
        RowStatus::VersionMismatch,
    ];
    let rows: Vec<ServerRow> = others
        .into_iter()
        .map(|status| ServerRow {
            name: "x".into(),
            address: "x".into(),
            status,
        })
        .chain(rows(1))
        .collect();
    let [x, y] = corner(3);
    let icon = px(x + ENTRY_WIDTH - 12.0, y + 3.0);
    let hover = UiInput::new(vec![InputEvent::PointerMoved { position: icon }]);
    let mut screen = ServerListScreen::new();
    let shown = frame(hover, |ui| {
        screen.show(ui, &FontAtlas::fallback(), &Lang::builtin(), &rows, None);
        ui.draw()
            .commands()
            .iter()
            .any(|c| matches!(c, DrawCommand::Text { text, .. } if text == "40ms"))
    });
    assert!(shown);
}

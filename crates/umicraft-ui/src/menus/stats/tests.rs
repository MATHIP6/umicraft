// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the statistics screen: tabs, loading, scrolling and sorting.

use super::*;
use crate::input::{InputEvent, Key, UiInput};
use crate::menus::common::testing::{click_at, click_point, frame, key, VIEWPORT};
use crate::widgets::SlotItem;

fn icon() -> SlotItem {
    SlotItem {
        key: 0,
        count: 1,
        damage: 0,
        max_durability: 0,
        glint: false,
    }
}

fn object(id: i32, crafted: i32, used: i32) -> ObjectRow {
    let cell = |raw: i32| {
        Some(StatCell {
            raw,
            text: raw.to_string(),
        })
    };
    ObjectRow {
        id,
        icon: icon(),
        name: format!("Item {id}"),
        cells: [cell(crafted), cell(used), None],
    }
}

fn data() -> StatsData {
    StatsData {
        general: (0..40)
            .map(|i| StatRow::new("stat.jump", i.to_string()))
            .collect(),
        blocks: vec![object(1, 3, 1), object(2, 9, 2), object(3, 5, 3)],
        items: Vec::new(),
        mobs: vec![MobRow {
            entity: "Zombie".to_string(),
            kills: 2,
            killed_by: 0,
        }],
    }
}

fn run(
    screen: &mut StatsScreen,
    input: UiInput,
    data: &StatsData,
    loading: bool,
) -> Option<StatsOutcome> {
    frame(input, |ui| {
        screen.show(ui, &FontAtlas::fallback(), &Lang::builtin(), data, loading)
    })
}

#[test]
fn the_buttons_sit_where_vanilla_puts_them() {
    // scale 3: (213 + 4) * 3, (240 - 28) * 3
    let done = stats_done_rect(VIEWPORT);
    assert_eq!((done.x, done.y, done.width), (651.0, 636.0, 450.0));
    let blocks = stats_tab_rect(VIEWPORT, StatsTab::Blocks);
    // (213 - 80) * 3, (240 - 52) * 3
    assert_eq!((blocks.x, blocks.y, blocks.width), (399.0, 564.0, 240.0));
}

#[test]
fn done_and_escape_leave() {
    let mut screen = StatsScreen::new();
    let done = stats_done_rect(VIEWPORT);
    let data = data();
    assert_eq!(
        run(&mut screen, click_at(done), &data, false),
        Some(StatsOutcome::Done)
    );
    assert_eq!(
        run(&mut screen, key(Key::Escape), &data, false),
        Some(StatsOutcome::Done)
    );
    assert_eq!(run(&mut screen, UiInput::default(), &data, false), None);
}

#[test]
fn a_tab_button_switches_the_list_and_an_empty_tab_is_disabled() {
    let mut screen = StatsScreen::new();
    let data = data();
    run(
        &mut screen,
        click_at(stats_tab_rect(VIEWPORT, StatsTab::Mobs)),
        &data,
        false,
    );
    assert_eq!(screen.tab(), StatsTab::Mobs);
    run(
        &mut screen,
        click_at(stats_tab_rect(VIEWPORT, StatsTab::Items)),
        &data,
        false,
    );
    assert_eq!(screen.tab(), StatsTab::Mobs, "the items tab has no rows");
}

#[test]
fn the_loading_text_has_no_buttons_but_escape_leaves() {
    let mut screen = StatsScreen::new();
    let data = data();
    let done = stats_done_rect(VIEWPORT);
    assert_eq!(run(&mut screen, click_at(done), &data, true), None);
    assert_eq!(
        run(&mut screen, key(Key::Escape), &data, true),
        Some(StatsOutcome::Done)
    );
}

#[test]
fn each_tab_scrolls_on_its_own() {
    let wheel = UiInput::new(vec![
        InputEvent::PointerMoved {
            position: [640.0, 300.0],
        },
        InputEvent::Wheel { delta: 1.0 },
    ]);
    let mut screen = StatsScreen::new();
    run(&mut screen, wheel, &data(), false);
    // 40 rows of 10 units: one notch is half a row, 5 units
    assert_eq!(screen.offset(StatsTab::General), 5.0);
    assert_eq!(screen.offset(StatsTab::Mobs), 0.0);
}

#[test]
fn clicking_a_column_header_sorts_the_table() {
    let mut screen = StatsScreen::new();
    let data = data();
    run(
        &mut screen,
        click_at(stats_tab_rect(VIEWPORT, StatsTab::Blocks)),
        &data,
        false,
    );
    screen.refresh(&data);
    // id order first
    assert_eq!(screen.orders[0], [0, 1, 2]);
    // GUI (cx - 108 + 100, list top + 4 + 10) = (205, 46), scale 3
    run(
        &mut screen,
        click_point([205.0 * 3.0, 46.0 * 3.0]),
        &data,
        false,
    );
    assert_eq!(screen.orders[0], [1, 2, 0], "crafted, most first");
}

#[test]
fn a_press_outside_the_header_does_not_sort() {
    let mut screen = StatsScreen::new();
    let data = data();
    run(
        &mut screen,
        click_at(stats_tab_rect(VIEWPORT, StatsTab::Blocks)),
        &data,
        false,
    );
    screen.refresh(&data);
    let on_a_row = click_point([205.0 * 3.0, 80.0 * 3.0]);
    run(&mut screen, on_a_row, &data, false);
    assert_eq!(screen.orders[0], [0, 1, 2]);
}

#[test]
fn the_loading_marker_moves_with_the_ticks() {
    let mut screen = StatsScreen::new();
    let first = LOADING_MARKERS[(screen.ticks / TICKS_PER_MARKER) as usize % 7];
    for _ in 0..TICKS_PER_MARKER {
        screen.tick();
    }
    let second = LOADING_MARKERS[(screen.ticks / TICKS_PER_MARKER) as usize % 7];
    assert_ne!(first, second);
}

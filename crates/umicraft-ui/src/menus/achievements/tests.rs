// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the achievements screen: loading, zoom, drag and the tooltip.

use super::*;
use crate::draw::DrawCommand;
use crate::input::{InputEvent, Key, PointerButton, UiInput};
use crate::menus::common::testing::{click_at, frame, key, VIEWPORT};
use crate::widgets::SlotItem;

fn node(name: &str, cell: (i32, i32), parent: Option<usize>, unlocked: bool) -> AchievementNode {
    AchievementNode {
        column: cell.0,
        row: cell.1,
        special: false,
        icon: SlotItem {
            key: 0,
            count: 1,
            damage: 0,
            max_durability: 0,
            glint: false,
        },
        name: name.to_string(),
        description: format!("About {name}"),
        parent,
        unlocked,
    }
}

fn data() -> AchievementsData {
    AchievementsData {
        nodes: vec![
            node("Root", (0, 0), None, true),
            node("Child", (2, 1), Some(0), false),
            node("Grandchild", (4, -1), Some(1), false),
        ],
    }
}

fn run(
    screen: &mut AchievementsScreen,
    input: UiInput,
    loading: bool,
) -> Option<AchievementsOutcome> {
    frame(input, |ui| {
        let atlas = FontAtlas::fallback();
        screen.show(ui, &atlas, &Lang::builtin(), &data(), (loading, 7))
    })
}

/// Every text the frame draws.
fn texts(screen: &mut AchievementsScreen, input: UiInput, loading: bool) -> Vec<String> {
    frame(input, |ui| {
        let atlas = FontAtlas::fallback();
        screen.show(ui, &atlas, &Lang::builtin(), &data(), (loading, 7));
        ui.draw()
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    })
}

fn wheel(delta: f32) -> UiInput {
    UiInput::new(vec![InputEvent::Wheel { delta }])
}

fn hover(gui_x: f32, gui_y: f32) -> UiInput {
    // The tests run at GUI scale 3.
    let position = [gui_x * 3.0, gui_y * 3.0];
    UiInput::new(vec![InputEvent::PointerMoved { position }])
}

#[test]
fn the_done_button_sits_right_of_the_centre_below_the_window() {
    // scale 3: (213 + 24) * 3, (120 + 74) * 3
    let done = achievements_done_rect(VIEWPORT);
    assert_eq!((done.x, done.y, done.width), (711.0, 582.0, 240.0));
    let mut screen = AchievementsScreen::new();
    let clicked = run(&mut screen, click_at(done), false);
    assert_eq!(clicked, Some(AchievementsOutcome::Done));
    let escaped = run(&mut screen, key(Key::Escape), false);
    assert_eq!(escaped, Some(AchievementsOutcome::Done));
    assert_eq!(run(&mut screen, UiInput::default(), false), None);
}

#[test]
fn loading_shows_the_download_text_and_no_button() {
    let mut screen = AchievementsScreen::new();
    let done = achievements_done_rect(VIEWPORT);
    assert_eq!(run(&mut screen, click_at(done), true), None);
    let escaped = run(&mut screen, key(Key::Escape), true);
    assert_eq!(escaped, Some(AchievementsOutcome::Done));
    let shown = texts(&mut screen, UiInput::default(), true);
    assert!(shown.iter().any(|text| text == "oooooo"), "{shown:?}");
}

#[test]
fn the_wheel_zooms_between_one_and_two() {
    let mut screen = AchievementsScreen::new();
    for _ in 0..6 {
        run(&mut screen, wheel(1.0), false);
    }
    assert_eq!(screen.zoom(), 2.0);
    for _ in 0..6 {
        run(&mut screen, wheel(-1.0), false);
    }
    assert_eq!(screen.zoom(), 1.0);
}

#[test]
fn dragging_the_map_scrolls_it_the_other_way() {
    let mut screen = AchievementsScreen::new();
    run(&mut screen, UiInput::default(), false);
    let before = screen.scroll.map(|position| position[0]);
    // The window is at GUI (85, 19); press inside the drag area and move
    // 10 units right.
    let (from, to) = ([200.0 * 3.0, 100.0 * 3.0], [210.0 * 3.0, 100.0 * 3.0]);
    let input = UiInput::new(vec![
        InputEvent::PointerPressed {
            position: from,
            button: PointerButton::Left,
        },
        InputEvent::PointerMoved { position: to },
    ]);
    run(&mut screen, input, false);
    let after = screen.scroll.map(|position| position[0]);
    assert_eq!(after, before.map(|x| x - 10.0));
    let release = UiInput::new(vec![InputEvent::PointerReleased {
        position: to,
        button: PointerButton::Left,
    }]);
    run(&mut screen, release, false);
    assert_eq!(screen.drag, None);
}

#[test]
fn the_map_starts_centred_on_the_first_achievement() {
    let mut screen = AchievementsScreen::new();
    run(&mut screen, UiInput::default(), false);
    // 0 * 24 - 141 / 2 - 12 and 0 * 24 - 141 / 2
    assert_eq!(screen.scroll, Some([-82.0, -70.0]));
}

#[test]
fn the_title_counts_the_unlocked_achievements() {
    let mut screen = AchievementsScreen::new();
    let shown = texts(&mut screen, UiInput::default(), false);
    assert!(shown.iter().any(|text| text == "1/3"), "{shown:?}");
}

#[test]
fn hovering_an_achievement_shows_its_name_and_description() {
    let mut screen = AchievementsScreen::new();
    run(&mut screen, UiInput::default(), false);
    // Root is at map (82, 70); the map origin is GUI (85 + 16, 19 + 17).
    let shown = texts(&mut screen, hover(101.0 + 93.0, 36.0 + 81.0), false);
    assert!(shown.iter().any(|text| text == "Root"), "{shown:?}");
    assert!(shown.iter().any(|text| text == "About Root"), "{shown:?}");
    assert!(shown.iter().any(|text| text == "Taken!"), "{shown:?}");
}

#[test]
fn a_locked_achievement_asks_for_its_parent() {
    let mut screen = AchievementsScreen::new();
    run(&mut screen, UiInput::default(), false);
    // Grandchild is at map (4 * 24 + 82, -24 + 70) = (178, 46).
    let shown = texts(&mut screen, hover(101.0 + 189.0, 36.0 + 57.0), false);
    assert!(shown.iter().any(|text| text == "Grandchild"), "{shown:?}");
    assert!(shown.iter().any(|text| text.contains("Child")), "{shown:?}");
}

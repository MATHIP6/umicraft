// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;
use crate::draw::DrawCommand;
use crate::geom::Rect;
use crate::input::{Key, UiInput};
use crate::menus::common::testing::{
    click_at, click_point, frame, key, lang_with_account_texts, VIEWPORT,
};

pub(super) fn rows() -> Vec<AccountRow> {
    ["Alpha", "Beta", "Gamma"]
        .iter()
        .enumerate()
        .map(|(i, name)| AccountRow {
            username: (*name).into(),
            badge: if i == 1 {
                AccountBadge::Microsoft
            } else {
                AccountBadge::Offline
            },
            face: None,
        })
        .collect()
}

pub(super) fn run(screen: &mut AccountsScreen, input: UiInput) -> Option<AccountsOutcome> {
    frame(input, |ui| {
        screen.show(ui, &FontAtlas::fallback(), &lang_with_account_texts())
    })
}

pub(super) fn texts(screen: &mut AccountsScreen) -> Vec<String> {
    frame(UiInput::default(), |ui| {
        screen.show(ui, &FontAtlas::fallback(), &lang_with_account_texts());
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

fn center_of_row(index: usize) -> [f32; 2] {
    let rect = account_row_rect(VIEWPORT, index, 0.0);
    [rect.x + rect.width / 2.0, rect.y + rect.height / 2.0]
}

#[test]
fn the_layout_has_the_server_list_coordinates() {
    let layout = accounts_layout(VIEWPORT);
    // scale 3, cx = 213, height = 240
    assert_eq!(layout.list.y, 96.0);
    assert_eq!(layout.list.height, 144.0 * 3.0);
    assert_eq!(layout.play, Rect::new(59.0 * 3.0, 188.0 * 3.0, 450.0, 60.0));
    assert_eq!(layout.add.x, 217.0 * 3.0);
    assert_eq!(
        layout.cancel,
        Rect::new(293.0 * 3.0, 212.0 * 3.0, 225.0, 60.0)
    );
}

#[test]
fn rows_are_hit_inside_their_box_only() {
    assert_eq!(account_row_rect(VIEWPORT, 1, 0.0).y, (36.0 + 36.0) * 3.0);
    assert_eq!(account_row_at(VIEWPORT, center_of_row(2), 0.0), Some(2));
    // The 4 units between two rows and the space above the first one.
    let gap = [center_of_row(0)[0], (36.0 + 34.0) * 3.0];
    assert_eq!(account_row_at(VIEWPORT, gap, 0.0), None);
    assert_eq!(
        account_row_at(VIEWPORT, [center_of_row(0)[0], 100.0], 0.0),
        None
    );
    assert_eq!(
        account_row_at(VIEWPORT, [10.0, center_of_row(0)[1]], 0.0),
        None
    );
    // Scrolled by one slot, the second row sits where the first was.
    assert_eq!(account_row_at(VIEWPORT, center_of_row(0), 36.0), Some(1));
}

#[test]
fn scrolling_to_a_row_keeps_it_inside_the_view() {
    // Row 2 spans 76..108 plus 4 of padding in a 100 unit view.
    assert_eq!(offset_to_show(2, 0.0, 100.0), 12.0);
    assert_eq!(offset_to_show(2, 0.0, 140.0), 0.0);
    assert_eq!(offset_to_show(0, 100.0, 100.0), 0.0);
}

#[test]
fn usernames_are_one_to_sixteen_word_characters() {
    for valid in ["a", "Steve_01", "ABCDEFGHIJKLMNOP", "_"] {
        assert!(is_valid_username(valid), "{valid}");
    }
    for invalid in ["", "ABCDEFGHIJKLMNOPQ", "a b", "é", "a-b", "a.b"] {
        assert!(!is_valid_username(invalid), "{invalid}");
    }
}

#[test]
fn the_rows_show_their_badges() {
    let mut screen = AccountsScreen::new(rows(), None);
    let shown = texts(&mut screen);
    assert!(shown.iter().any(|t| t == "Alpha"));
    assert!(shown.iter().any(|t| t == "[Free]"));
    assert!(shown.iter().any(|t| t == "[Microsoft]"));
    assert_eq!(
        AccountBadge::Offline.key(),
        "umicraft.accounts.badge.offline"
    );
    assert_ne!(
        AccountBadge::Offline.color(),
        AccountBadge::Microsoft.color()
    );
}

#[test]
fn a_click_selects_and_play_needs_a_selection() {
    let layout = accounts_layout(VIEWPORT);
    let mut screen = AccountsScreen::new(rows(), None);
    assert_eq!(run(&mut screen, click_at(layout.play)), None);
    assert_eq!(run(&mut screen, click_point(center_of_row(1))), None);
    assert_eq!(screen.selected(), Some(1));
    assert_eq!(
        run(&mut screen, click_at(layout.play)),
        Some(AccountsOutcome::Play(1))
    );
}

#[test]
fn a_quick_second_click_plays_and_a_slow_one_does_not() {
    let mut screen = AccountsScreen::new(rows(), None);
    run(&mut screen, click_point(center_of_row(2)));
    screen.tick();
    assert_eq!(
        run(&mut screen, click_point(center_of_row(2))),
        Some(AccountsOutcome::Play(2))
    );
    run(&mut screen, click_point(center_of_row(0)));
    for _ in 0..=DOUBLE_CLICK_TICKS {
        screen.tick();
    }
    assert_eq!(run(&mut screen, click_point(center_of_row(0))), None);
}

#[test]
fn the_keys_move_the_selection_and_enter_plays() {
    let mut screen = AccountsScreen::new(rows(), None);
    run(&mut screen, key(Key::ArrowDown));
    assert_eq!(screen.selected(), Some(0));
    run(&mut screen, key(Key::ArrowDown));
    run(&mut screen, key(Key::ArrowDown));
    run(&mut screen, key(Key::ArrowDown));
    assert_eq!(screen.selected(), Some(2));
    run(&mut screen, key(Key::ArrowUp));
    assert_eq!(screen.selected(), Some(1));
    assert_eq!(
        run(&mut screen, key(Key::Enter)),
        Some(AccountsOutcome::Play(1))
    );
    let mut empty = AccountsScreen::new(Vec::new(), None);
    run(&mut empty, key(Key::ArrowUp));
    assert_eq!(empty.selected(), None);
}

#[test]
fn the_plain_buttons_report_their_outcome() {
    let layout = accounts_layout(VIEWPORT);
    let mut screen = AccountsScreen::new(rows(), Some(0));
    for (rect, expected) in [
        (layout.refresh, AccountsOutcome::Refresh),
        (layout.cancel, AccountsOutcome::Cancel),
    ] {
        assert_eq!(run(&mut screen, click_at(rect)), Some(expected));
    }
    assert_eq!(
        run(&mut screen, key(Key::Escape)),
        Some(AccountsOutcome::Cancel)
    );
}

#[test]
fn busy_greys_the_work_buttons_but_not_cancel() {
    let layout = accounts_layout(VIEWPORT);
    let mut screen = AccountsScreen::new(rows(), Some(2));
    screen.set_busy(true);
    for rect in [
        layout.play,
        layout.add,
        layout.edit,
        layout.delete,
        layout.refresh,
    ] {
        assert_eq!(run(&mut screen, click_at(rect)), None);
    }
    assert!(matches!(screen.dialog, Dialog::None));
    assert_eq!(
        run(&mut screen, click_at(layout.cancel)),
        Some(AccountsOutcome::Cancel)
    );
}

#[test]
fn the_error_is_shown_until_cleared() {
    let mut screen = AccountsScreen::new(rows(), None);
    screen.set_error(Some("no network".into()));
    assert!(texts(&mut screen).iter().any(|t| t == "Error: no network"));
    screen.set_error(None);
    assert!(!texts(&mut screen).iter().any(|t| t.starts_with("Error")));
}

#[test]
fn replacing_the_rows_drops_a_selection_that_is_gone() {
    let mut screen = AccountsScreen::new(rows(), Some(2));
    screen.set_rows(rows().into_iter().take(1).collect());
    assert_eq!(screen.selected(), None);
    assert_eq!(AccountsScreen::new(rows(), Some(9)).selected(), None);
}

#[test]
fn an_empty_list_says_there_is_no_account_and_how_to_add_one() {
    let mut screen = AccountsScreen::new(Vec::new(), None);
    let shown = texts(&mut screen);
    assert!(
        shown.iter().any(|t| t == "You have no account."),
        "{shown:?}"
    );
    assert!(shown.iter().any(|t| t.contains("Add account")), "{shown:?}");
}

#[test]
fn the_empty_message_goes_away_once_there_are_accounts() {
    let mut screen = AccountsScreen::new(rows(), None);
    assert!(!texts(&mut screen)
        .iter()
        .any(|t| t == "You have no account."));
}

/// The pixel at `(dx, dy)` GUI units from the top-left corner of row `index`.
fn on_row(index: usize, dx: f32, dy: f32) -> [f32; 2] {
    let rect = account_row_rect(VIEWPORT, index, 0.0);
    [rect.x + dx * 3.0, rect.y + dy * 3.0]
}

fn shifted(code: Key) -> UiInput {
    UiInput {
        modifiers: crate::input::Modifiers {
            shift: true,
            ..Default::default()
        },
        ..key(code)
    }
}

#[test]
fn the_head_arrows_play_and_move_like_the_server_list() {
    let mut screen = AccountsScreen::new(rows(), None);
    assert_eq!(
        run(&mut screen, click_point(on_row(1, 24.0, 10.0))),
        Some(AccountsOutcome::Play(1)),
        "the play arrow"
    );
    assert_eq!(
        run(&mut screen, click_point(on_row(1, 5.0, 5.0))),
        Some(AccountsOutcome::MoveUp(1))
    );
    assert_eq!(screen.selected(), Some(0), "the selection follows the row");
    assert_eq!(
        run(&mut screen, click_point(on_row(1, 5.0, 25.0))),
        Some(AccountsOutcome::MoveDown(1))
    );
    assert_eq!(screen.selected(), Some(2));
}

#[test]
fn the_first_row_has_no_up_arrow_and_the_last_no_down_arrow() {
    let mut screen = AccountsScreen::new(rows(), None);
    assert_eq!(run(&mut screen, click_point(on_row(0, 5.0, 5.0))), None);
    assert_eq!(screen.selected(), Some(0), "that zone only selects the row");
    assert_eq!(run(&mut screen, click_point(on_row(2, 5.0, 25.0))), None);
    assert_eq!(screen.selected(), Some(2));
}

#[test]
fn a_busy_screen_does_not_play_from_the_arrow() {
    let mut screen = AccountsScreen::new(rows(), None);
    screen.set_busy(true);
    assert_eq!(run(&mut screen, click_point(on_row(1, 24.0, 10.0))), None);
}

#[test]
fn shift_arrows_move_the_account_and_the_selection_follows() {
    let mut screen = AccountsScreen::new(rows(), Some(1));
    assert_eq!(
        run(&mut screen, shifted(Key::ArrowUp)),
        Some(AccountsOutcome::MoveUp(1))
    );
    assert_eq!(screen.selected(), Some(0));
    assert_eq!(run(&mut screen, shifted(Key::ArrowUp)), None);
    assert_eq!(screen.selected(), Some(0), "the first row cannot move up");
    assert_eq!(
        run(&mut screen, shifted(Key::ArrowDown)),
        Some(AccountsOutcome::MoveDown(0))
    );
    assert_eq!(screen.selected(), Some(1));
}

#[test]
fn a_hovered_row_draws_the_arrows_over_its_head() {
    // The glyphs stand in for the arrow sheet, and each is drawn with a shadow.
    let arrows = |pointer: Option<[f32; 2]>| {
        let mut screen = AccountsScreen::new(rows(), None);
        let input = UiInput::new(
            pointer
                .map(|position| crate::input::InputEvent::PointerMoved { position })
                .into_iter()
                .collect(),
        );
        frame(input, |ui| {
            screen.show(ui, &FontAtlas::fallback(), &lang_with_account_texts());
            let glyphs: std::collections::BTreeSet<&str> = ui
                .draw()
                .commands()
                .iter()
                .filter_map(|c| match c {
                    DrawCommand::Text { text, .. } if matches!(text.as_str(), ">" | "^" | "v") => {
                        Some(text.as_str())
                    }
                    _ => None,
                })
                .collect();
            glyphs.len()
        })
    };
    assert_eq!(arrows(None), 0);
    // The middle row shows play, up and down.
    assert_eq!(arrows(Some(on_row(1, 5.0, 5.0))), 3);
    // The first row has no up arrow.
    assert_eq!(arrows(Some(on_row(0, 5.0, 5.0))), 2);
}

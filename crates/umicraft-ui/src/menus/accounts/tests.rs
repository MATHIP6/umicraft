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

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the dialogs: edit, add, delete and the device code.

use super::tests::{rows, run, texts};
use super::*;
use crate::input::{InputEvent, Key, UiInput};
use crate::menus::common::testing::{click_at, key, lang_with_account_texts, VIEWPORT};
use crate::menus::confirm::confirm_layout;

#[test]
fn edit_is_for_offline_accounts_and_returns_the_new_name() {
    let layout = accounts_layout(VIEWPORT);
    let mut screen = AccountsScreen::new(rows(), Some(1));
    // Microsoft account: the button is disabled.
    assert_eq!(run(&mut screen, click_at(layout.edit)), None);
    assert!(matches!(screen.dialog, Dialog::None));

    screen.selected = Some(2);
    assert_eq!(run(&mut screen, click_at(layout.edit)), None);
    let dialog = name_dialog_layout(VIEWPORT, false);
    assert_eq!(
        run(&mut screen, click_at(dialog.primary)),
        Some(AccountsOutcome::EditOffline {
            index: 2,
            username: "Gamma".into(),
        })
    );
    assert!(matches!(screen.dialog, Dialog::None));
}

#[test]
fn the_add_dialog_validates_live_and_offers_both_kinds() {
    let layout = accounts_layout(VIEWPORT);
    let dialog = name_dialog_layout(VIEWPORT, true);
    let mut screen = AccountsScreen::new(rows(), None);
    let mut ui = Ui::new(VIEWPORT);
    let mut step = |screen: &mut AccountsScreen, input: UiInput| {
        ui.begin_frame(VIEWPORT, input);
        let outcome = screen.show(&mut ui, &FontAtlas::fallback(), &lang_with_account_texts());
        ui.end_frame();
        outcome
    };
    step(&mut screen, click_at(layout.add));
    step(&mut screen, UiInput::default());
    let typed = |text: &str| UiInput::new(vec![InputEvent::Text(text.into())]);
    step(&mut screen, typed("bad name"));
    // An invalid name leaves the offline button dead and Enter ignored.
    assert_eq!(step(&mut screen, click_at(dialog.primary)), None);
    assert_eq!(step(&mut screen, key(Key::Enter)), None);
    assert_eq!(
        step(&mut screen, click_at(dialog.microsoft)),
        Some(AccountsOutcome::AddMicrosoft)
    );

    step(&mut screen, click_at(layout.add));
    step(&mut screen, UiInput::default());
    step(&mut screen, typed("Bob_1"));
    assert_eq!(
        step(&mut screen, click_at(dialog.primary)),
        Some(AccountsOutcome::AddOffline("Bob_1".into()))
    );
}

#[test]
fn cancelling_the_add_dialog_returns_to_the_list() {
    let dialog = name_dialog_layout(VIEWPORT, true);
    let mut screen = AccountsScreen::new(rows(), None);
    screen.dialog = Dialog::Add(AddDialog::new());
    assert_eq!(run(&mut screen, click_at(dialog.cancel)), None);
    assert!(matches!(screen.dialog, Dialog::None));
    screen.dialog = Dialog::Add(AddDialog::new());
    assert_eq!(run(&mut screen, key(Key::Escape)), None);
    assert!(matches!(screen.dialog, Dialog::None));
}

#[test]
fn delete_asks_for_confirmation_first() {
    let layout = accounts_layout(VIEWPORT);
    let confirm = confirm_layout(VIEWPORT);
    let mut screen = AccountsScreen::new(rows(), Some(1));
    assert_eq!(run(&mut screen, click_at(layout.delete)), None);
    assert!(texts(&mut screen)
        .iter()
        .any(|t| t == "'Beta' will be removed."));
    assert_eq!(run(&mut screen, click_at(confirm.no)), None);
    assert!(matches!(screen.dialog, Dialog::None));
    run(&mut screen, click_at(layout.delete));
    assert_eq!(
        run(&mut screen, click_at(confirm.yes)),
        Some(AccountsOutcome::Delete(1))
    );
}

#[test]
fn the_device_code_dialog_shows_the_code_and_cancels_the_login() {
    let mut screen = AccountsScreen::new(rows(), Some(0));
    screen.set_device_code(Some(DeviceCodePrompt {
        user_code: "AB12CD".into(),
        url: "https://example.test/link".into(),
    }));
    let shown = texts(&mut screen);
    assert!(shown.iter().any(|t| t == "AB12CD"));
    assert!(shown.iter().any(|t| t == "https://example.test/link"));
    assert!(!shown.iter().any(|t| t == "Alpha"), "the list is covered");
    let cancel = crate::menus::connecting::connecting_cancel_rect(VIEWPORT);
    assert_eq!(
        run(&mut screen, click_at(cancel)),
        Some(AccountsOutcome::CancelLogin)
    );
    assert_eq!(
        run(&mut screen, key(Key::Escape)),
        Some(AccountsOutcome::CancelLogin)
    );
    screen.set_device_code(None);
    assert_eq!(
        run(&mut screen, key(Key::Escape)),
        Some(AccountsOutcome::Cancel)
    );
}

#[test]
fn the_device_code_dialog_opens_and_copies_the_link_and_the_code() {
    let mut screen = AccountsScreen::new(rows(), Some(0));
    screen.set_device_code(Some(DeviceCodePrompt {
        user_code: "AB12CD".into(),
        url: "https://example.test/link".into(),
    }));
    let layout = device_code_dialog::device_layout(VIEWPORT);
    assert_eq!(
        run(&mut screen, click_at(layout.open)),
        Some(AccountsOutcome::OpenUrl("https://example.test/link".into()))
    );
    assert_eq!(
        run(&mut screen, click_at(layout.copy_link)),
        Some(AccountsOutcome::CopyText(
            "https://example.test/link".into()
        ))
    );
    assert_eq!(
        run(&mut screen, click_at(layout.copy_code)),
        Some(AccountsOutcome::CopyText("AB12CD".into()))
    );
}

#[test]
fn the_device_code_buttons_do_not_overlap_each_other_or_cancel() {
    let layout = device_code_dialog::device_layout(VIEWPORT);
    let cancel = crate::menus::connecting::connecting_cancel_rect(VIEWPORT);
    let rects = [layout.open, layout.copy_link, layout.copy_code, cancel];
    for (i, a) in rects.iter().enumerate() {
        for b in &rects[i + 1..] {
            let apart =
                a.right() <= b.x || b.right() <= a.x || a.bottom() <= b.y || b.bottom() <= a.y;
            assert!(apart, "{a:?} overlaps {b:?}");
        }
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the page flow: the list, the settings, and the way back.

use super::*;
use crate::menus::common::testing::click_at;

fn escape() -> UiInput {
    UiInput::new(vec![InputEvent::KeyPressed(Key::Escape)])
}

#[test]
fn the_menu_opens_on_the_list() {
    let mut screen = ModsScreen::new();
    assert!(!screen.can_go_back(), "the list is the root: Escape closes");
    let names = texts(&mut screen, &view());
    assert!(has_text(&names, "FPS"), "{names:?}");
    assert!(has_text(&names, "umicraft.mods.hint"), "{names:?}");
}

#[test]
fn the_cross_of_the_panel_closes_the_menu() {
    let mut screen = on_list();
    let cross = mods_layout(VIEWPORT).close;
    assert_eq!(show(&mut screen, click_at(cross)), Some(ModsOutcome::Close));
}

#[test]
fn escape_on_the_settings_goes_back_to_the_list_and_on_the_list_is_the_windows() {
    let mut screen = on_settings_page();
    assert!(screen.can_go_back());
    assert_eq!(show(&mut screen, escape()), None);
    assert!(!screen.can_go_back(), "the list is reached");
    assert!(has_text(&texts(&mut screen, &view()), "Coordinates"));
    // On the list the key is the window's, which closes the menu: the menu
    // itself keeps its page and answers nothing.
    assert_eq!(show(&mut screen, escape()), None);
    assert!(!screen.can_go_back());
    assert!(has_text(&texts(&mut screen, &view()), "Coordinates"));
}

/// Regression: Escape on the settings page closed the whole menu because the
/// window took it before the UI.
#[test]
fn a_page_whose_mod_vanished_goes_back_to_the_list() {
    let mut screen = on_settings_page();
    let mut gone = view();
    gone.cards.remove(0);
    assert_eq!(show_view(&mut screen, &gone, escape()), None);
    assert!(!screen.can_go_back());
}

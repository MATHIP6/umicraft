// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the page flow: landing, list, settings, and the way back.

use super::*;
use crate::menus::common::testing::click_at;

fn escape() -> UiInput {
    UiInput::new(vec![InputEvent::KeyPressed(Key::Escape)])
}

#[test]
fn the_menu_opens_on_the_landing_page_not_the_list() {
    let mut screen = ModsScreen::new();
    assert!(
        !screen.can_go_back(),
        "the landing is the root: Escape closes"
    );
    let names = texts(&mut screen, &view());
    assert!(!has_text(&names, "FPS"), "the cards are not shown yet");
}

#[test]
fn the_mods_button_opens_the_list() {
    let mut screen = ModsScreen::new();
    let button = landing_layout(VIEWPORT).mods;
    assert_eq!(show(&mut screen, click_at(button)), None);
    assert!(screen.can_go_back());
    assert!(has_text(&texts(&mut screen, &view()), "Coordinates"));
}

#[test]
fn the_cross_of_the_landing_closes_the_menu() {
    let cross = landing_layout(VIEWPORT).close;
    assert_eq!(
        show(&mut ModsScreen::new(), click_at(cross)),
        Some(ModsOutcome::Close)
    );
}

#[test]
fn the_cross_of_the_panel_goes_back_to_the_landing_page() {
    let mut screen = on_list();
    let cross = mods_layout(VIEWPORT).close;
    assert_eq!(show(&mut screen, click_at(cross)), None);
    assert!(!screen.can_go_back());
    assert!(!has_text(&texts(&mut screen, &view()), "FPS"));
}

#[test]
fn escape_walks_back_one_page_at_a_time() {
    let mut screen = on_settings_page();
    assert_eq!(show(&mut screen, escape()), None);
    assert!(screen.can_go_back(), "the list is reached");
    assert!(has_text(&texts(&mut screen, &view()), "Coordinates"));
    assert_eq!(show(&mut screen, escape()), None);
    assert!(!screen.can_go_back(), "the landing page is reached");
    // On the landing page the key is the window's, which closes the menu.
    assert_eq!(show(&mut screen, escape()), None);
    assert!(!screen.can_go_back());
}

/// Regression: Escape on the settings page closed the whole menu because the
/// window took it before the UI.
#[test]
fn a_page_whose_mod_vanished_goes_back_to_the_list() {
    let mut screen = on_settings_page();
    let mut gone = view();
    gone.cards.remove(0);
    assert_eq!(show_view(&mut screen, &gone, escape()), None);
    assert!(
        !screen.can_go_back(),
        "the vanished page fell to the list, Escape went on to the landing"
    );
}

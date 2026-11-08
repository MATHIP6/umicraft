// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the landing page of the mods menu (the logo, the wordmark, the Mods
//! button and the cross), restored from the menu that had no pages.

use super::landing_layout::landing_layout;
use super::*;
use crate::menus::common::testing::{click_at, VIEWPORT};
use crate::menus::common::Gui;

#[test]
fn the_group_is_centred_on_the_screen() {
    let layout = landing_layout(VIEWPORT);
    let half_unit = Gui::new(VIEWPORT).scale * 0.5;
    let centre = VIEWPORT.x + VIEWPORT.width * 0.5;
    for rect in [layout.logo, layout.mods] {
        assert!(
            (rect.x + rect.width * 0.5 - centre).abs() <= half_unit,
            "{rect:?}"
        );
    }
    let middle = VIEWPORT.y + VIEWPORT.height * 0.5;
    assert!(layout.logo.y < middle && layout.hint[1] > middle);
}

#[test]
fn the_parts_stack_from_the_logo_down_to_the_hint() {
    let layout = landing_layout(VIEWPORT);
    let bottom = |rect: Rect| rect.y + rect.height;
    assert!(bottom(layout.logo) <= layout.wordmark.y);
    assert!(bottom(layout.wordmark) <= layout.mods.y);
    assert!(bottom(layout.mods) <= layout.hint[1]);
}

#[test]
fn the_menu_is_small() {
    // The group is much smaller than the screen and its button is smaller
    // than a title-screen button (160 x 22 units).
    let layout = landing_layout(VIEWPORT);
    let group = layout.hint[1] - layout.logo.y;
    assert!(group < VIEWPORT.height * 0.5, "{group}");
    assert!(layout.mods.width < 160.0 * Gui::new(VIEWPORT).scale);
}

#[test]
fn the_cross_is_in_the_top_right_corner_of_the_screen() {
    let layout = landing_layout(VIEWPORT);
    assert!(layout.close.x > VIEWPORT.x + VIEWPORT.width * 0.5);
    assert!(layout.close.y < VIEWPORT.y + VIEWPORT.height * 0.2);
}

#[test]
fn the_two_buttons_open_the_list_and_close_the_menu() {
    let layout = landing_layout(VIEWPORT);
    let mut screen = ModsScreen::new();
    assert_eq!(show(&mut screen, click_at(layout.mods)), None);
    assert!(screen.can_go_back(), "the Mods button opened the list");
    assert_eq!(
        show(&mut ModsScreen::new(), click_at(layout.close)),
        Some(ModsOutcome::Close)
    );
}

#[test]
fn a_click_beside_the_buttons_does_nothing() {
    let layout = landing_layout(VIEWPORT);
    let mut screen = ModsScreen::new();
    let away = Rect::new(VIEWPORT.x + 10.0, VIEWPORT.y + 10.0, 8.0, 8.0);
    assert_eq!(show(&mut screen, click_at(away)), None);
    let _ = layout;
}

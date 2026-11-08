// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the list page of the mods menu: the cards, the search and the tabs.

use super::*;
use crate::modern::theme;

#[test]
fn the_cards_name_and_describe_every_mod() {
    let names = texts(&mut on_list(), &view());
    for wanted in [
        "FPS",
        "Coordinates",
        "Toggle Sprint",
        "About FPS",
        "HUD",
        "Gameplay",
    ] {
        assert!(has_text(&names, wanted), "{wanted} in {names:?}");
    }
}

#[test]
fn the_switch_of_a_card_asks_for_the_other_state() {
    let (switch, _) = controls(first_card(), mods_layout(VIEWPORT).scale);
    let outcome = show(&mut on_list(), click_at(switch));
    assert_eq!(
        outcome,
        Some(ModsOutcome::Toggle {
            id: "fps".into(),
            enabled: true
        })
    );
    let mut on = view();
    on.cards[0].enabled = true;
    let outcome = show_view(&mut on_list(), &on, click_at(switch));
    assert_eq!(
        outcome,
        Some(ModsOutcome::Toggle {
            id: "fps".into(),
            enabled: false
        })
    );
}

#[test]
fn a_click_beside_everything_does_nothing() {
    let away = Rect::new(VIEWPORT.x + 4.0, VIEWPORT.y + 4.0, 4.0, 4.0);
    assert_eq!(show(&mut on_list(), click_at(away)), None);
}

#[test]
fn typing_filters_the_cards_by_name() {
    let mut screen = on_list();
    let typed = UiInput::new(vec![InputEvent::Text("sprint".into())]);
    show(&mut screen, typed);
    let names = texts(&mut screen, &view());
    assert!(has_text(&names, "Toggle Sprint"));
    assert!(!has_text(&names, "FPS") && !has_text(&names, "Coordinates"));
    let erase = vec![InputEvent::KeyPressed(Key::Backspace); 6];
    show(&mut screen, UiInput::new(erase));
    assert!(has_text(&texts(&mut screen, &view()), "FPS"));
}

#[test]
fn a_search_without_result_says_so() {
    let mut screen = on_list();
    show(
        &mut screen,
        UiInput::new(vec![InputEvent::Text("zzz".into())]),
    );
    let names = texts(&mut screen, &view());
    assert!(has_text(&names, "umicraft.mods.empty"), "{names:?}");
}

#[test]
fn a_category_tab_keeps_only_its_mods() {
    let layout = mods_layout(VIEWPORT);
    let mut screen = on_list();
    // The tabs are laid out left to right: All, HUD, Gameplay. Gameplay is
    // the last, so a click near the right end of the labels finds it.
    let gameplay = {
        let atlas = FontAtlas::fallback();
        let size = theme::TEXT_SIZE_SMALL * layout.scale;
        let pad = 5.0 * layout.scale;
        let gap = 2.0 * layout.scale;
        let width = |text: &str| modern::measure_text(&atlas, text, size) + 2.0 * pad;
        layout.tabs.x + width("umicraft.mods.tab.all") + width("HUD") + 2.0 * gap + 4.0
    };
    let click = crate::menus::common::testing::click_point([gameplay, layout.tabs.y + 4.0]);
    show(&mut screen, click);
    let names = texts(&mut screen, &view());
    assert!(has_text(&names, "Toggle Sprint"));
    assert!(!has_text(&names, "FPS"));
}

#[test]
fn the_pointer_is_not_needed_to_type() {
    // Typing works without any click on the search box first.
    let mut screen = on_list();
    let events = vec![
        InputEvent::PointerMoved {
            position: [0.0, 0.0],
        },
        InputEvent::Text("c".into()),
        InputEvent::PointerReleased {
            position: [0.0, 0.0],
            button: PointerButton::Left,
        },
    ];
    show(&mut screen, UiInput::new(events));
    let names = texts(&mut screen, &view());
    assert!(has_text(&names, "Coordinates") && !has_text(&names, "FPS"));
}

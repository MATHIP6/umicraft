// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the settings page of the mods menu.

use super::*;
use crate::menus::common::testing::click_at;

#[test]
fn the_settings_page_says_what_the_mod_does() {
    let names = texts(&mut on_settings_page(), &view());
    assert!(has_text(&names, "About FPS"), "{names:?}");
}

#[test]
fn the_gear_opens_the_settings_page_and_back_returns_to_the_list() {
    let mut screen = on_settings_page();
    let page = texts(&mut screen, &view());
    for wanted in [
        "Label shadow",
        "Label size",
        "Label mode",
        "Label color",
        "Split",
    ] {
        assert!(has_text(&page, wanted), "{wanted} in {page:?}");
    }
    assert!(!has_text(&page, "Coordinates"), "the list is hidden");
    let back = mods_layout(VIEWPORT).back;
    assert_eq!(show(&mut screen, click_at(back)), None);
    assert!(has_text(&texts(&mut screen, &view()), "Coordinates"));
}

#[test]
fn a_page_whose_mod_vanished_falls_back_to_the_list() {
    let mut screen = on_settings_page();
    let mut gone = view();
    gone.cards.remove(0);
    assert!(has_text(&texts(&mut screen, &gone), "Coordinates"));
}

#[test]
fn a_toggle_setting_reports_its_new_value() {
    let mut screen = on_settings_page();
    let scale = mods_layout(VIEWPORT).scale;
    let outcome = show(&mut screen, click_at(toggle_track(row(0), scale)));
    assert_eq!(
        outcome,
        Some(ModsOutcome::Set {
            id: "fps".into(),
            key: "shadow".into(),
            value: ModValue::Bool(true)
        })
    );
}

#[test]
fn a_slider_setting_reports_a_snapped_value() {
    let mut screen = on_settings_page();
    let scale = mods_layout(VIEWPORT).scale;
    let track = slider_track(row(1), scale);
    // The middle of the track is the middle of 1..=3.
    let outcome = show(&mut screen, click_at(track));
    assert_eq!(
        outcome,
        Some(ModsOutcome::Set {
            id: "fps".into(),
            key: "size".into(),
            value: ModValue::Number(2.0)
        })
    );
    // Far past the right end: clamped to the maximum.
    let past = crate::menus::common::testing::click_point([track.right() - 1.0, track.y + 3.0]);
    let outcome = show(&mut on_settings_page(), past);
    assert!(matches!(
        outcome,
        Some(ModsOutcome::Set { value: ModValue::Number(v), .. }) if (v - 3.0).abs() < 1e-4
    ));
}

#[test]
fn a_choice_setting_cycles_to_the_next_option() {
    let mut screen = on_settings_page();
    let scale = mods_layout(VIEWPORT).scale;
    let outcome = show(&mut screen, click_at(choice_button(row(2), scale)));
    assert_eq!(
        outcome,
        Some(ModsOutcome::Set {
            id: "fps".into(),
            key: "mode".into(),
            value: ModValue::Text("sum".into())
        })
    );
}

#[test]
fn a_color_setting_reports_the_clicked_preset() {
    let mut screen = on_settings_page();
    let scale = mods_layout(VIEWPORT).scale;
    let outcome = show(&mut screen, click_at(swatch_rect(row(3), scale, 2)));
    assert_eq!(
        outcome,
        Some(ModsOutcome::Set {
            id: "fps".into(),
            key: "color".into(),
            value: ModValue::Text("#ff5555".into())
        })
    );
}

#[test]
fn a_mod_without_settings_says_so() {
    let mut screen = on_list();
    let layout = mods_layout(VIEWPORT);
    let cols = columns(layout.list.width / layout.scale);
    let second = card_rect(layout.list, layout.scale, 1, (cols, 0.0));
    show(&mut screen, click_at(controls(second, layout.scale).1));
    let page = texts(&mut screen, &view());
    assert!(has_text(&page, "umicraft.mods.no_settings"), "{page:?}");
}

/// A view whose first mod has more settings than the page can show.
fn long_view() -> ModsView {
    let mut long = view();
    long.cards[0].settings = (0..12)
        .map(|n| setting(&format!("k{n}"), SettingControl::Toggle(false)))
        .collect();
    long
}

fn wheel_over(area: Rect, delta: f32) -> UiInput {
    UiInput::new(vec![
        InputEvent::PointerMoved {
            position: [area.x + 4.0, area.y + 4.0],
        },
        InputEvent::Wheel { delta },
    ])
}

/// The settings page area, as the page lays it out.
fn page_area() -> Rect {
    let layout = mods_layout(VIEWPORT);
    Rect {
        y: layout.tabs.y,
        ..layout.detail
    }
}

#[test]
fn the_wheel_scrolls_a_settings_page_taller_than_the_panel() {
    let long = long_view();
    let mut screen = on_list();
    let (_, gear) = controls(first_card(), mods_layout(VIEWPORT).scale);
    show_view(&mut screen, &long, click_at(gear));
    let area = page_area();
    let scale = mods_layout(VIEWPORT).scale;
    show_view(&mut screen, &long, wheel_over(area, 50.0));
    let max = settings_panel::content_height(12) - area.height / scale;
    assert!((screen.settings_scroll - max).abs() < 1e-3, "{max}");
    // The last row is now where the first one used to be reachable.
    let top = Rect {
        y: area.y - screen.settings_scroll * scale,
        ..area
    };
    let last = row_rect(top, scale, 11);
    let outcome = show_view(&mut screen, &long, click_at(toggle_track(last, scale)));
    assert_eq!(
        outcome,
        Some(ModsOutcome::Set {
            id: "fps".into(),
            key: "k11".into(),
            value: ModValue::Bool(true)
        })
    );
    show_view(&mut screen, &long, wheel_over(area, -50.0));
    assert_eq!(screen.settings_scroll, 0.0);
}

#[test]
fn a_short_settings_page_does_not_scroll() {
    let mut screen = on_settings_page();
    show(&mut screen, wheel_over(page_area(), 10.0));
    assert_eq!(screen.settings_scroll, 0.0);
}

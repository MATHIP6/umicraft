// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the options screens.

use umicraft_core::lang::Lang;
use umicraft_ui::{FontAtlas, InputEvent, PointerButton, Rect, Ui, UiInput};

use super::controls_page::sensitivity_label;
use super::main_page::fov_label;
use super::*;

const VIEWPORT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 1280.0,
    height: 720.0,
};

fn frame(screen: &mut OptionsScreen, options: &mut Options, events: Vec<InputEvent>) -> Outcome {
    let mut ui = Ui::new(VIEWPORT);
    ui.begin_frame(VIEWPORT, UiInput::new(events));
    let outcome = screen.show(&mut ui, &FontAtlas::fallback(), options, &Lang::builtin());
    ui.end_frame();
    outcome
}

fn click(rect: Rect) -> Vec<InputEvent> {
    let position = [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
    vec![
        InputEvent::PointerMoved { position },
        InputEvent::PointerPressed {
            position,
            button: PointerButton::Left,
        },
        InputEvent::PointerReleased {
            position,
            button: PointerButton::Left,
        },
    ]
}

/// Draws a frame to lay the widgets out, then clicks `id` in the next.
fn click_widget(screen: &mut OptionsScreen, options: &mut Options, id: &str) -> Outcome {
    frame(screen, options, vec![]);
    let rect = screen
        .rect_of(id)
        .unwrap_or_else(|| panic!("no widget {id}"));
    frame(screen, options, click(rect))
}

/// GUI scale 3 at 1280x720: the window is 427x240 GUI units.
fn gui_rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x * 3.0, y * 3.0, w * 3.0, h * 3.0)
}

#[test]
fn the_hub_page_matches_gui_options() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    frame(&mut screen, &mut options, vec![]);
    // width 427 -> cx 213; height 240 -> h/6 = 40.
    assert_eq!(
        screen.rect_of("fov"),
        Some(gui_rect(58.0, 28.0, 150.0, 20.0))
    );
    assert_eq!(
        screen.rect_of("difficulty"),
        Some(gui_rect(218.0, 28.0, 150.0, 20.0))
    );
    assert_eq!(
        screen.rect_of("nav:video"),
        Some(gui_rect(58.0, 130.0, 150.0, 20.0))
    );
    assert_eq!(
        screen.rect_of("nav:controls"),
        Some(gui_rect(218.0, 130.0, 150.0, 20.0))
    );
    assert_eq!(
        screen.rect_of("nav:chat"),
        Some(gui_rect(218.0, 154.0, 150.0, 20.0))
    );
    assert_eq!(
        screen.rect_of("done"),
        Some(gui_rect(113.0, 208.0, 200.0, 20.0))
    );
}

#[test]
fn done_on_the_hub_closes_and_done_on_a_page_goes_back() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "nav:chat");
    assert_eq!(screen.page(), Page::Chat);
    let outcome = click_widget(&mut screen, &mut options, "done");
    assert!(!outcome.done);
    assert_eq!(screen.page(), Page::Main);
    assert!(click_widget(&mut screen, &mut options, "done").done);
}

#[test]
fn a_disabled_hub_button_does_nothing() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "sounds");
    assert_eq!(screen.page(), Page::Main);
}

#[test]
fn the_fov_slider_edits_within_its_range_and_names_the_ends() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    frame(&mut screen, &mut options, vec![]);
    let fov = screen.rect_of("fov").unwrap();
    let events = vec![InputEvent::PointerPressed {
        position: [fov.x + fov.width, fov.y + 5.0],
        button: PointerButton::Left,
    }];
    let outcome = frame(&mut screen, &mut options, events);
    assert!(outcome.changed);
    assert_eq!(options.video.fov, 110.0);
}

#[test]
fn the_video_page_lists_the_sixteen_options_in_vanilla_order() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "nav:video");
    frame(&mut screen, &mut options, vec![]);
    // Row 0 starts at top + 4 = 36; Render Distance is the right column.
    assert_eq!(
        screen.rect_of("video:0"),
        Some(gui_rect(58.0, 36.0, 150.0, 20.0))
    );
    assert_eq!(
        screen.rect_of("distance"),
        Some(gui_rect(218.0, 36.0, 150.0, 20.0))
    );
    assert_eq!(
        screen.rect_of("done"),
        Some(gui_rect(113.0, 213.0, 200.0, 20.0))
    );
}

#[test]
fn the_wheel_scrolls_the_video_list() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "nav:video");
    frame(&mut screen, &mut options, vec![]);
    let before = screen.rect_of("video:0").unwrap();
    frame(
        &mut screen,
        &mut options,
        vec![InputEvent::Wheel { delta: 1.0 }],
    );
    let after = screen.rect_of("video:0").unwrap();
    assert!(after.y < before.y, "the row moved up");
}

#[test]
fn the_controls_page_rebinds_a_key_and_resets_it() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "nav:controls");
    assert_eq!(screen.page(), Page::Controls);
    // Scroll to the Movement keys, then click Jump.
    frame(
        &mut screen,
        &mut options,
        vec![InputEvent::Wheel { delta: 16.0 }],
    );
    click_widget(&mut screen, &mut options, "key:jump");
    assert_eq!(screen.capturing(), Some(Action::Jump));
    assert!(screen.capture_key(&mut options, KeyCode::KeyJ));
    assert_eq!(options.keys.get(Action::Jump), KeyCode::KeyJ);
    click_widget(&mut screen, &mut options, "keyreset:jump");
    assert_eq!(options.keys.get(Action::Jump), KeyCode::Space);
}

#[test]
fn reset_keys_is_disabled_until_a_key_changes() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "nav:controls");
    let outcome = click_widget(&mut screen, &mut options, "reset");
    assert!(!outcome.changed);
    options.keys.set(Action::Chat, KeyCode::KeyC);
    let outcome = click_widget(&mut screen, &mut options, "reset");
    assert!(outcome.changed);
    assert_eq!(options.keys.get(Action::Chat), KeyCode::KeyT);
}

#[test]
fn the_chat_page_cycles_visibility_and_toggles_colors() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "nav:chat");
    let before = options.chat.visibility;
    let outcome = click_widget(&mut screen, &mut options, "visibility");
    assert!(outcome.changed);
    assert_ne!(options.chat.visibility, before);
    let colors = options.chat.colors;
    click_widget(&mut screen, &mut options, "colors");
    assert_eq!(options.chat.colors, !colors);
    // Vanilla places the right column 160 units after the left.
    let gap = screen.rect_of("colors").unwrap().x - screen.rect_of("visibility").unwrap().x;
    assert_eq!(gap, 160.0 * 3.0);
}

#[test]
fn the_chat_sliders_edit_the_options_and_name_their_pixels() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "nav:chat");
    frame(&mut screen, &mut options, vec![]);
    let width = screen.rect_of("width").expect("a width slider");
    // The far left of the bar is the minimum: 40 pixels.
    let events = vec![InputEvent::PointerPressed {
        position: [width.x + 1.0, width.y + 5.0],
        button: PointerButton::Left,
    }];
    let outcome = frame(&mut screen, &mut options, events);
    assert!(outcome.changed);
    assert_eq!(options.chat.width, 0.0);
    let opacity = screen.rect_of("opacity").expect("an opacity slider");
    let events = vec![InputEvent::PointerPressed {
        position: [opacity.x + 1.0, opacity.y + 5.0],
        button: PointerButton::Left,
    }];
    frame(&mut screen, &mut options, events);
    assert_eq!(options.chat.opacity, 0.0);
    click_widget(&mut screen, &mut options, "links");
    assert!(!options.chat.links);
}

#[test]
fn labels_follow_game_settings() {
    let lang = Lang::from_texts(
        "xx_XX",
        None,
        Some(
            "options.sensitivity=Sens\noptions.sensitivity.min=low\n\
             options.sensitivity.max=high\noptions.fov=View\noptions.fov.min=std\n\
             options.fov.max=wide\n",
        ),
    );
    assert_eq!(sensitivity_label(&lang, 0.0), "Sens: low");
    assert_eq!(sensitivity_label(&lang, 1.0), "Sens: high");
    assert_eq!(sensitivity_label(&lang, 0.5), "Sens: 100%");
    assert_eq!(fov_label(&lang, 70.0), "View: std");
    assert_eq!(fov_label(&lang, 110.0), "View: wide");
    assert_eq!(fov_label(&lang, 90.0), "View: 90");
}

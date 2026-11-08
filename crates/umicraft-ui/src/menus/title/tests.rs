// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the title façade and the modern screen.

use super::*;
use crate::draw::{DrawCommand, FontFace, SpriteSheet};
use crate::geom::Rect;
use crate::input::{InputEvent, UiInput};
use crate::menus::common::testing::{click_at, frame, VIEWPORT};
use crate::menus::{BrandArt, MenuArt};

fn info(atlas: &FontAtlas, style: TitleStyle) -> TitleInfo<'_> {
    TitleInfo {
        version: "Umicraft test",
        panorama: None,
        partial_tick: 0.0,
        style,
        brand_font: atlas,
    }
}

fn show(screen: &mut TitleScreen, input: UiInput, style: TitleStyle) -> Option<TitleOutcome> {
    let atlas = FontAtlas::fallback();
    let info = info(&atlas, style);
    frame(input, |ui| {
        screen.show(ui, &atlas, &Lang::builtin(), &info).0
    })
}

fn commands(screen: &mut TitleScreen, art: MenuArt, style: TitleStyle) -> Vec<DrawCommand> {
    let atlas = FontAtlas::fallback();
    let info = info(&atlas, style);
    frame(UiInput::default(), |ui| {
        ui.set_menu_art(art);
        screen.show(ui, &atlas, &Lang::builtin(), &info);
        ui.draw().commands().to_vec()
    })
}

fn centre(rect: Rect) -> [f32; 2] {
    [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5]
}

/// A screen whose opening animation has run its course.
fn opened() -> TitleScreen {
    let mut screen = TitleScreen::new(0);
    for _ in 0..60 {
        screen.tick();
    }
    screen
}

#[test]
fn the_modern_screen_is_the_default_and_toggles_to_classic() {
    assert_eq!(TitleStyle::default(), TitleStyle::Modern);
    assert_eq!(TitleStyle::Modern.toggled(), TitleStyle::Classic);
    assert_eq!(TitleStyle::Classic.toggled(), TitleStyle::Modern);
}

#[test]
fn each_modern_button_reports_its_outcome() {
    let layout = modern_title_layout(VIEWPORT);
    let mut screen = opened();
    for (rect, expected) in [
        (layout.play, TitleOutcome::Multiplayer),
        (layout.options, TitleOutcome::Options),
    ] {
        let outcome = show(&mut screen, click_at(rect), TitleStyle::Modern);
        assert_eq!(outcome, Some(expected));
    }
}

#[test]
fn the_modern_screen_has_two_buttons_and_a_small_quit_cross() {
    let layout = modern_title_layout(VIEWPORT);
    let mut screen = opened();
    // The cross is far smaller than a main button.
    assert!(layout.quit.width < layout.play.height);
    // The right edge below the cross answers nothing.
    let corner = Rect::new(VIEWPORT.width - 80.0, 200.0, 80.0, 40.0);
    assert_eq!(
        show(&mut screen, click_at(corner), TitleStyle::Modern),
        None
    );
    let drawn = commands(&mut screen, MenuArt::default(), TitleStyle::Modern);
    let round_buttons = drawn
        .iter()
        .filter(|command| matches!(command, DrawCommand::RoundedRect { .. }))
        .count();
    assert_eq!(round_buttons, 3, "Play, Options and Quit");
}

#[test]
fn the_style_selects_the_screen_that_answers_the_click() {
    let classic = title_layout(VIEWPORT);
    let mut screen = TitleScreen::new(0);
    let click = click_at(classic.multiplayer);
    assert_eq!(
        show(&mut screen, click, TitleStyle::Classic),
        Some(TitleOutcome::Multiplayer)
    );
    // The classic Language button lies outside the modern buttons.
    let language = click_at(classic.language);
    assert_eq!(show(&mut screen, language, TitleStyle::Modern), None);
}

#[test]
fn the_modern_screen_draws_the_wordmark_in_the_brand_face() {
    let mut screen = opened();
    let drawn = commands(&mut screen, MenuArt::default(), TitleStyle::Modern);
    let letters = wordmark_letters(&drawn);
    assert!(letters.contains(WORDMARK), "{letters}");
    let classic_logo = drawn
        .iter()
        .any(|command| matches!(command, DrawCommand::Text { text, .. } if text == LOGO_TEXT));
    assert!(!classic_logo);
}

#[test]
fn the_logo_is_fitted_into_its_box_when_the_sprite_exists() {
    let logo = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 359, 354);
    let art = MenuArt {
        brand: BrandArt {
            logo: Some(logo),
            ..BrandArt::default()
        },
        ..MenuArt::default()
    };
    let mut screen = opened();
    let area = modern_title_layout(VIEWPORT).logo;
    let images: Vec<_> = commands(&mut screen, art, TitleStyle::Modern)
        .into_iter()
        .filter_map(|command| match command {
            DrawCommand::Image { src, dst, .. } if src == logo.sub(0, 0, 359, 354) => Some(dst),
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 1);
    assert!((images[0].width - area.width).abs() < 1e-3);
    assert!(images[0].height <= area.height);
}

/// The black sheet of the opening animation: a viewport-sized `Rect` whose
/// colour is pure black (the vignette has a blue tint).
fn black_sheet_alpha(drawn: &[DrawCommand]) -> Option<f32> {
    drawn.iter().find_map(|command| match command {
        DrawCommand::Rect { rect, color } if *rect == VIEWPORT && color[..3] == [0.0, 0.0, 0.0] => {
            Some(color[3])
        }
        _ => None,
    })
}

/// The single-letter brand texts of a frame: the letters of the wordmark.
fn wordmark_letters(drawn: &[DrawCommand]) -> String {
    drawn
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Text { text, style, .. } if style.face == FontFace::Brand => {
                Some(text.as_str())
            }
            _ => None,
        })
        .filter(|text| text.chars().count() == 1 && WORDMARK.contains(*text))
        .collect()
}

#[test]
fn the_modern_screen_opens_from_black_and_then_stays_clear() {
    let mut screen = TitleScreen::new(0);
    let first = commands(&mut screen, MenuArt::default(), TitleStyle::Modern);
    assert_eq!(black_sheet_alpha(&first), Some(1.0), "starts fully black");
    let mut previous = 1.0;
    let mut fading_frames = 0;
    for _ in 0..60 {
        screen.tick();
        let drawn = commands(&mut screen, MenuArt::default(), TitleStyle::Modern);
        let Some(alpha) = black_sheet_alpha(&drawn) else {
            break;
        };
        assert!(alpha < previous, "{alpha} < {previous}");
        previous = alpha;
        fading_frames += 1;
    }
    assert!(
        fading_frames >= 10,
        "the dark lifts over {fading_frames} frames"
    );
    let done = commands(&mut screen, MenuArt::default(), TitleStyle::Modern);
    assert_eq!(black_sheet_alpha(&done), None, "the sheet is gone");
}

#[test]
fn the_opening_starts_when_the_window_gets_the_focus() {
    let mut screen = TitleScreen::new(0);
    screen.set_focused(false);
    for _ in 0..100 {
        screen.tick();
    }
    let waiting = commands(&mut screen, MenuArt::default(), TitleStyle::Modern);
    assert_eq!(
        black_sheet_alpha(&waiting),
        Some(1.0),
        "nothing played unseen"
    );
    screen.set_focused(true);
    screen.tick();
    let started = commands(&mut screen, MenuArt::default(), TitleStyle::Modern);
    let alpha = black_sheet_alpha(&started).expect("still opening");
    assert!(alpha < 1.0, "it started with the focus: {alpha}");
    for _ in 0..60 {
        screen.tick();
    }
    let played = commands(&mut screen, MenuArt::default(), TitleStyle::Modern);
    assert_eq!(black_sheet_alpha(&played), None, "it plays out");
}

#[test]
fn the_opening_pauses_when_the_focus_is_lost_and_goes_on_when_it_comes_back() {
    let mut screen = TitleScreen::new(0);
    for _ in 0..10 {
        screen.tick();
    }
    let before = black_sheet_alpha(&commands(
        &mut screen,
        MenuArt::default(),
        TitleStyle::Modern,
    ));
    screen.set_focused(false);
    for _ in 0..50 {
        screen.tick();
    }
    let paused = black_sheet_alpha(&commands(
        &mut screen,
        MenuArt::default(),
        TitleStyle::Modern,
    ));
    assert_eq!(paused, before, "it stood still without the focus");
    screen.set_focused(true);
    for _ in 0..60 {
        screen.tick();
    }
    let done = commands(&mut screen, MenuArt::default(), TitleStyle::Modern);
    assert_eq!(black_sheet_alpha(&done), None);
}

#[test]
fn a_screen_never_told_about_the_focus_opens_normally() {
    // The headless run has no window to ask: it is taken to be focused.
    let mut screen = TitleScreen::new(0);
    for _ in 0..60 {
        screen.tick();
    }
    let drawn = commands(&mut screen, MenuArt::default(), TitleStyle::Modern);
    assert_eq!(black_sheet_alpha(&drawn), None);
}

#[test]
fn the_letters_of_the_wordmark_come_in_one_by_one() {
    let mut screen = TitleScreen::new(0);
    let first = commands(&mut screen, MenuArt::default(), TitleStyle::Modern);
    assert!(wordmark_letters(&first).is_empty(), "none at the start");
    for _ in 0..16 {
        screen.tick();
    }
    let part = wordmark_letters(&commands(
        &mut screen,
        MenuArt::default(),
        TitleStyle::Modern,
    ));
    assert!(
        (1..WORDMARK.len()).contains(&part.chars().count()),
        "some letters, not all: {part:?}"
    );
    for _ in 0..44 {
        screen.tick();
    }
    let all = wordmark_letters(&commands(
        &mut screen,
        MenuArt::default(),
        TitleStyle::Modern,
    ));
    assert!(all.contains(WORDMARK), "{all}");
}

#[test]
fn hovering_a_modern_button_fades_it_in_over_ticks() {
    let layout = modern_title_layout(VIEWPORT);
    let mut screen = TitleScreen::new(0);
    let hover = UiInput::new(vec![InputEvent::PointerMoved {
        position: centre(layout.play),
    }]);
    show(&mut screen, hover, TitleStyle::Modern);
    let before = screen.modern.fade_level(0);
    screen.tick();
    assert!(screen.modern.fade_level(0) > before);
}

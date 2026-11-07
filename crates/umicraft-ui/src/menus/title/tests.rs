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

#[test]
fn the_modern_screen_is_the_default_and_toggles_to_classic() {
    assert_eq!(TitleStyle::default(), TitleStyle::Modern);
    assert_eq!(TitleStyle::Modern.toggled(), TitleStyle::Classic);
    assert_eq!(TitleStyle::Classic.toggled(), TitleStyle::Modern);
}

#[test]
fn each_modern_button_reports_its_outcome() {
    let layout = modern_title_layout(VIEWPORT);
    let mut screen = TitleScreen::new(0);
    for (rect, expected) in [
        (layout.play, TitleOutcome::Multiplayer),
        (layout.options, TitleOutcome::Options),
    ] {
        let outcome = show(&mut screen, click_at(rect), TitleStyle::Modern);
        assert_eq!(outcome, Some(expected));
    }
}

#[test]
fn the_modern_screen_has_no_quit_button() {
    // Regression: the power icon in the top-right corner was removed.
    let mut screen = TitleScreen::new(0);
    let corner = Rect::new(VIEWPORT.width - 80.0, 0.0, 80.0, 80.0);
    assert_eq!(
        show(&mut screen, click_at(corner), TitleStyle::Modern),
        None
    );
    let drawn = commands(&mut screen, MenuArt::default(), TitleStyle::Modern);
    let round_buttons = drawn
        .iter()
        .filter(|command| matches!(command, DrawCommand::RoundedRect { .. }))
        .count();
    assert_eq!(round_buttons, 2, "only Play and Options");
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
    // The classic Multiplayer button is not a modern button.
    let modern = modern_title_layout(VIEWPORT);
    assert!(!modern.play.contains(centre(classic.multiplayer)));
}

#[test]
fn the_modern_screen_draws_the_wordmark_in_the_brand_face() {
    let mut screen = TitleScreen::new(0);
    let drawn = commands(&mut screen, MenuArt::default(), TitleStyle::Modern);
    let letters: String = drawn
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Text { text, style, .. } if style.face == FontFace::Brand => {
                Some(text.as_str())
            }
            _ => None,
        })
        .filter(|text| text.chars().count() == 1)
        .collect();
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
    let mut screen = TitleScreen::new(0);
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

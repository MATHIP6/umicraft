// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the mods menu.

use super::*;
use crate::draw::{DrawCommand, FontFace, SpriteSheet};
use crate::geom::Rect;
use crate::menus::common::testing::{click_at, frame, VIEWPORT};
use crate::menus::title::WORDMARK;
use crate::menus::{BrandArt, MenuArt};

fn show(screen: &mut ModsScreen, input: crate::input::UiInput) -> Option<ModsOutcome> {
    let atlas = FontAtlas::fallback();
    frame(input, |ui| screen.show(ui, &atlas, &Lang::builtin(), 0.0))
}

#[test]
fn the_group_is_centred_on_the_screen() {
    let layout = mods_layout(VIEWPORT);
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
    let layout = mods_layout(VIEWPORT);
    let bottom = |rect: Rect| rect.y + rect.height;
    assert!(bottom(layout.logo) <= layout.wordmark.y);
    assert!(bottom(layout.wordmark) <= layout.mods.y);
    assert!(bottom(layout.mods) <= layout.hint[1]);
}

#[test]
fn the_menu_is_small() {
    // The group is much smaller than the screen and its button is smaller
    // than a title-screen button (160 x 22 units).
    let layout = mods_layout(VIEWPORT);
    let group = layout.hint[1] - layout.logo.y;
    assert!(group < VIEWPORT.height * 0.5, "{group}");
    assert!(layout.mods.width < 160.0 * Gui::new(VIEWPORT).scale);
}

#[test]
fn the_cross_is_in_the_top_right_corner_of_the_screen() {
    let layout = mods_layout(VIEWPORT);
    assert!(layout.close.x > VIEWPORT.x + VIEWPORT.width * 0.5);
    assert!(layout.close.y < VIEWPORT.y + VIEWPORT.height * 0.2);
}

#[test]
fn the_two_buttons_report_their_outcome() {
    let layout = mods_layout(VIEWPORT);
    let mut screen = ModsScreen::new();
    assert_eq!(
        show(&mut screen, click_at(layout.mods)),
        Some(ModsOutcome::OpenMods)
    );
    assert_eq!(
        show(&mut screen, click_at(layout.close)),
        Some(ModsOutcome::Close)
    );
}

#[test]
fn a_click_beside_the_buttons_does_nothing() {
    let layout = mods_layout(VIEWPORT);
    let mut screen = ModsScreen::new();
    let away = Rect::new(VIEWPORT.x + 10.0, VIEWPORT.y + 10.0, 8.0, 8.0);
    assert_eq!(show(&mut screen, click_at(away)), None);
    let _ = layout;
}

/// A menu whose opening animation is over.
fn opened() -> ModsScreen {
    let mut screen = ModsScreen::new();
    for _ in 0..30 {
        screen.tick();
    }
    screen
}

fn drawn_by(mut screen: ModsScreen, art: MenuArt) -> Vec<DrawCommand> {
    let atlas = FontAtlas::fallback();
    frame(crate::input::UiInput::default(), |ui| {
        ui.set_menu_art(art);
        screen.show(ui, &atlas, &Lang::builtin(), 0.0);
        ui.draw().commands().to_vec()
    })
}

fn drawn(art: MenuArt) -> Vec<DrawCommand> {
    drawn_by(opened(), art)
}

/// The alpha of the dark sheet: the viewport-sized black `Rect`.
fn sheet_alpha(commands: &[DrawCommand]) -> Option<f32> {
    commands.iter().find_map(|command| match command {
        DrawCommand::Rect { rect, color } if *rect == VIEWPORT && color[..3] == [0.0; 3] => {
            Some(color[3])
        }
        _ => None,
    })
}

/// The letters of the name: single brand glyphs above the Mods button (the
/// label of the button is spelled in glyphs too).
fn name_letters(commands: &[DrawCommand]) -> usize {
    let button_top = mods_layout(VIEWPORT).mods.y;
    commands
        .iter()
        .filter(|command| {
            matches!(command, DrawCommand::Text { position, text, style }
                if style.face == FontFace::Brand
                    && position[1] < button_top
                    && text.chars().count() == 1
                    && WORDMARK.contains(text.as_str()))
        })
        .count()
}

#[test]
fn only_the_sheet_the_logo_and_the_blur_animate_the_text_is_always_there() {
    // The text is complete from the first frame, the sheet is not.
    let first = drawn_by(ModsScreen::new(), MenuArt::default());
    assert_eq!(sheet_alpha(&first), Some(0.0), "the sheet starts clear");
    assert_eq!(name_letters(&first), WORDMARK.len(), "the name is there");
    let done = drawn(MenuArt::default());
    assert_eq!(sheet_alpha(&done), Some(BACKDROP[3]));
    assert_eq!(name_letters(&done), WORDMARK.len());

    // In between the sheet only darkens; the name never changes.
    let mut screen = ModsScreen::new();
    let mut sheet = 0.0;
    for _ in 0..OPEN_TICKS {
        screen.tick();
        let frame = drawn_by(screen.clone(), MenuArt::default());
        let now = sheet_alpha(&frame).unwrap();
        assert!(now >= sheet, "{now} >= {sheet}");
        assert_eq!(name_letters(&frame), WORDMARK.len());
        sheet = now;
    }
    assert_eq!(sheet, BACKDROP[3]);
}

#[test]
fn the_buttons_and_the_hint_do_not_fade_or_move() {
    // The same frame at the first tick and at the end, once the sheet and
    // the logo (absent here) are left out.
    let strip = |commands: Vec<DrawCommand>| -> Vec<DrawCommand> {
        commands
            .into_iter()
            .filter(
                |command| !matches!(command, DrawCommand::Rect { rect, .. } if *rect == VIEWPORT),
            )
            .collect()
    };
    let first = strip(drawn_by(ModsScreen::new(), MenuArt::default()));
    let done = strip(drawn(MenuArt::default()));
    assert_eq!(first, done);
}

#[test]
fn the_blur_fades_in_over_the_opening() {
    let mut screen = ModsScreen::new();
    assert_eq!(screen.blur_strength(0.0), 0.0, "sharp at the first frame");
    let mut previous = 0.0;
    for _ in 0..OPEN_TICKS {
        screen.tick();
        let now = screen.blur_strength(0.0);
        assert!(now >= previous, "{now} >= {previous}");
        previous = now;
    }
    assert_eq!(previous, 1.0, "fully blurred once open");
    assert_eq!(screen.blur_strength(0.7), 1.0, "and it stays so");
}

#[test]
fn the_logo_grows_into_its_box_while_it_opens() {
    let logo = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 359, 354);
    let art = MenuArt {
        brand: BrandArt {
            logo: Some(logo),
            ..BrandArt::default()
        },
        ..MenuArt::default()
    };
    let width = |commands: Vec<DrawCommand>| {
        commands.into_iter().find_map(|command| match command {
            DrawCommand::Image { src, dst, .. } if src == logo.sub(0, 0, 359, 354) => {
                Some(dst.width)
            }
            _ => None,
        })
    };
    let start = width(drawn_by(ModsScreen::new(), art)).expect("logo drawn");
    let end = width(drawn(art)).expect("logo drawn");
    assert!(start < end, "{start} < {end}");
    assert!((start / end - LOGO_START_SCALE).abs() < 1e-2);
}

#[test]
fn nothing_but_a_light_sheet_is_drawn_behind_the_buttons() {
    // No panel: no rounded shape is wider than the Mods button.
    let widest = drawn(MenuArt::default())
        .iter()
        .filter_map(|command| match command {
            DrawCommand::RoundedRect { rect, .. } => Some(rect.width),
            _ => None,
        })
        .fold(0.0_f32, f32::max);
    assert!(
        widest <= mods_layout(VIEWPORT).mods.width + 1e-3,
        "{widest}"
    );
}

#[test]
fn the_mods_button_is_a_card_with_a_rim_an_icon_and_tracked_capitals() {
    let layout = mods_layout(VIEWPORT);
    let commands = drawn(MenuArt::default());
    let inside = |rect: &Rect| {
        rect.x >= layout.mods.x - 1e-3
            && rect.x + rect.width <= layout.mods.x + layout.mods.width + 1e-3
            && rect.y >= layout.mods.y - 1e-3
            && rect.y + rect.height <= layout.mods.y + layout.mods.height + 1e-3
    };
    let shapes = commands
        .iter()
        .filter(|command| matches!(command, DrawCommand::RoundedRect { rect, .. } if inside(rect)))
        .count();
    // The rim, the fill and the four squares of the icon.
    assert_eq!(shapes, 6);
    // The label is spelled in capitals, one glyph at a time.
    let letters: String = commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Text { text, .. } if text.chars().count() == 1 => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(letters.contains("MODS"), "{letters}");
}

#[test]
fn the_rim_lights_up_under_the_pointer() {
    let layout = mods_layout(VIEWPORT);
    let rim_of = |fade: f32| {
        let atlas = FontAtlas::fallback();
        frame(crate::input::UiInput::default(), |ui| {
            let id = ui.id("probe");
            mods_button(ui, &atlas, (id, layout.mods), (1.0, fade), "Mods");
            ui.draw()
                .commands()
                .iter()
                .find_map(|command| match command {
                    DrawCommand::RoundedRect { rect, color, .. } if *rect == layout.mods => {
                        Some(*color)
                    }
                    _ => None,
                })
        })
        .expect("the rim is the card-sized shape")
    };
    let (rest, hover) = (rim_of(0.0), rim_of(1.0));
    assert!(hover[3] > rest[3], "{hover:?} over {rest:?}");
    assert!(hover[2] > hover[0], "the hovered rim is blue");
}

#[test]
fn the_name_is_written_in_the_brand_face_above_the_button() {
    let commands = drawn(MenuArt::default());
    let letters: String = commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Text { text, style, .. } if style.face == FontFace::Brand => {
                Some(text.as_str())
            }
            _ => None,
        })
        .filter(|text| text.chars().count() == 1 && WORDMARK.contains(*text))
        .collect();
    assert!(letters.contains(WORDMARK), "{letters}");
}

#[test]
fn the_logo_is_drawn_inside_its_box_when_the_sprite_exists() {
    let logo = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 359, 354);
    let art = MenuArt {
        brand: BrandArt {
            logo: Some(logo),
            ..BrandArt::default()
        },
        ..MenuArt::default()
    };
    let area = mods_layout(VIEWPORT).logo;
    let images: Vec<Rect> = drawn(art)
        .into_iter()
        .filter_map(|command| match command {
            DrawCommand::Image { src, dst, .. } if src == logo.sub(0, 0, 359, 354) => Some(dst),
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 1);
    assert!(images[0].height <= area.height + 1e-3);
    assert!(images[0].y >= area.y - 1e-3);
}

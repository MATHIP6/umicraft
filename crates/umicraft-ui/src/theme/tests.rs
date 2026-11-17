// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;
use crate::draw::{Color, DrawCommand};
use crate::geom::Rect;
use crate::input::UiInput;
use crate::menus::draw_menu_background;
use crate::text::FontAtlas;
use crate::widgets::{button, slider, SliderState};
use crate::Ui;

const VIEW: Rect = Rect::new(0.0, 0.0, 1280.0, 720.0);
const BUTTON: Rect = Rect::new(100.0, 100.0, 300.0, 60.0);

fn commands(theme: Theme, f: impl FnOnce(&mut Ui)) -> Vec<DrawCommand> {
    let mut ui = Ui::new(VIEW);
    ui.set_theme(theme);
    ui.begin_frame(VIEW, UiInput::default());
    f(&mut ui);
    ui.draw().commands().to_vec()
}

fn rects(commands: &[DrawCommand]) -> Vec<(Rect, Color)> {
    commands
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Rect { rect, color } => Some((*rect, *color)),
            _ => None,
        })
        .collect()
}

#[test]
fn rgb_unpacks_the_channels() {
    assert_eq!(rgb(0xFF0000), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(rgb(0x000000)[3], 1.0);
}

#[test]
fn the_default_theme_is_vanilla() {
    assert_eq!(Theme::default(), ThemeId::Vanilla.theme());
    assert_eq!(Ui::new(VIEW).theme(), Theme::VANILLA);
}

#[test]
fn every_theme_has_a_distinct_lang_key() {
    let keys: Vec<_> = ThemeId::ALL.iter().map(|id| id.lang_key()).collect();
    assert_eq!(keys.len(), 3);
    assert!(keys.iter().all(|key| key.starts_with("umicraft.theme.")));
    assert_ne!(keys[1], keys[2]);
}

#[test]
fn a_vanilla_button_without_textures_keeps_the_grey_fallback() {
    let drawn = commands(Theme::VANILLA, |ui| {
        let id = ui.id("b");
        button(ui, id, BUTTON, &FontAtlas::fallback(), "Done");
    });
    assert_eq!(rects(&drawn).len(), 1);
}

#[test]
fn an_amethyst_button_has_border_fill_and_bevel() {
    let drawn = commands(ThemeId::Amethyst.theme(), |ui| {
        let id = ui.id("b");
        button(ui, id, BUTTON, &FontAtlas::fallback(), "Done");
    });
    let painted = rects(&drawn);
    assert_eq!(painted.len(), 3);
    assert_eq!(painted[0], (BUTTON, palette::AMETHYST.border.unwrap()));
    // 60 px tall: texel 3, so a 3 px border and a 6 px bevel.
    assert_eq!(painted[1].0, BUTTON.inset(3.0));
    assert_eq!(painted[1].1, palette::AMETHYST.fill);
    assert_eq!(painted[2].0.height, 6.0);
    assert_eq!(painted[2].1, palette::AMETHYST.bevel.unwrap());
}

#[test]
fn a_disabled_flat_button_loses_its_bevel() {
    let drawn = commands(ThemeId::Amethyst.theme(), |ui| {
        let skin = palette::AMETHYST;
        let atlas = FontAtlas::fallback();
        skin.draw_button(ui, BUTTON, (&atlas, "x"), ControlState::Disabled, None);
    });
    let painted = rects(&drawn);
    assert_eq!(painted.len(), 2);
    assert_eq!(painted[1].1, palette::AMETHYST.disabled);
}

#[test]
fn an_ember_slider_draws_its_handle_in_the_accent() {
    let drawn = commands(ThemeId::Ember.theme(), |ui| {
        let id = ui.id("s");
        let mut state = SliderState::default();
        slider(
            ui,
            id,
            BUTTON,
            &FontAtlas::fallback(),
            "FOV",
            1.0,
            &mut state,
        );
    });
    let handle = rects(&drawn).into_iter().last().unwrap();
    assert_eq!(handle.1, palette::EMBER.accent);
    // The handle sits flush with the right edge at value 1.0.
    assert_eq!(handle.0.right(), BUTTON.right());
    assert_eq!(handle.0.width, 18.0);
}

#[test]
fn a_gradient_background_covers_the_viewport() {
    let drawn = commands(ThemeId::Ember.theme(), draw_menu_background);
    assert_eq!(
        drawn,
        [DrawCommand::VerticalGradient {
            rect: VIEW,
            top: palette::EMBER_BACKDROP_TOP,
            bottom: palette::EMBER_BACKDROP_BOTTOM,
        }]
    );
}

#[test]
fn a_gradient_slice_matches_the_full_gradient() {
    let top = rgb(0x000000);
    let bottom = rgb(0xFFFFFF);
    let lower_half = Rect::new(0.0, 360.0, 1280.0, 360.0);
    let (from, to) = background::gradient_slice(VIEW, lower_half, (top, bottom));
    assert!((from[0] - 0.5).abs() < 1e-6);
    assert_eq!(to, bottom);
}

#[test]
fn only_the_vanilla_theme_keeps_the_game_font() {
    assert_eq!(ThemeId::Vanilla.theme().font, FontSkin::Game);
    assert_eq!(ThemeId::Ember.theme().font, FontSkin::Hd);
    assert_eq!(ThemeId::Amethyst.theme().font, FontSkin::Hd);
}

#[test]
fn the_hd_cell_follows_the_gui_scale() {
    let cells: Vec<u32> = (1..=6).map(hd_cell_for_scale).collect();
    assert_eq!(cells, [16, 16, 24, 32, 32, 32]);
}

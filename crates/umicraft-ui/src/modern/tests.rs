// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the modern widgets.

use super::*;
use crate::draw::{DrawCommand, FontFace, SpriteSheet};
use crate::geom::Rect;
use crate::input::{InputEvent, PointerButton, UiInput};
use crate::text::FontAtlas;
use crate::Ui;

const VIEWPORT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 1280.0,
    height: 720.0,
};
const BUTTON: Rect = Rect {
    x: 100.0,
    y: 100.0,
    width: 300.0,
    height: 48.0,
};

fn click_in(rect: Rect) -> UiInput {
    let position = [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
    UiInput::new(vec![
        InputEvent::PointerMoved { position },
        InputEvent::PointerPressed {
            position,
            button: PointerButton::Left,
        },
    ])
}

/// Runs one frame and returns what `f` returned plus the draw commands.
fn frame<R>(input: UiInput, f: impl FnOnce(&mut Ui) -> R) -> (R, Vec<DrawCommand>) {
    let mut ui = Ui::new(VIEWPORT);
    ui.begin_frame(VIEWPORT, input);
    let result = f(&mut ui);
    let output = ui.end_frame();
    (result, output.draw_list.commands().to_vec())
}

fn sheet() -> SpriteSheet {
    SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 64, 64)
}

fn labelled(ui: &mut Ui, enabled: bool, icon: Option<SpriteSheet>) -> crate::UiResponse {
    let id = ui.id("play");
    ModernButton::new(id, BUTTON, "Play")
        .with_icon(icon)
        .enabled(enabled)
        .show(ui, &FontAtlas::fallback(), 3.0, 0.0)
}

#[test]
fn click_is_reported() {
    let (response, _) = frame(click_in(BUTTON), |ui| labelled(ui, true, None));
    assert!(response.clicked && response.hovered);
}

#[test]
fn disabled_button_ignores_pointer() {
    let (response, _) = frame(click_in(BUTTON), |ui| labelled(ui, false, None));
    assert!(!response.clicked && !response.hovered);
}

#[test]
fn button_draws_rounded_surface_and_brand_text() {
    let (_, commands) = frame(UiInput::default(), |ui| labelled(ui, true, None));
    assert!(matches!(commands[0], DrawCommand::RoundedRect { .. }));
    let brand_text = commands.iter().any(|command| {
        matches!(command, DrawCommand::Text { text, style, .. }
            if text == "Play" && style.face == FontFace::Brand)
    });
    assert!(brand_text);
}

#[test]
fn icon_is_drawn_only_when_present() {
    let images = |icon| {
        let (_, commands) = frame(UiInput::default(), |ui| labelled(ui, true, icon));
        commands
            .iter()
            .filter(|c| matches!(c, DrawCommand::Image { .. }))
            .count()
    };
    assert_eq!(images(Some(sheet())), 1);
    assert_eq!(images(None), 0);
}

#[test]
fn icon_button_clicks_and_respects_shape() {
    let rect = Rect::new(10.0, 10.0, 40.0, 40.0);
    let (response, commands) = frame(click_in(rect), |ui| {
        let id = ui.id("power");
        IconButton::new(id, rect, Some(sheet()))
            .round()
            .show(ui, 3.0, 0.0)
    });
    assert!(response.clicked);
    assert!(matches!(
        commands[0],
        DrawCommand::RoundedRect { radius, .. } if radius == 20.0
    ));
    assert!(matches!(commands[1], DrawCommand::Image { .. }));
}

#[test]
fn hover_fade_rises_and_falls_back() {
    let mut fade = HoverFade::default();
    assert_eq!(fade.level(1.0), 0.0);
    for _ in 0..4 {
        fade.tick(true);
    }
    assert_eq!(fade.level(1.0), 1.0);
    fade.tick(true);
    assert_eq!(fade.level(0.5), 1.0, "saturates");
    for _ in 0..4 {
        fade.tick(false);
    }
    assert_eq!(fade.level(1.0), 0.0);
}

#[test]
fn hover_fade_interpolates_between_ticks() {
    let mut fade = HoverFade::default();
    fade.tick(true);
    assert_eq!(fade.level(0.0), 0.0);
    assert!((fade.level(0.5) - theme::HOVER_FADE_STEP * 0.5).abs() < 1e-6);
    assert!((fade.level(1.0) - theme::HOVER_FADE_STEP).abs() < 1e-6);
}

#[test]
fn hover_blends_surface_colour() {
    let colour = |level| {
        let (_, commands) = frame(UiInput::default(), |ui| {
            let id = ui.id("b");
            ModernButton::new(id, BUTTON, "x").show(ui, &FontAtlas::fallback(), 3.0, level)
        });
        match commands[0] {
            DrawCommand::RoundedRect { color, .. } => color,
            _ => unreachable!("the surface is drawn first"),
        }
    };
    assert_eq!(colour(0.0), theme::SURFACE);
    assert_eq!(colour(1.0), theme::SURFACE_HOVER);
}

#[test]
fn vignette_covers_the_viewport() {
    let (_, commands) = frame(UiInput::default(), |ui| vignette(ui, VIEWPORT));
    assert!(matches!(
        commands[0],
        DrawCommand::Rect { rect, .. } if rect == VIEWPORT
    ));
    let (mut top, mut bottom) = (f32::MAX, f32::MIN);
    for command in &commands {
        if let DrawCommand::Rect { rect, .. } = command {
            top = top.min(rect.y);
            bottom = bottom.max(rect.y + rect.height);
        }
    }
    assert_eq!((top, bottom), (0.0, 720.0));
}

#[test]
fn panel_is_a_rounded_rect() {
    let (_, commands) = frame(UiInput::default(), |ui| panel(ui, BUTTON, 3.0));
    assert!(matches!(
        commands[0],
        DrawCommand::RoundedRect { radius, color, .. }
            if radius == theme::RADIUS * 3.0 && color == theme::SURFACE
    ));
}

#[test]
fn brand_text_is_scaled_from_pixels() {
    let style = brand_text_style(24.0, theme::TEXT);
    assert_eq!(style.face, FontFace::Brand);
    assert_eq!(style.scale, 24.0 / theme::BRAND_PIXEL_SIZE);
}

#[test]
fn centered_text_straddles_its_anchor() {
    let atlas = FontAtlas::fallback();
    let (_, commands) = frame(UiInput::default(), |ui| {
        draw_text(
            ui,
            &atlas,
            "Umicraft",
            (200.0, 50.0),
            (24.0, theme::TEXT, Align::Center),
        );
    });
    let width = measure_text(&atlas, "Umicraft", 24.0);
    assert!(matches!(
        commands[0],
        DrawCommand::Text { position, .. } if (position[0] - (200.0 - width * 0.5)).abs() < 1e-3
    ));
}

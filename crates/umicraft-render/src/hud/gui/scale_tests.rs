// SPDX-License-Identifier: MIT OR Apache-2.0

//! The HUD at the four GUI scales (MCP `ScaledResolution`): every sprite
//! stays on the screen and the hotbar and crosshair stay centred.
//!
//! The scale follows the auto rule (the largest factor that keeps 320x240 GUI
//! units); the video option that caps it does not exist yet, so the four
//! factors are reached by four window sizes of 16:9 and 4:3.

use umicraft_ui::screens::ScaledResolution;
use umicraft_ui::{DrawCommand, DrawList, FontAtlas, Rect, SpriteSheet};

use super::{draw_hud_gui, HudSheets};
use crate::hud::HudState;

/// A window size and the GUI scale the auto rule gives it.
const CASES: [((u32, u32), u32); 4] = [
    ((500, 400), 1),
    ((800, 600), 2),
    ((1280, 720), 3),
    ((1920, 1080), 4),
];

fn hud(viewport: (u32, u32)) -> DrawList {
    let sheet = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 256, 256);
    let sheets = HudSheets {
        widgets: Some(sheet),
        icons: Some(sheet),
    };
    let mut list = DrawList::new();
    draw_hud_gui(
        &mut list,
        &HudState::default(),
        viewport,
        &FontAtlas::fallback(),
        &sheets,
    );
    list
}

fn rects(list: &DrawList) -> Vec<Rect> {
    list.commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Image { dst, .. } | DrawCommand::InvertedImage { dst, .. } => Some(*dst),
            DrawCommand::Rect { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect()
}

#[test]
fn the_cases_reach_the_four_scale_factors() {
    for ((width, height), scale) in CASES {
        assert_eq!(ScaledResolution::new(width, height, 0).scale_factor, scale);
    }
}

#[test]
fn every_sprite_stays_inside_the_screen() {
    for ((width, height), scale) in CASES {
        let list = hud((width, height));
        let all = rects(&list);
        assert!(!all.is_empty());
        for rect in all {
            assert!(
                rect.x >= 0.0
                    && rect.y >= 0.0
                    && rect.x + rect.width <= width as f32 + scale as f32
                    && rect.y + rect.height <= height as f32,
                "scale {scale}: {rect:?} leaves {width}x{height}"
            );
        }
    }
}

#[test]
fn the_hotbar_and_the_crosshair_are_centred() {
    for ((width, height), scale) in CASES {
        let s = scale as f32;
        let all = rects(&hud((width, height)));
        let hotbar = all
            .iter()
            .find(|rect| {
                (rect.width - 182.0 * s).abs() < 0.5 && (rect.height - 22.0 * s).abs() < 0.5
            })
            .unwrap_or_else(|| panic!("scale {scale}: no hotbar background"));
        let middle = width as f32 / 2.0;
        assert!(
            (hotbar.x + hotbar.width / 2.0 - middle).abs() <= s,
            "scale {scale}: hotbar at {hotbar:?}"
        );
        assert!(
            (hotbar.y + hotbar.height - height as f32).abs() < 0.5,
            "scale {scale}: the hotbar rests on the bottom edge"
        );
        let cross = all
            .iter()
            .find(|rect| {
                (rect.width - 16.0 * s).abs() < 0.5 && (rect.height - 16.0 * s).abs() < 0.5
            })
            .unwrap_or_else(|| panic!("scale {scale}: no crosshair"));
        // The crosshair sits one GUI unit right and below the exact centre
        // (`width / 2 - 7`), which vanilla does too.
        assert!((cross.x + cross.width / 2.0 - middle).abs() <= 2.0 * s);
        assert!((cross.y + cross.height / 2.0 - height as f32 / 2.0).abs() <= 2.0 * s);
    }
}

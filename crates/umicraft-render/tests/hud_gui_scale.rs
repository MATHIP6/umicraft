// SPDX-License-Identifier: MIT OR Apache-2.0

//! The "GUI Scale" option reaches the whole HUD, not only its text: the hotbar,
//! the hearts and the crosshair are sized by it too (`GuiIngame` builds its
//! `ScaledResolution` from `gameSettings.guiScale`).
//!
//! The option is a process-wide setting, so this lives in its own test binary
//! instead of racing the auto-scale tests of the library.

use umicraft_render::{draw_hud_gui, HudSheets, HudState};
use umicraft_ui::{set_gui_scale_option, DrawCommand, DrawList, FontAtlas, Rect, SpriteSheet};

fn hotbar_background(viewport: (u32, u32)) -> Option<Rect> {
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
    list.commands().iter().find_map(|command| match command {
        // The 182 x 22 texel background of the hotbar.
        DrawCommand::Image { dst, .. } if (dst.height / dst.width - 22.0 / 182.0).abs() < 1e-4 => {
            Some(*dst)
        }
        _ => None,
    })
}

#[test]
fn the_gui_scale_option_resizes_the_hotbar() {
    let viewport = (1920, 1080);

    set_gui_scale_option(0);
    let auto = hotbar_background(viewport).expect("a hotbar at the auto scale");
    assert_eq!(auto.width, 182.0 * 4.0, "auto is 4 at 1920x1080");

    set_gui_scale_option(2);
    let normal = hotbar_background(viewport).expect("a hotbar at the normal scale");
    assert_eq!(normal.width, 182.0 * 2.0, "the option caps the scale at 2");
    // It stays centred and on the bottom edge at the new scale.
    assert!((normal.x + normal.width / 2.0 - 960.0).abs() <= 2.0);
    assert!((normal.y + normal.height - 1080.0).abs() < 0.5);

    set_gui_scale_option(0);
}

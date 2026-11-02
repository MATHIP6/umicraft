// SPDX-License-Identifier: MIT OR Apache-2.0

//! The textured in-game HUD rendered into the immediate-mode [`DrawList`] pass
//! using authentic Minecraft 1.8 GUI textures and proportional font (M10).
//!
//! Hotbar, status rows (health, food, armor), experience bar, crosshair,
//! boss bar, chat, scoreboard sidebar, titles, action bar and tab list are
//! rendered in vanilla GUI scale with official textures from `widgets.png` and
//! `icons.png`. [`draw_hud_gui`] is the entry point; each element family
//! lives in its own submodule.

mod action_bar;
mod boss_bar;
mod chat;
mod crosshair;
mod debug_f3;
mod fade;
mod held_name;
mod hotbar;
mod icons;
mod runs;
#[cfg(test)]
mod scale_tests;
mod sidebar;
mod tab_layout;
mod tab_list;
mod titles;
mod vitals;
mod xp;

use umicraft_ui::screens::gui_scale;
use umicraft_ui::{DrawList, FontAtlas, SpriteSheet};

use action_bar::draw_action_bar;
use boss_bar::draw_boss_bar;
pub use chat::chat_run_at;
use chat::draw_chat;
use debug_f3::draw_debug_f3;
use held_name::draw_held_name;
use hotbar::draw_hotbar;
use icons::{draw_icons_layer, Canvas};
use sidebar::draw_sidebar;
use tab_list::draw_tab_list;
use titles::draw_title;

use super::HudState;

/// The sprite sheets extracted from the GUI atlas for HUD rendering.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HudSheets {
    /// The `widgets.png` region from the GUI atlas.
    pub widgets: Option<SpriteSheet>,
    /// The `icons.png` region from the GUI atlas.
    pub icons: Option<SpriteSheet>,
}

impl HudSheets {
    /// Returns true when both `widgets` and `icons` sheets are present.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.widgets.is_some() && self.icons.is_some()
    }
}

/// Draws the complete textured HUD into `list`.
pub fn draw_hud_gui(
    list: &mut DrawList,
    state: &HudState,
    viewport: (u32, u32),
    font: &FontAtlas,
    sheets: &HudSheets,
) {
    let s = gui_scale(viewport.0, viewport.1);
    let sf = s as f32;
    let gw = viewport.0 as f32 / sf;
    let gh = viewport.1 as f32 / sf;

    if let Some(widgets) = &sheets.widgets {
        draw_hotbar(&mut Canvas::new(list, viewport), state, font, widgets);
    }

    if let Some(icons) = &sheets.icons {
        draw_icons_layer(list, state, viewport, font, icons);
        if let Some(boss) = &state.boss {
            draw_boss_bar(list, state, boss, (gw, gh), sf, (font, icons));
        }
    }

    if let Some(name) = &state.held_name {
        draw_held_name(&mut Canvas::new(list, viewport), name, font);
    }

    draw_chat(list, state, gw, gh, sf, font);
    draw_social_overlays(list, state, (gw, gh), sf, (font, sheets.icons.as_ref()));
}

/// Draws the sidebar, title, action bar and, while the key is held, the tab
/// list, in the order of `GuiIngame.renderGameOverlay`.
fn draw_social_overlays(
    list: &mut DrawList,
    state: &HudState,
    size: (f32, f32),
    sf: f32,
    (font, icons): (&FontAtlas, Option<&SpriteSheet>),
) {
    if let Some(sidebar) = &state.sidebar {
        draw_sidebar(list, state, sidebar, size, sf, font);
    }
    if let Some(bar) = &state.action_bar {
        draw_action_bar(list, state, bar, size, sf, font);
    }
    if let Some(title) = &state.title {
        draw_title(list, state, title, size, sf, font);
    }
    if let (true, Some(tab)) = (state.show_tab_list, &state.tab_list) {
        draw_tab_list(list, state, tab, size.0, sf, (font, icons));
    }
    if let Some(view) = &state.debug_f3 {
        draw_debug_f3(list, view, size.0, sf, font);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_hud_gui_empty_state_emits_commands() {
        let mut list = DrawList::new();
        let state = HudState::default();
        let font = FontAtlas::fallback();
        let sheet = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 256, 256);
        let sheets = HudSheets {
            widgets: Some(sheet),
            icons: Some(sheet),
        };
        assert!(sheets.is_complete());

        draw_hud_gui(&mut list, &state, (1280, 720), &font, &sheets);
        assert!(!list.is_empty());

        let images = list
            .commands()
            .iter()
            .filter(|cmd| matches!(cmd, umicraft_ui::DrawCommand::Image { .. }))
            .count();
        // Hotbar, selector, crosshair, 10 hearts containers, 10 full hearts,
        // 10 food containers, 10 full food, xp bar background = 44 images
        assert!(images >= 40, "expected at least 40 images, got {images}");
    }

    #[test]
    fn draw_hud_gui_with_boss_bar() {
        use crate::BossView;

        let mut list = DrawList::new();
        let state = HudState {
            boss: Some(BossView {
                name: vec![runs::run("Ender Dragon")],
                health: 0.8,
            }),
            ..Default::default()
        };
        let font = FontAtlas::fallback();
        let sheet = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 256, 256);
        let sheets = HudSheets {
            widgets: Some(sheet),
            icons: Some(sheet),
        };

        draw_hud_gui(&mut list, &state, (1280, 720), &font, &sheets);
        let boss_text = list.commands().iter().any(|cmd| match cmd {
            umicraft_ui::DrawCommand::Text { text, .. } => text == "Ender Dragon",
            _ => false,
        });
        assert!(boss_text);
    }
}

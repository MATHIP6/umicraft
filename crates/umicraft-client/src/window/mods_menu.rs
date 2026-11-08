// SPDX-License-Identifier: MIT OR Apache-2.0

//! Window glue of the mods menu: the key bound to [`Action::Mods`] (right Shift
//! by default) that opens and closes the menu, the keys it takes, the blur it
//! puts over the game and the changes it makes, HUD widgets included.
//!
//! The screen is drawn by `MenuFlow` (see `menu_flow/mods.rs`) and laid out in
//! `umicraft_ui::menus::mods`; the blur is applied by the renderer.

use std::time::Instant;

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

use umicraft_core::lang::Lang;
use umicraft_render::ModWidgetView;
use umicraft_ui::menus::{ModsOutcome, ModsView};
use umicraft_ui::FontAtlas;

use super::App;
use crate::hud_editor_view;
use crate::menu_flow::MenuFlow;
use crate::mods_hud_view::mod_widget_views;
use crate::mods_runtime::{FrameInput, ModsRuntime};
use crate::mods_view;
use crate::options::{Action, Binding};
use crate::screen::Screen;

/// How blurred the game behind the current screen is, `0.0` to `1.0`: the
/// mods menu blurs it (and the blur fades in while the menu opens).
pub(super) fn background_blur(screen: Screen, menu: &MenuFlow) -> f32 {
    match screen {
        Screen::Mods => menu.mods_blur(),
        _ => 0.0,
    }
}

/// The mod widgets the HUD draws. The mods menu draws them itself, where the
/// player drags them, so the HUD leaves them out while it is open.
pub(super) fn hud_widgets(
    screen: Screen,
    mods: &ModsRuntime,
    font: &FontAtlas,
    display: (u32, u32),
) -> Vec<ModWidgetView> {
    if screen == Screen::Mods {
        Vec::new()
    } else {
        mod_widget_views(mods.hud_widgets(), font, display)
    }
}

/// What the mods menu lists and the HUD widgets it lets the player move: built
/// only while it is open, because it is rebuilt every frame and no other screen
/// reads it.
pub(super) fn view_for(
    screen: Screen,
    (mods, lang): (&ModsRuntime, &Lang),
    (font, display): (&FontAtlas, (u32, u32)),
) -> ModsView {
    if screen == Screen::Mods {
        ModsView {
            hud: hud_editor_view::build(mods.registry(), lang, font, display),
            ..mods_view::build(mods.registry(), lang)
        }
    } else {
        ModsView::default()
    }
}

impl App {
    /// The key bound to [`Action::Mods`] (right Shift by default, the left one
    /// stays the sneak key) opens the mods menu over a game and closes it
    /// again. It is taken before the other key bindings so it triggers no
    /// action sharing it, and only where it means something: elsewhere it
    /// stays a plain key. A mouse button bound to the action is not a shortcut
    /// and is ignored here. Returns whether the key was taken.
    pub(super) fn take_mods_shortcut(&mut self, event: &KeyEvent, code: KeyCode) -> bool {
        let applies = matches!(self.options.keys.get(Action::Mods), Binding::Key(key) if key == code)
            && self.session.is_some()
            && !self.death_screen_visible()
            && matches!(self.screen(), Screen::None | Screen::Mods);
        if applies && event.state == ElementState::Pressed && !event.repeat {
            self.toggle_mods_menu();
        }
        applies
    }

    /// Feeds the built-in mods once per frame with what the window knows.
    pub(super) fn update_mods(&mut self, now: Instant) {
        let eye = self.camera.eye();
        self.mods.update(&FrameInput {
            now_ms: u64::try_from(now.duration_since(self.started).as_millis()).unwrap_or(u64::MAX),
            fps: self.fps.round() as u32,
            frame_ms: self.frame_ms,
            eye: [eye.x, eye.y, eye.z],
            yaw_radians: self.camera.yaw,
            pitch_radians: self.camera.pitch,
            sprinting: self.held(Action::Sprint),
            ping_ms: self.world.as_ref().and_then(|world| world.hud().ping_ms),
        });
    }

    /// Opens the mods menu over the game, or closes it when it is open.
    fn toggle_mods_menu(&mut self) {
        let next = if self.screen == Screen::Mods {
            Screen::None
        } else {
            Screen::Mods
        };
        self.set_screen(next);
    }

    /// Applies a change the player made on the mods menu (a switch, a setting,
    /// a HUD widget dropped, resized or reset) and saves the options when it
    /// went through, never for the frames of a drag. A change the registry
    /// refuses (a mod or setting that does not exist) is logged and dropped.
    pub(super) fn apply_mods_change(&mut self, change: &ModsOutcome) {
        match mods_view::apply(self.mods.registry_mut(), change) {
            Ok(true) => {
                self.options.mods = self.mods.config();
                self.save_options();
            }
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, "ignored a change of the mods menu"),
        }
    }

    /// Hands a key to the mods menu. Escape closes it from the landing page; on
    /// the list and the settings pages it goes to the UI, which steps back.
    pub(super) fn handle_mods_key(&mut self, event: &KeyEvent) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        if code == KeyCode::Escape {
            let fresh = event.state == ElementState::Pressed && !event.repeat;
            match escape_target(self.menu.mods_can_go_back()) {
                Some(screen) if fresh => self.set_screen(screen),
                Some(_) => {}
                None if fresh => self.ui_input.key(code, event.state, event.text.as_deref()),
                None => {}
            }
            return;
        }
        self.ui_input.key(code, event.state, event.text.as_deref());
    }
}

/// The screen Escape leads to from the mods menu, or `None` when the key is
/// the UI's: the list and the settings pages handle it themselves, to step back
/// before the menu closes.
fn escape_target(can_go_back: bool) -> Option<Screen> {
    (!can_go_back).then_some(Screen::None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: Escape on the settings page closed the whole menu because
    /// the window took it before the UI.
    #[test]
    fn escape_steps_back_inside_the_menu_before_it_closes() {
        assert_eq!(escape_target(true), None, "the UI goes back a page");
        assert_eq!(escape_target(false), Some(Screen::None));
    }

    #[test]
    fn the_hud_leaves_the_mod_widgets_to_the_open_mods_menu() {
        let mut mods = ModsRuntime::new(&crate::options::ModsOptions::default());
        // The mod is on and has run once, so it has a line to draw.
        assert!(mods.registry_mut().set_enabled("fps", true).is_ok());
        mods.update(&FrameInput {
            fps: 60,
            ..FrameInput::default()
        });
        let font = FontAtlas::fallback();
        let on_hud = hud_widgets(Screen::None, &mods, &font, (800, 600));
        assert!(!on_hud.is_empty(), "the HUD draws it in the game");
        assert!(hud_widgets(Screen::Mods, &mods, &font, (800, 600)).is_empty());
    }
}

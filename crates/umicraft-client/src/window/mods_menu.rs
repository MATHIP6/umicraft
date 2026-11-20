// SPDX-License-Identifier: MIT OR Apache-2.0

//! Window glue of the mods menu: the key bound to [`Action::Mods`] (right Shift
//! by default) that opens and closes the menu, the keys it takes, the blur it
//! puts over the game and the changes it makes, HUD widgets included.
//!
//! The screen is drawn by `MenuFlow` (see `screens/menu_flow/mods.rs`) and laid out in
//! `umicraft_ui::menus::mods`; the blur is applied by the renderer.

use std::time::Instant;

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::KeyCode;

use umicraft_builtin_mods::{CombatInfo, KeyState, MAX_EFFECTS};
use umicraft_core::lang::Lang;
use umicraft_modhost::ModHost;
use umicraft_render::ModWidgetView;
use umicraft_ui::menus::{ModsOutcome, ModsView};
use umicraft_ui::FontAtlas;

use super::App;
use crate::game::GameEvent;
use crate::mods::external::ExternalMods;
use crate::mods::external_menu;
use crate::mods::hud::{item_of, mod_widget_view, mod_widget_views};
use crate::mods::hud_editor;
use crate::mods::icons::ModIcons;
use crate::mods::menu;
use crate::mods::runtime::{FrameInput, ModsRuntime};
use crate::options::{Action, Binding};
use crate::resources::item_icons::ItemView;
use crate::screens::menu_flow::MenuFlow;
use crate::screens::Screen;

/// How blurred the game behind the current screen is, `0.0` to `1.0`: the
/// mods menu blurs it (and the blur fades in while the menu opens).
pub(super) fn background_blur(screen: Screen, menu: &MenuFlow) -> f32 {
    match screen {
        Screen::Mods => menu.mods_blur(),
        _ => 0.0,
    }
}

/// The mod widgets the HUD draws: the built-in ones, then those the external mods
/// drew this frame (see [`App::update_mods`]), converted the same way. The mods menu
/// draws them itself, where the player drags them, so the HUD leaves them out while
/// it is open.
pub(super) fn hud_widgets(
    screen: Screen,
    (mods, external): (&ModsRuntime, Option<&ModHost>),
    font: &FontAtlas,
    display: (u32, u32),
    icons: &ModIcons<'_>,
) -> Vec<ModWidgetView> {
    if screen == Screen::Mods {
        return Vec::new();
    }
    let mut views = mod_widget_views(mods.hud_widgets().map(item_of), font, display, icons);
    if let Some(host) = external {
        host.visit_widgets(|drawn| {
            let item = (drawn.placement, drawn.content);
            views.extend(mod_widget_view(item, font, display, icons));
        });
    }
    views
}

/// What the mods menu lists and the HUD widgets it lets the player move: built
/// only while it is open, because it is rebuilt every frame and no other screen
/// reads it.
pub(super) fn view_for(
    screen: Screen,
    (mods, external, lang): (&ModsRuntime, Option<&ExternalMods>, &Lang),
    (font, display): (&FontAtlas, (u32, u32)),
    icons: &ModIcons<'_>,
) -> ModsView {
    if screen == Screen::Mods {
        let mut hud = hud_editor::build(mods.registry(), lang, font, display, icons);
        hud.history = mods.history_state();
        let mut view = ModsView {
            hud,
            ..menu::build(mods.registry(), lang)
        };
        if let Some((host, untrusted)) =
            external.and_then(|mods| Some((mods.host()?, mods.untrusted())))
        {
            external_menu::extend(&mut view, host, untrusted, lang);
            hud_editor::external::extend(&mut view.hud, host, font, display, icons);
        }
        view
    } else {
        ModsView::default()
    }
}

impl App {
    /// What the pictures of the mod widgets are made from this frame: the
    /// inventory of the session, the item resources and the GUI textures.
    pub(super) fn mod_icons(&self) -> ModIcons<'_> {
        let world = self.world.as_ref();
        ModIcons {
            slots: world.map(|world| &world.window_view().player_slots[..]),
            selected: world.map_or(0, |world| world.hud().selected_slot),
            items: Some(ItemView::new(
                self.assets.items.as_deref(),
                self.dials.frames(),
            )),
            gui_atlas: self.assets.gui_atlas.as_ref(),
        }
    }

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

    /// Feeds the mods once per frame with what the window knows: the built-in ones
    /// get their context and the external ones the `Frame` event made from it, the
    /// external ones draw their widgets (so they are drawn
    /// once per frame whether the HUD or the HUD editor shows them), and the chat
    /// lines the external mods printed go to the game.
    pub(super) fn update_mods(&mut self, now: Instant) {
        if self.session.is_some() {
            if let Some(host) = self.external_mods.as_ref().and_then(ExternalMods::host) {
                host.draw_widgets();
            }
        }
        self.forward_mod_chat();
        let eye = self.camera.eye();
        let (hud, slots) = match self.world.as_ref() {
            Some(world) => (Some(world.hud()), Some(&world.window_view().player_slots)),
            None => (None, None),
        };
        let (armor, held) = slots.map_or(Default::default(), |slots| {
            let selected = hud.map_or(0, |hud| hud.selected_slot);
            crate::mods::facts::equipment(slots, selected)
        });
        self.mods.update(
            &FrameInput {
                now_ms: u64::try_from(now.duration_since(self.clock.started).as_millis())
                    .unwrap_or(u64::MAX),
                fps: self.clock.fps.round() as u32,
                frame_ms: self.clock.frame_ms,
                eye: [eye.x, eye.y, eye.z],
                yaw_radians: self.camera.yaw,
                pitch_radians: self.camera.pitch,
                sprinting: self.held(Action::Sprint),
                ping_ms: hud.and_then(|hud| hud.ping_ms),
                keys: KeyState {
                    forward: self.held(Action::Forward),
                    left: self.held(Action::Left),
                    back: self.held(Action::Back),
                    right: self.held(Action::Right),
                    jump: self.held(Action::Jump),
                    attack: self.held(Action::Attack),
                    use_item: self.held(Action::Use),
                },
                combat: hud.map_or(CombatInfo::default(), |hud| CombatInfo {
                    hits: hud.combat.0,
                    hurts: hud.combat.1,
                }),
                armor,
                held,
                effects: hud.map_or([None; MAX_EFFECTS], |hud| {
                    crate::mods::facts::effects(&hud.active_effects)
                }),
            },
            &mut self.render_bus,
        );
    }

    /// Opens the mods menu over the game, or closes it when it is open.
    fn toggle_mods_menu(&mut self) {
        let next = if self.screen == Screen::Mods {
            Screen::None
        } else {
            // "Cancel" of the HUD editor goes back to the state of now.
            self.mods.begin_edit();
            Screen::Mods
        };
        self.set_screen(next);
    }

    /// Applies a change the player made on the mods menu (a switch, a setting,
    /// a HUD widget dropped, resized or reset) and saves the options when it
    /// went through, never for the frames of a drag. A change the registry
    /// refuses (a mod or setting that does not exist) is logged and dropped.
    pub(super) fn apply_mods_change(&mut self, change: &ModsOutcome) {
        let route = external_menu::route(change);
        if let Some(request) = route.external {
            self.apply_external_change(request);
        }
        if route.builtin {
            self.apply_builtin_change(change);
        }
    }

    fn apply_builtin_change(&mut self, change: &ModsOutcome) {
        match self.mods.apply_change(change) {
            Ok(true) => {
                self.options.mods = self.mods.config();
                self.save_options();
                self.send_legacy_animations();
            }
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, "ignored a change of the mods menu"),
        }
    }

    /// Carries out a change of the menu for an external mod and saves that mod's
    /// choices. A refused change (a failed mod's switch, an unknown key) is logged.
    fn apply_external_change(&mut self, request: external_menu::Request<'_>) {
        if let external_menu::Request::Trust { mod_id } = request {
            if let Some(mods) = &mut self.external_mods {
                mods.trust(mod_id);
            }
            return;
        }
        let Some(mods) = &self.external_mods else {
            return;
        };
        let Some(host) = mods.host() else {
            return;
        };
        let compat = match request {
            external_menu::Request::ServerCompat { enabled } => Some(enabled),
            _ => None,
        };
        match external_menu::apply(host, request) {
            Ok(changed) => changed.iter().for_each(|id| mods.save_config(id)),
            Err(error) => tracing::warn!(%error, "ignored a change of the mods menu"),
        }
        if let Some(enabled) = compat {
            self.options.multiplayer.mod_server_compat = enabled;
            self.save_options();
        }
    }

    /// Tells the game thread which 1.7 animations to run (ADR-0079).
    pub(super) fn send_legacy_animations(&self) {
        self.send_event(GameEvent::LegacyAnimations(self.mods.legacy_animations()));
    }

    /// Hands a key to the mods menu. Escape closes it from the list; on the
    /// settings page it goes to the UI, which steps back to the list.
    pub(super) fn handle_mods_key(&mut self, event: &KeyEvent) {
        let Some(code) = self.text_key_code(event) else {
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
        mods.update(
            &FrameInput {
                fps: 60,
                ..FrameInput::default()
            },
            &mut umicraft_core::events::EventBus::default(),
        );
        let font = FontAtlas::fallback();
        let on_hud = hud_widgets(
            Screen::None,
            (&mods, None),
            &font,
            (800, 600),
            &ModIcons::default(),
        );
        assert!(!on_hud.is_empty(), "the HUD draws it in the game");
        assert!(hud_widgets(
            Screen::Mods,
            (&mods, None),
            &font,
            (800, 600),
            &ModIcons::default()
        )
        .is_empty());
    }
}

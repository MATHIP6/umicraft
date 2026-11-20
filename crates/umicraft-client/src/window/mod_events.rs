// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mod events of the render thread (ADR-0086, F3.3): connection and screen
//! changes.
//!
//! The window owns one [`EventBus`] for them, built once at startup. Every
//! emission checks `has_subscribers` first, so with no mod nothing is allocated.
//! A child of [`super`] so it reaches the window state without widening its
//! visibility.

use umicraft_modhost::{
    Connected, Disconnected, EventThread, KeyBinding, KeyPressed, ScreenClosed, ScreenOpened,
};

use super::App;
use crate::game::{GameEvent, GameMods, SessionMods};
use crate::mods::external::ExternalMods;
use crate::network::NetMods;
use crate::options::Binding;
use crate::screens::Screen;

/// The bindings among `bindings` that `pressed` triggers, in load order.
///
/// A switched-off mod is listed too (its bindings stay on the controls page); the
/// host drops an event aimed at it, so it receives nothing.
fn triggered<'a>(
    bindings: impl Iterator<Item = KeyBinding<'a>>,
    pressed: Binding,
) -> impl Iterator<Item = KeyBinding<'a>> {
    bindings.filter(move |binding| Binding::from_name(&binding.current) == Some(pressed))
}

/// The names of the screen events a change from `old` to `new` fires: the old
/// screen closes, then the new one opens. [`Screen::None`] is "no screen" and
/// fires neither, and an unchanged screen fires nothing.
pub(super) fn screen_events(
    old: Screen,
    new: Screen,
) -> (Option<&'static str>, Option<&'static str>) {
    if old == new {
        return (None, None);
    }
    let named = |screen: Screen| (screen != Screen::None).then(|| screen.mod_name());
    (named(old), named(new))
}

impl App {
    /// The mods of a new session: the bus and the client commands of its game
    /// thread, the bus and the injected packets of its network thread. They move
    /// into those threads through `Session::start`.
    pub(super) fn session_mods(&self) -> SessionMods {
        let Some(mods) = &self.external_mods else {
            return SessionMods::default();
        };
        SessionMods {
            game: GameMods {
                bus: mods.bus(EventThread::Game),
                commands: mods.commands(),
                physics: mods.physics(),
            },
            network: NetMods {
                bus: mods.bus(EventThread::Network),
                outbox: mods.packet_outbox(),
            },
        }
    }

    /// Hands the local chat lines the mods printed to the game thread, which shows
    /// them in the chat, plus one notice naming each mod that stopped working. Called
    /// every frame so the outbox never fills up; without a session there is no chat
    /// to show them in, so they are dropped.
    pub(super) fn forward_mod_chat(&mut self) {
        let Some(mods) = &mut self.external_mods else {
            return;
        };
        let lang = crate::language::current();
        let mut lines: Vec<String> = mods.drain_chat().collect();
        lines.extend(
            mods.take_stopped()
                .iter()
                .map(|name| lang.tr_args("umicraft.mods.external.stopped", &[name])),
        );
        for line in lines {
            self.send_event(GameEvent::LocalChat(line));
        }
    }

    /// Tells the mods a key binding of theirs was pressed: every enabled mod whose
    /// binding is `pressed`, if any. A key that is also bound to a built-in action
    /// fires both, as two vanilla bindings on one key do.
    pub(super) fn emit_key_pressed(&mut self, pressed: Binding) {
        if !self.render_bus.has_subscribers::<KeyPressed>() {
            return;
        }
        let Some(host) = self.external_mods.as_ref().and_then(ExternalMods::host) else {
            return;
        };
        let events: Vec<KeyPressed> = triggered(host.key_bindings(), pressed)
            .map(|binding| KeyPressed::new(binding.mod_id.clone(), binding.key))
            .collect();
        for mut event in events {
            self.render_bus.dispatch(&mut event);
        }
    }

    /// Tells the mods a session to `address` was started.
    ///
    /// Fired when the session threads are up, before the server accepted the
    /// player: the window cannot know sooner or later without waiting for the
    /// game thread. Always paired with one [`App::emit_disconnected`].
    pub(super) fn emit_connected(&mut self, address: &str) {
        if self.render_bus.has_subscribers::<Connected>() {
            self.render_bus.dispatch(&mut Connected::new(address));
        }
    }

    /// Tells the mods a session ended; `reason` is empty when the player left.
    pub(super) fn emit_disconnected(&mut self, reason: &str) {
        if self.render_bus.has_subscribers::<Disconnected>() {
            self.render_bus.dispatch(&mut Disconnected::new(reason));
        }
    }

    /// Tells the mods the screen changed from `old` to `new`.
    pub(super) fn emit_screen_events(&mut self, old: Screen, new: Screen) {
        let (closed, opened) = screen_events(old, new);
        if let Some(name) = closed.filter(|_| self.render_bus.has_subscribers::<ScreenClosed>()) {
            self.render_bus.dispatch(&mut ScreenClosed::new(name));
        }
        if let Some(name) = opened.filter(|_| self.render_bus.has_subscribers::<ScreenOpened>()) {
            self.render_bus.dispatch(&mut ScreenOpened::new(name));
        }
    }
}

#[cfg(test)]
mod tests {
    use umicraft_modhost::ModId;
    use winit::keyboard::KeyCode;

    use super::*;

    fn binding<'a>(mod_id: &'a ModId, key: &'a str, current: &str) -> KeyBinding<'a> {
        KeyBinding {
            mod_id,
            key,
            label: "",
            default_binding: "",
            current: current.to_owned(),
        }
    }

    #[test]
    fn a_press_triggers_every_binding_on_that_key_and_no_other() {
        let (a, b): (ModId, ModId) = ("a".parse().expect("id"), "b".parse().expect("id"));
        let all = vec![
            binding(&a, "toggle", "KeyG"),
            binding(&a, "other", "KeyH"),
            binding(&b, "go", "KeyG"),
            binding(&b, "mouse", "Mouse4"),
            binding(&b, "unbound", ""),
        ];
        let keys = |pressed| -> Vec<&str> {
            triggered(all.clone().into_iter(), pressed)
                .map(|found| found.key)
                .collect()
        };
        assert_eq!(keys(Binding::Key(KeyCode::KeyG)), ["toggle", "go"]);
        assert_eq!(keys(Binding::Mouse(3)), ["mouse"]);
        assert!(keys(Binding::Key(KeyCode::KeyZ)).is_empty());
    }

    #[test]
    fn an_unchanged_screen_fires_nothing() {
        assert_eq!(screen_events(Screen::Chat, Screen::Chat), (None, None));
    }

    #[test]
    fn a_change_closes_the_old_screen_then_opens_the_new_one() {
        assert_eq!(
            screen_events(Screen::GameMenu, Screen::Options),
            (Some("game_menu"), Some("options"))
        );
    }

    #[test]
    fn the_empty_screen_fires_no_event() {
        assert_eq!(
            screen_events(Screen::None, Screen::Chat),
            (None, Some("chat"))
        );
        assert_eq!(
            screen_events(Screen::Chat, Screen::None),
            (Some("chat"), None)
        );
    }
}

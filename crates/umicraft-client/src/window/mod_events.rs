// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mod events of the render thread (ADR-0086, F3.3): connection and screen
//! changes.
//!
//! The window owns one [`EventBus`] for them, built once at startup. Every
//! emission checks `has_subscribers` first, so with no mod nothing is allocated.
//! A child of [`super`] so it reaches the window state without widening its
//! visibility.

use umicraft_core::events::EventBus;
use umicraft_modhost::{Connected, Disconnected, EventThread, ScreenClosed, ScreenOpened};

use super::App;
use crate::screens::Screen;

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
    /// The bus of the game thread of a new session, with the mods' game-thread
    /// subscriptions. It moves into that thread through `Session::start`.
    pub(super) fn game_bus(&self) -> EventBus {
        self.external_mods
            .as_ref()
            .map(|mods| mods.bus(EventThread::Game))
            .unwrap_or_default()
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
    use super::*;

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

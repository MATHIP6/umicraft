// SPDX-License-Identifier: MIT OR Apache-2.0

//! The lifetime of the play session behind the window (ADR-0044): starting
//! one, leaving the connecting screen once the server placed the player,
//! ending one for a kick or the pause menu, and sending events while there
//! may be none.
//!
//! The decisions are pure functions ([`lifecycle_step`], [`network_config`])
//! so they are tested without a window; the [`App`] methods only apply them.
//! A child of [`super`] so it reaches the window state without widening its
//! visibility.

use super::App;
use crate::args::Args;
use crate::connection_end::{ConnectionEnd, EndPhase};
use crate::game::{GameEvent, Session};
use crate::network::NetworkConfig;
use crate::options::Options;
use crate::screen::Screen;
use crate::servers::{ResourcePackMode, ServerAddress};

/// What the window does with the session this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Lifecycle {
    /// Nothing changes.
    Stay,
    /// The server placed the player: leave the connecting screen.
    StartPlaying,
    /// The session ended (a kick or a connection error) for this reason.
    Disconnect(ConnectionEnd),
}

/// Decides the next step from the current screen, whether the player has
/// been placed and why the session ended, if it did.
///
/// The end wins over everything: a kick that arrives together with the first
/// placement still shows the reason. A placement only matters while the
/// connecting screen is up; the server places the player again after a
/// respawn or a teleport and that must not close the game menu.
pub(super) fn lifecycle_step(
    screen: Screen,
    placed: bool,
    ended: Option<&ConnectionEnd>,
) -> Lifecycle {
    if let Some(end) = ended {
        Lifecycle::Disconnect(end.clone())
    } else if screen == Screen::Connecting && placed {
        Lifecycle::StartPlaying
    } else {
        Lifecycle::Stay
    }
}

/// The network settings of a session to `address`.
///
/// The handshake host override of the command line only applies to the server
/// named there (it exists to record a session on a public server).
pub(super) fn network_config(
    args: &Args,
    options: &Options,
    address: &ServerAddress,
    pack_mode: ResourcePackMode,
) -> NetworkConfig {
    let is_cli_server = args.server() == Some((address.host.as_str(), address.port));
    NetworkConfig {
        host: address.host.clone(),
        port: address.port,
        name: args.name.clone(),
        session: args.session.clone(),
        view_distance: options.video.render_distance,
        chat_mode: options.chat.visibility.wire(),
        chat_colors: options.chat.colors,
        skin_parts: options.skin.parts_mask(),
        handshake_host: args.handshake_host.clone().filter(|_| is_cli_server),
        decline_server_packs: pack_mode == ResourcePackMode::Disabled,
    }
}

/// Drops `session` on its own thread.
///
/// Dropping a [`Session`] joins the game and network threads, and the network
/// thread may sit in a connect attempt for seconds. The render thread must
/// never wait for that (ADR-0005), so cancelling a connection hands the
/// session to a short-lived thread.
fn release_in_background(session: Session) {
    let spawned = std::thread::Builder::new()
        .name("umicraft-session-drop".to_string())
        .spawn(move || drop(session));
    if let Err(error) = spawned {
        // The closure, and with it the session, was dropped by the failed
        // spawn on this thread; nothing is left to clean up.
        tracing::warn!(%error, "could not drop the session in the background");
    }
}

impl App {
    /// Sends `event` to the game thread. Returns whether it was queued; with
    /// no session, or a full inbox, it is not and the caller may retry on the
    /// next frame.
    pub(super) fn send_event(&self, event: GameEvent) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.events().try_send(event).is_ok())
    }

    /// Starts a session to `address` and shows the connecting screen.
    ///
    /// The world view is cleared first, so nothing of an earlier session
    /// shows. A thread that cannot start ends in the disconnected screen
    /// with the error as its reason.
    pub(super) fn connect(&mut self, address: &ServerAddress) {
        self.stop_session();
        let pack_mode = self.menu.pack_mode_for(address);
        let config = network_config(&self.args, &self.options, address, pack_mode);
        match Session::start(config) {
            Ok(session) => {
                self.server_pack.set_mode(pack_mode);
                self.session = Some(session);
                self.send_particle_setting();
                self.set_screen(Screen::Connecting);
            }
            Err(error) => self.show_disconnected(&ConnectionEnd::other(
                EndPhase::Connecting,
                error.to_string(),
            )),
        }
    }

    /// Connects to the server named on the command line, if any. Tests and
    /// scripts keep their `umicraft-client <host> <port>` start this way.
    pub(super) fn connect_from_arguments(&mut self) {
        if let Some((host, port)) = self.args.server() {
            let address = ServerAddress {
                host: host.to_string(),
                port,
            };
            self.connect(&address);
        }
    }

    /// Ends the session and goes to the server list (the pause menu's
    /// Disconnect, cancelling a connection).
    pub(super) fn disconnect(&mut self) {
        self.stop_session();
        self.set_screen(Screen::ServerList);
    }

    /// Ends the session and shows the disconnected screen for `end`, in the
    /// player's language.
    pub(super) fn show_disconnected(&mut self, end: &ConnectionEnd) {
        self.stop_session();
        self.menu.set_disconnected(end, &self.lang);
        self.set_screen(Screen::Disconnected);
    }

    /// Applies the lifecycle decision of this frame.
    pub(super) fn apply_lifecycle(&mut self, step: Lifecycle) {
        match step {
            Lifecycle::Stay => {}
            Lifecycle::StartPlaying => self.set_screen(Screen::None),
            Lifecycle::Disconnect(end) => self.show_disconnected(&end),
        }
    }

    /// Stops and drops the session, forgets everything the world view and
    /// the window kept of it and frees the mouse.
    fn stop_session(&mut self) {
        if let Some(session) = self.session.take() {
            release_in_background(session);
        }
        if let (Some(world), Some(renderer)) = (self.world.as_mut(), self.renderer.as_mut()) {
            world.clear_session(renderer);
        }
        self.forget_play_state();
        self.drop_server_pack();
        self.capture_mouse(false);
    }

    /// Resets what belongs to one play session: the death screen, the open
    /// inventory, the text screens and the input that was waiting to be sent.
    fn forget_play_state(&mut self) {
        self.death_started = None;
        self.respawn_requested = false;
        self.container_was_open = false;
        self.last_input = crate::input::InputState::default();
        self.anvil_field = crate::anvil_field::AnvilField::default();
        self.text_screens = crate::text_screens::TextScreens::default();
        self.inventory_input.reset();
        self.creative_input.reset();
        self.buttons.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Args {
        let words: Vec<String> = words.iter().map(|word| (*word).to_string()).collect();
        Args::parse(&words).expect("valid arguments")
    }

    fn address(host: &str, port: u16) -> ServerAddress {
        ServerAddress {
            host: host.to_string(),
            port,
        }
    }

    #[test]
    fn the_connecting_screen_ends_with_the_placement() {
        assert_eq!(
            lifecycle_step(Screen::Connecting, false, None),
            Lifecycle::Stay
        );
        assert_eq!(
            lifecycle_step(Screen::Connecting, true, None),
            Lifecycle::StartPlaying
        );
    }

    #[test]
    fn a_later_placement_does_not_close_another_screen() {
        for screen in [Screen::None, Screen::GameMenu, Screen::Stats, Screen::Chat] {
            assert_eq!(lifecycle_step(screen, true, None), Lifecycle::Stay);
        }
    }

    #[test]
    fn the_end_of_the_session_wins_over_the_placement() {
        assert_eq!(
            lifecycle_step(
                Screen::Connecting,
                true,
                Some(&ConnectionEnd::kicked("kicked"))
            ),
            Lifecycle::Disconnect(ConnectionEnd::kicked("kicked"))
        );
        assert_eq!(
            lifecycle_step(
                Screen::None,
                false,
                Some(&ConnectionEnd::kicked("timed out"))
            ),
            Lifecycle::Disconnect(ConnectionEnd::kicked("timed out"))
        );
    }

    #[test]
    fn the_network_settings_come_from_the_options_and_the_name_from_the_arguments() {
        let mut options = Options::default();
        options.video.render_distance = 12;
        options.chat.colors = false;
        let config = network_config(
            &args(&["--name", "Pixel"]),
            &options,
            &address("play.example.net", 25570),
            ResourcePackMode::Prompt,
        );
        assert_eq!(config.host, "play.example.net");
        assert_eq!(config.port, 25570);
        assert_eq!(config.name, "Pixel");
        assert_eq!(config.view_distance, 12);
        assert!(!config.chat_colors);
        assert_eq!(config.skin_parts, options.skin.parts_mask());
    }

    #[test]
    fn the_handshake_override_only_applies_to_the_server_of_the_command_line() {
        let args = args(&["10.0.0.1", "25565", "--handshake-host", "play.example.net"]);
        let options = Options::default();
        let cli = network_config(
            &args,
            &options,
            &address("10.0.0.1", 25565),
            ResourcePackMode::Prompt,
        );
        assert_eq!(cli.handshake_host.as_deref(), Some("play.example.net"));
        let other = network_config(
            &args,
            &options,
            &address("10.0.0.2", 25565),
            ResourcePackMode::Prompt,
        );
        assert_eq!(other.handshake_host, None);
    }

    #[test]
    fn only_a_disabled_pack_mode_declines_offers_at_the_network_layer() {
        let (args, options) = (args(&[]), Options::default());
        let declines =
            |mode| network_config(&args, &options, &address("h", 1), mode).decline_server_packs;
        assert!(declines(ResourcePackMode::Disabled));
        assert!(!declines(ResourcePackMode::Prompt));
        assert!(!declines(ResourcePackMode::Enabled));
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Rust helper for Umicraft mod authors (ADR-0086).
//!
//! A mod implements [`Mod`], registers handlers through [`InitCx`] and exports
//! the C entry points with [`umicraft_mod!`]. All `unsafe` lives in the private
//! `ffi` module (ADR-0087); everything a mod author touches is safe.
//!
//! # Events
//!
//! | View | Thread | Cancellable | Text setter |
//! |---|---|---|---|
//! | [`PreTick`], [`PostTick`] | game | no | no |
//! | [`ChatReceived`] | game | yes | [`ChatReceived::set_json`] |
//! | [`ChatSend`] | game | yes | [`ChatSend::set_text`] |
//! | [`Connected`], [`Disconnected`] | render | no | no |
//! | [`ScreenOpened`], [`ScreenClosed`] | render | no | no |
//! | [`KeyPressed`] | render | no | no |
//! | [`PacketReceived`], [`PacketSend`] | network | yes | no |
//!
//! The packet events and [`InitCx::packets`] need `network_packets = true` in
//! the mod's `mod.toml`; without it the host answers `DENIED`.
//!
//! # Settings, HUD, keys and commands
//!
//! During `init` a mod also declares what the player can see and change:
//! [`InitCx::toggle`], `slider`, `choice` and `color` return a [`Setting`]
//! handle to read later; [`InitCx::hud_widget`] draws with a [`Hud`] at a
//! [`Placement`]; [`InitCx::key`] adds a key binding that arrives as
//! [`KeyPressed`]; [`InitCx::command`] adds a `/command`; [`InitCx::chat`]
//! gives a [`Chat`] to print local lines.
//!
//! # Physics and world queries
//!
//! A mod with `movement = true` and `wraps = ["player_physics"]` (or
//! `replaces`) in `mod.toml` can take part in the player's movement step:
//! [`InitCx::wrap_physics`] and [`InitCx::replace_physics`] register a closure
//! that gets a [`Physics`]. It runs on the game thread under a time budget and
//! runs again for past ticks when the server corrects the position, so keep it
//! a function of state and input. [`InitCx::world`] gives a [`World`] to read
//! blocks and entities, answered only on the game thread inside a callback.
//!
//! # Post-process passes
//!
//! [`InitCx::post_pass`] registers a WGSL fragment shader that the host runs
//! over the world picture; the returned [`PostPass`] sets its parameters with
//! [`PostPass::set_param`]. The contract is in `docs/modding/post.md`.
//!
//! Setters work only inside [`InitCx::intercept`] handlers; observers get a
//! copy and the host answers `DENIED`.
//!
//! ```ignore
//! struct Hello;
//!
//! impl umicraft_modapi::Mod for Hello {
//!     fn init(cx: &mut umicraft_modapi::InitCx<'_>) -> Result<Self, umicraft_modapi::ModError> {
//!         let log = cx.logger();
//!         cx.observe::<umicraft_modapi::PreTick>(0, move |e| log.info(&format!("tick {}", e.tick())))?;
//!         Ok(Hello)
//!     }
//! }
//!
//! umicraft_modapi::umicraft_mod!(Hello);
//! ```

mod chat;
mod error;
mod event;
mod ffi;
mod host;
mod hud;
mod init;
mod logger;
mod packets;
mod physics;
mod post;
#[doc(hidden)]
pub mod private;
mod setting;
mod world;

pub use chat::Chat;
pub use error::ModError;
pub use event::{
    ChatPosition, ChatReceived, ChatSend, Connected, Control, Disconnected, Event, KeyPressed,
    PacketReceived, PacketSend, PostTick, PreTick, ScreenClosed, ScreenOpened,
};
pub use hud::{Anchor, Hud, Placement};
pub use init::{InitCx, Mod};
pub use logger::Logger;
pub use packets::Packets;
pub use physics::{Input, Physics, PlayerState};
pub use post::PostPass;
pub use setting::Setting;
pub use world::{Block, Entity, World};

/// Exports `umi_mod_api_version`, `umi_mod_init` and `umi_mod_shutdown` for a
/// type implementing [`Mod`]; use it once per mod library.
///
/// The macro is a thin shim: all logic (panic containment, state, validation)
/// lives in the hidden `private` module so it can be tested and kept out of
/// the mod's own crate.
#[macro_export]
macro_rules! umicraft_mod {
    ($mod_ty:ty) => {
        static UMI_MOD_STATE: $crate::private::ModState<$mod_ty> = $crate::private::ModState::new();

        /// C entry point: API version this mod was built for.
        #[allow(unsafe_code)]
        #[unsafe(no_mangle)]
        pub extern "C" fn umi_mod_api_version() -> $crate::private::UmiVersion {
            $crate::private::api_version()
        }

        /// C entry point: builds the mod and registers its handlers.
        #[allow(unsafe_code)]
        #[unsafe(no_mangle)]
        pub extern "C" fn umi_mod_init(
            host: *const $crate::private::UmiHost,
            me: $crate::private::UmiModHandle,
        ) -> $crate::private::UmiStatus {
            $crate::private::init(&UMI_MOD_STATE, host, me)
        }

        /// C entry point: shuts the mod down and frees its handlers.
        #[allow(unsafe_code)]
        #[unsafe(no_mangle)]
        pub extern "C" fn umi_mod_shutdown() {
            $crate::private::shutdown(&UMI_MOD_STATE)
        }
    };
}

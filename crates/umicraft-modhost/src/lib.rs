// SPDX-License-Identifier: MIT OR Apache-2.0

//! Host side of the Umicraft modding API (ADR-0086).
//!
//! F3.1 covers the part that needs no native code: reading a mod's `mod.toml`
//! ([`Manifest`]) and deciding which of the installed mods load, and in which order
//! ([`resolve`]). Every mod that cannot load comes back with a [`RejectReason`] whose
//! `Display` is written for the player, so the launcher can show why instead of
//! failing silently.
//!
//! F3.2 adds the native side: [`discover`] finds installed mods, [`TrustList`] gates
//! libraries by SHA-256, [`CrashSentinel`] detects an unclean exit, and [`ModHost`]
//! loads libraries and runs their lifecycle. All `unsafe` is confined to `ffi`.
//!
//! F3.3 adds dispatch: the [`events`] the host emits, [`ModHost::install`] which
//! adapts each mod's handlers onto a core `EventBus` (intercept handlers run on the
//! owning thread under a time budget, observers on a per-mod worker thread), and
//! per-mod [`ModStats`].
//!
//! F3.4 adds what the player sees ([`ui`]): schema settings, HUD widgets, key bindings and
//! client commands that mods declare during init, plus the [`ModHost`] methods the mods
//! menu, the HUD, the controls page and the chat use.
//!
//! F3.6 adds the `player_physics` service ([`PhysicsChain`], built by
//! [`ModHost::physics`]) and read-only world queries ([`WorldQuery`], lent to the game
//! thread with [`with_world`]).

mod budget;
mod context;
mod discover;
mod dispatch;
pub mod error;
pub mod events;
mod ffi;
mod hook;
mod host;
mod load;
pub mod manifest;
mod packets;
mod physics;
pub mod resolve;
mod sentinel;
mod trust;
pub mod ui;
mod worker;
mod world;

pub use budget::{ModStats, BUDGET, HUD_BUDGET, LOG_EVERY, MAX_CONSECUTIVE_OVERRUNS};
pub use context::{ModContext, Subscription};
pub use discover::{discover, Discovered};
pub use dispatch::install_mod;
pub use error::{
    ConfigError, ConfigureError, DeclareError, DiscoverError, EnableError, IdError, LoadError,
    LoadFailure, ManifestError, TrustError,
};
pub use events::{
    ChatReceived, ChatSend, Connected, Disconnected, EventThread, HostEvent, KeyPressed,
    PacketReceived, PacketSend, PostTick, PreTick, ScreenClosed, ScreenOpened,
};
pub use ffi::{host_table, UserPtr};
pub use host::ModHost;
pub use manifest::{
    library_file_name, platform_dir, ApiVersion, ApiVersionError, Arch, Load, Manifest, ModId, Os,
    Permissions, ServiceId, KNOWN_SERVICES, PLAYER_PHYSICS,
};
pub use packets::{PacketOutbox, MAX_PACKET_BODY_BYTES};
pub use physics::PhysicsChain;
pub use resolve::{resolve, Candidate, RejectReason, Rejection, Resolution};
pub use sentinel::{CrashSentinel, SentinelGuard};
pub use trust::{sha256_file, TrustList};
pub use ui::commands::{CommandOutcome, CommandTable};
pub use ui::config::ExternalModConfig;
pub use ui::hud::DrawnWidget;
pub use ui::settings::{SettingSpec, SettingSpecKind};
pub use ui::shared::HostShared;
pub use ui::view::{KeyBinding, ModView, SettingView, WidgetView};
pub use worker::QUEUE_CAPACITY;
pub use world::{with_world, EntityView, WorldQuery};

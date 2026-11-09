// SPDX-License-Identifier: MIT OR Apache-2.0
//! Built-in client mods of Umicraft (ADR-0065).
//!
//! A built-in mod is a feature of the client the player can switch on, configure
//! and, for the ones that draw on the HUD, move and resize: an FPS counter, the
//! position, a sprint toggle... They are compiled into the client. They are **not**
//! the external mods with an API (F3); nothing here is part of that API.
//!
//! The design is data-driven so that adding a mod is one small file:
//!
//! - a mod describes itself ([`ModMeta`]) and its options ([`SettingDef`]), and the
//!   settings page of the menu is generated from that description;
//! - a mod that shows something fills a [`HudContent`] (text lines); one generic
//!   renderer draws every mod, placed by a [`HudPlacement`];
//! - a mod that changes a behaviour edits a [`ModInput`];
//! - the [`ModRegistry`] owns all of them and their saved state ([`ModsConfig`]).
//!
//! The crate has no GPU, window or network dependency, so everything is unit tested.

pub mod config;
pub mod context;
pub mod hud;
pub mod id;
pub mod meta;
pub mod mods;
pub mod registry;
pub mod settings;
pub mod traits;

pub use config::{ModConfig, ModsConfig};
pub use context::ModContext;
pub use hud::{
    Anchor, HudContent, HudLine, HudPlacement, HudRect, HudStyle, Shadow, MAX_SCALE, MIN_SCALE,
};
pub use id::{ModCategory, ModId};
pub use meta::ModMeta;
pub use registry::{HudWidget, ModRegistry, ModView, RegistryError};
pub use settings::{ModSettings, SettingDef, SettingKind, SettingValue, SettingsError};
pub use traits::{BuiltinMod, HudSpec, ModInput};

// SPDX-License-Identifier: MIT OR Apache-2.0

//! What a mod declares for the player: settings, HUD widgets, key bindings and
//! client commands (API 0.3, F3.4, ADR-0086).
//!
//! Declarations are accepted during `umi_mod_init` only and validated here, so the
//! client menu and the HUD can trust them. The shared data model of the built-in
//! mods (`umicraft-builtin-mods`: [`HudContent`](umicraft_builtin_mods::HudContent),
//! [`HudPlacement`](umicraft_builtin_mods::HudPlacement),
//! [`SettingValue`](umicraft_builtin_mods::SettingValue)) is reused instead of
//! duplicated, and so are its validation rules.
//!
//! - [`settings`]: specs and the lock-free atomic slots that hold the values.
//! - [`declared`]: the per-mod lists and the checks that guard them.
//! - [`hud`]: widgets, their draw call and the visitor that exposes them.
//! - [`commands`] and [`shared`]: the host-wide command table and chat outbox.
//! - [`menu`]: categories and modules (API 1.2), the mod's own tabs and cards.
//! - [`config`] and [`view`]: persistence and read access for the mods menu.

pub mod commands;
pub mod config;
mod configure;
mod declare;
pub(crate) mod declared;
pub(crate) mod draw;
pub mod hud;
pub(crate) mod keys;
pub mod menu;
pub mod settings;
pub mod shared;
pub(crate) mod state;
#[cfg(test)]
pub(crate) mod testing;
pub(crate) mod validate;
pub mod view;

/// Longest label (setting, widget, key binding), in bytes; about 40 CJK characters.
pub const MAX_LABEL_BYTES: usize = 128;
/// Longest command usage text, in bytes.
pub const MAX_USAGE_BYTES: usize = 256;
/// Longest label of one option of a choice setting, in bytes.
pub const MAX_OPTION_BYTES: usize = 64;
/// Longest key name (`KeyG`, `Mouse4`, ...) a mod or a config may give, in bytes.
pub const MAX_BINDING_BYTES: usize = 32;
/// Longest text on a button (API 1.2), in bytes.
pub const MAX_BUTTON_TEXT_BYTES: usize = 32;
/// Longest label of a category (a tab of the mods menu, API 1.2), in bytes.
pub const MAX_CATEGORY_BYTES: usize = 32;
/// Longest description of a module (API 1.2), in bytes.
pub const MAX_DESCRIPTION_BYTES: usize = 256;
/// Most categories one mod may declare (API 1.2).
pub const MAX_CATEGORIES: usize = 2;
/// Most modules one mod may declare (API 1.2).
pub const MAX_MODULES: usize = 16;
/// Most options a choice setting may have.
pub const MAX_CHOICES: usize = 32;
/// Most settings one mod may declare.
pub const MAX_SETTINGS: usize = 64;
/// Most HUD widgets one mod may declare.
pub const MAX_WIDGETS: usize = 16;
/// Most key bindings one mod may declare.
pub const MAX_KEYS: usize = 32;
/// Most commands one mod may declare.
pub const MAX_COMMANDS: usize = 32;
/// Longest chat line `chat_print` accepts, in bytes.
pub const MAX_CHAT_BYTES: usize = 1024;
/// Longest text of one HUD item, in bytes; longer text is cut at a character
/// boundary (it is only displayed, so cutting is harmless).
pub const MAX_HUD_TEXT_BYTES: usize = 256;
/// Most lines, cells, labels and ticks (each) one widget may draw per frame.
pub const MAX_HUD_ITEMS: usize = 256;
/// Local chat lines that may wait for the client before new ones are dropped.
pub const OUTBOX_CAPACITY: usize = 64;
/// Injected packets that may wait for the network thread before `packet_send` answers `BUSY`.
pub const PACKET_OUTBOX_CAPACITY: usize = 64;

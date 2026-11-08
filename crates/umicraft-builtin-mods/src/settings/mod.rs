// SPDX-License-Identifier: MIT OR Apache-2.0
//! The options of a mod: their description ([`SettingDef`]), their values
//! ([`SettingValue`]) and the per-mod set of current values ([`ModSettings`]).

mod schema;
mod store;
mod value;

pub use schema::{SettingDef, SettingKind};
pub use store::{ModSettings, SettingsError};
pub use value::SettingValue;

// SPDX-License-Identifier: MIT OR Apache-2.0
//! A setting value as stored in `options.toml`.

use serde::{Deserialize, Serialize};

/// The value of one setting.
///
/// Choices store the option's key (not its index) and colors store `"#rrggbb"`, so
/// a saved file survives a reordering of the options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SettingValue {
    /// A toggle.
    Bool(bool),
    /// A slider.
    Number(f32),
    /// A choice or a color.
    Text(String),
}

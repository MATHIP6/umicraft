// SPDX-License-Identifier: MIT OR Apache-2.0

//! The player's choices for one external mod, as saved on disk.
//!
//! The client stores one file per mod, `<options dir>/mods/<id>.toml`, which is
//! this struct. Values are sanitised against the mod's declarations when applied
//! ([`ModHost::apply_config`](crate::ModHost::apply_config)), so a hand-edited file
//! cannot put a setting out of range.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use umicraft_builtin_mods::{HudPlacement, SettingValue};

use crate::error::ConfigError;

/// Saved choices of one mod. Every field has a default, so an older or partial
/// file loads; keys the mod does not declare are kept when the config is applied
/// and written back, so downgrading and upgrading a mod loses nothing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExternalModConfig {
    /// The player's switch; `None` keeps the default (on).
    pub enabled: Option<bool>,
    /// Setting key to value (a choice is stored as its option label, a colour as
    /// `#rrggbb`).
    pub settings: BTreeMap<String, SettingValue>,
    /// Widget key to placement.
    pub placements: BTreeMap<String, HudPlacement>,
    /// Key-binding id to key name.
    pub keys: BTreeMap<String, String>,
}

impl ExternalModConfig {
    /// Reads the config at `path`; a missing file is the default config (first run).
    ///
    /// # Errors
    ///
    /// [`ConfigError::Io`] for a read failure other than "not found", or
    /// [`ConfigError::Parse`] for malformed content.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match fs::read_to_string(path) {
            Ok(text) => Ok(toml::from_str(&text)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Writes the config to `path` through a temporary file and a rename, so a crash
    /// while saving never leaves a truncated file (like the trust list).
    ///
    /// # Errors
    ///
    /// [`ConfigError::Io`] or [`ConfigError::Serialize`].
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let text = toml::to_string(self)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension("toml.tmp");
        fs::write(&temp, text)?;
        fs::rename(&temp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use umicraft_builtin_mods::Anchor;

    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("umi-modcfg-{}-{name}", std::process::id()))
    }

    fn sample() -> ExternalModConfig {
        ExternalModConfig {
            enabled: Some(false),
            settings: BTreeMap::from([
                ("on".to_owned(), SettingValue::Bool(true)),
                ("size".to_owned(), SettingValue::Number(2.5)),
                ("style".to_owned(), SettingValue::Text("b".to_owned())),
            ]),
            placements: BTreeMap::from([(
                "main".to_owned(),
                HudPlacement::at(Anchor::BottomRight, -4.0, 8.0),
            )]),
            keys: BTreeMap::from([("toggle".to_owned(), "KeyG".to_owned())]),
        }
    }

    #[test]
    fn a_missing_file_is_the_default_config() {
        let config = ExternalModConfig::load(&scratch("missing.toml")).unwrap();
        assert_eq!(config, ExternalModConfig::default());
    }

    #[test]
    fn save_then_load_round_trips_and_leaves_no_temp_file() {
        let dir = scratch("roundtrip");
        let path = dir.join("hello.toml");
        sample().save(&path).unwrap();
        assert_eq!(ExternalModConfig::load(&path).unwrap(), sample());
        assert!(!path.with_extension("toml.tmp").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_partial_file_fills_the_rest_with_defaults_and_garbage_is_an_error() {
        let dir = scratch("partial");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("m.toml");
        std::fs::write(&path, "enabled = true\n").unwrap();
        let config = ExternalModConfig::load(&path).unwrap();
        assert_eq!(config.enabled, Some(true));
        assert!(config.settings.is_empty());
        std::fs::write(&path, "enabled = [").unwrap();
        assert!(matches!(
            ExternalModConfig::load(&path),
            Err(ConfigError::Parse(_))
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! TOML configuration store (spec §9).
//!
//! Configuration is one human-editable file of `[module-id]` sections. Modules
//! parse their section on demand into typed structs; missing sections are
//! `None` and the module applies its own defaults. Values are handled as TOML,
//! parsed lazily, so adding a module never changes the file layout of others.

use std::fs;
use std::path::Path;
use std::str::FromStr;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::CoreError;

/// A parsed configuration file exposing one section per module.
#[derive(Debug, Default, Clone)]
pub struct ConfigStore {
    /// The whole document; keyed by module id, preserving file order.
    doc: toml::Table,
}

impl FromStr for ConfigStore {
    type Err = CoreError;

    /// Parses in-memory TOML text (tests, defaults) — `text.parse::<ConfigStore>()`.
    ///
    /// # Errors
    /// [`CoreError::ConfigParse`] when `text` is not valid TOML.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            doc: text.parse::<toml::Table>()?,
        })
    }
}

impl ConfigStore {
    /// Loads and parses a configuration `toml` file from `path`.
    ///
    /// # Errors
    /// [`CoreError::ConfigIo`] when reading fails, [`CoreError::ConfigParse`]
    /// when the file is not valid TOML.
    pub fn from_path(path: &Path) -> Result<Self, CoreError> {
        let text = fs::read_to_string(path).map_err(|source| CoreError::ConfigIo {
            path: path.display().to_string(),
            source,
        })?;
        text.parse::<ConfigStore>()
    }

    /// Parses the `[module]` section into `T`.
    ///
    /// Returns `Ok(None)` when the module has no section yet; the caller is
    /// expected to apply its documented defaults.
    ///
    /// # Errors
    /// [`CoreError::ConfigParse`] when the section exists but does not fit `T`.
    pub fn section<T: DeserializeOwned>(&self, module: &str) -> Result<Option<T>, CoreError> {
        match self.doc.get(module) {
            None => Ok(None),
            // `Value` implements serde deserialization, so unknown/ill-typed
            // fields surface here as typed parse errors, never as panics.
            Some(value) => Ok(Some(value.clone().try_into()?)),
        }
    }

    /// Inserts or replaces the `[module]` section with a serializable value.
    ///
    /// # Errors
    /// [`CoreError::ConfigSerialize`] when `value` cannot round-trip through TOML.
    pub fn set_section<V: Serialize>(&mut self, module: &str, value: &V) -> Result<(), CoreError> {
        self.doc
            .insert(module.to_string(), toml::Value::try_from(value)?);
        Ok(())
    }

    /// Writes the document back to TOML text at `path`.
    ///
    /// # Errors
    /// [`CoreError::ConfigSerialize`] for unserializable content,
    /// [`CoreError::ConfigIo`] when writing fails.
    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        let text = toml::to_string(&self.doc)?;
        fs::write(path, text).map_err(|source| CoreError::ConfigIo {
            path: path.display().to_string(),
            source,
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq)]
    struct Hooks {
        tick_hz: u64,
    }

    #[test]
    fn parses_a_typed_section() {
        let store = "[scheduler]\ntick_hz = 20\n"
            .parse::<ConfigStore>()
            .unwrap();
        let hooks: Option<Hooks> = store.section("scheduler").unwrap();
        assert_eq!(hooks, Some(Hooks { tick_hz: 20 }));
    }

    #[test]
    fn missing_section_is_none() {
        let store = "".parse::<ConfigStore>().unwrap();
        let hooks: Option<Hooks> = store.section("scheduler").unwrap();
        assert_eq!(hooks, None);
    }

    #[test]
    fn malformed_toml_is_a_typed_error() {
        let result = "not = [toml".parse::<ConfigStore>();
        assert!(matches!(result, Err(CoreError::ConfigParse(_))));
    }

    #[test]
    fn set_then_load_round_trips_a_file() {
        let path =
            std::env::temp_dir().join(format!("umicraft-config-test-{}.toml", std::process::id()));
        let mut store = ConfigStore::default();
        store
            .set_section("scheduler", &Hooks { tick_hz: 42 })
            .unwrap();
        store.save(&path).unwrap();

        let loaded = ConfigStore::from_path(&path).unwrap();
        let hooks: Option<Hooks> = loaded.section("scheduler").unwrap();
        assert_eq!(hooks, Some(Hooks { tick_hz: 42 }));

        let _ = fs::remove_file(&path);
    }
}

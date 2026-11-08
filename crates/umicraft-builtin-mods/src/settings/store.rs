// SPDX-License-Identifier: MIT OR Apache-2.0
//! The current values of a mod's settings.

use std::collections::BTreeMap;

use thiserror::Error;

use super::{SettingDef, SettingValue};

/// Why a setting could not be changed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SettingsError {
    /// The mod has no setting with this key.
    #[error("unknown setting `{0}`")]
    UnknownKey(String),
}

/// The values of one mod's settings, always complete and valid: every key of the
/// schema is present, and every value passed [`SettingDef::sanitize`].
#[derive(Debug, Clone, PartialEq)]
pub struct ModSettings {
    schema: &'static [SettingDef],
    values: BTreeMap<&'static str, SettingValue>,
}

impl ModSettings {
    /// The settings of a player who changed nothing.
    #[must_use]
    pub fn defaults(schema: &'static [SettingDef]) -> Self {
        let values = schema.iter().map(|d| (d.key, d.default_value())).collect();
        Self { schema, values }
    }

    /// The schema these values follow.
    #[must_use]
    pub fn schema(&self) -> &'static [SettingDef] {
        self.schema
    }

    /// The value of `key`, if the schema has it.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&SettingValue> {
        self.values.get(key)
    }

    /// A toggle's state. A mod reads only keys of its own schema, so an absent or
    /// mistyped key (a bug of the mod, caught by its tests) reads as `false`.
    #[must_use]
    pub fn bool(&self, key: &str) -> bool {
        matches!(self.get(key), Some(SettingValue::Bool(true)))
    }

    /// A slider's value, or `0.0` for an absent or mistyped key.
    #[must_use]
    pub fn number(&self, key: &str) -> f32 {
        match self.get(key) {
            Some(SettingValue::Number(n)) => *n,
            _ => 0.0,
        }
    }

    /// A choice's key or a color, or `""` for an absent or mistyped key.
    #[must_use]
    pub fn text(&self, key: &str) -> &str {
        match self.get(key) {
            Some(SettingValue::Text(t)) => t,
            _ => "",
        }
    }

    /// Changes a setting; the value is made valid first.
    ///
    /// # Errors
    ///
    /// [`SettingsError::UnknownKey`] if the schema has no such key.
    pub fn set(&mut self, key: &str, value: &SettingValue) -> Result<(), SettingsError> {
        let def = self
            .schema
            .iter()
            .find(|d| d.key == key)
            .ok_or_else(|| SettingsError::UnknownKey(key.to_owned()))?;
        self.values.insert(def.key, def.sanitize(value));
        Ok(())
    }

    /// Loads saved values over the defaults. Keys this version does not know (from
    /// a newer one) are ignored, invalid values fall back to the default.
    pub fn load(&mut self, saved: &BTreeMap<String, SettingValue>) {
        for def in self.schema {
            if let Some(value) = saved.get(def.key) {
                self.values.insert(def.key, def.sanitize(value));
            }
        }
    }

    /// The values, for saving.
    #[must_use]
    pub fn to_map(&self) -> BTreeMap<String, SettingValue> {
        self.values
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SettingKind;

    const SCHEMA: &[SettingDef] = &[
        SettingDef {
            key: "on",
            label_key: "k",
            kind: SettingKind::Toggle { default: true },
        },
        SettingDef {
            key: "size",
            label_key: "k",
            kind: SettingKind::Slider {
                min: 1.0,
                max: 3.0,
                step: 1.0,
                default: 2.0,
            },
        },
    ];

    #[test]
    fn defaults_fill_every_key() {
        let s = ModSettings::defaults(SCHEMA);
        assert!(s.bool("on"));
        assert!((s.number("size") - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn set_sanitizes_the_value() {
        let mut s = ModSettings::defaults(SCHEMA);
        s.set("size", &SettingValue::Number(50.0))
            .expect("known key");
        assert!((s.number("size") - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn set_rejects_an_unknown_key() {
        let mut s = ModSettings::defaults(SCHEMA);
        assert_eq!(
            s.set("nope", &SettingValue::Bool(true)),
            Err(SettingsError::UnknownKey("nope".into()))
        );
    }

    #[test]
    fn load_ignores_unknown_keys_and_repairs_bad_values() {
        let mut saved = BTreeMap::new();
        saved.insert("future".to_owned(), SettingValue::Bool(true));
        saved.insert("on".to_owned(), SettingValue::Number(7.0));
        saved.insert("size".to_owned(), SettingValue::Number(3.0));
        let mut s = ModSettings::defaults(SCHEMA);
        s.load(&saved);
        assert!(s.bool("on"), "wrong type -> default");
        assert!((s.number("size") - 3.0).abs() < f32::EPSILON);
        assert!(!s.to_map().contains_key("future"));
    }

    #[test]
    fn a_missing_key_reads_as_the_neutral_value() {
        let s = ModSettings::defaults(SCHEMA);
        assert!(!s.bool("absent"));
        assert_eq!(s.text("absent"), "");
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0
//! The saved state of the mods: the `[mods.<id>]` tables of `options.toml`.

use std::collections::BTreeMap;

use serde::de::{Deserializer, IgnoredAny};
use serde::{Deserialize, Serialize};

use crate::hud::HudPlacement;
use crate::settings::SettingValue;

/// The saved state of one mod.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ModConfig {
    /// `None` means the player never chose, so the mod's default applies.
    pub enabled: Option<bool>,
    /// Values of the settings, by key.
    #[serde(deserialize_with = "lenient_settings")]
    pub settings: BTreeMap<String, SettingValue>,
    /// Where the widget is, for a HUD mod the player moved.
    pub placement: Option<HudPlacement>,
    /// What the mod itself keeps between two runs, apart from its settings: the
    /// switch of toggle sprint, say. Keys are the mod's own and append-only.
    #[serde(deserialize_with = "lenient_settings")]
    pub state: BTreeMap<String, SettingValue>,
}

/// The saved state of all mods, by mod id.
///
/// Ids this version does not know (a newer version, or a removed mod) are kept as
/// they are and written back, so switching versions never loses a configuration.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModsConfig(pub BTreeMap<String, ModConfig>);

/// Reads a settings table, dropping the values of a type no setting uses (a list, a
/// table) instead of failing the whole file.
fn lenient_settings<'de, D>(de: D) -> Result<BTreeMap<String, SettingValue>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Known(SettingValue),
        Unknown(IgnoredAny),
    }
    let raw = BTreeMap::<String, Raw>::deserialize(de)?;
    Ok(raw
        .into_iter()
        .filter_map(|(key, value)| match value {
            Raw::Known(value) => Some((key, value)),
            Raw::Unknown(_) => None,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::Anchor;

    #[test]
    fn a_config_round_trips_through_toml() {
        let mut config = ModsConfig::default();
        config.0.insert(
            "fps".into(),
            ModConfig {
                enabled: Some(true),
                settings: BTreeMap::from([("shadow".to_owned(), SettingValue::Bool(true))]),
                placement: Some(HudPlacement::at(Anchor::TopRight, -4.0, 4.0)),
                state: BTreeMap::from([("toggled".to_owned(), SettingValue::Bool(true))]),
            },
        );
        let text = toml::to_string(&config).expect("serializes");
        assert_eq!(toml::from_str::<ModsConfig>(&text), Ok(config));
    }

    #[test]
    fn a_hand_written_file_reads() {
        let config: ModsConfig = toml::from_str(
            "[fps]\nenabled = true\n[fps.settings]\nsize = 2\ncolor = \"#ff0000\"\n[fps.placement]\nanchor = \"bottom_right\"\n",
        )
        .expect("parses");
        let fps = &config.0["fps"];
        assert_eq!(fps.enabled, Some(true));
        assert_eq!(fps.settings["size"], SettingValue::Number(2.0));
        assert_eq!(fps.placement.map(|p| p.anchor), Some(Anchor::BottomRight));
    }

    #[test]
    fn an_unsupported_setting_type_is_dropped_not_fatal() {
        let config: ModsConfig =
            toml::from_str("[fps]\n[fps.settings]\nbad = [1, 2]\ngood = true\n").expect("parses");
        let settings = &config.0["fps"].settings;
        assert!(!settings.contains_key("bad"));
        assert_eq!(settings["good"], SettingValue::Bool(true));
    }

    #[test]
    fn a_file_without_state_reads_with_an_empty_one() {
        let config: ModsConfig = toml::from_str(
            "[fps]
enabled = true
",
        )
        .expect("parses");
        assert!(config.0["fps"].state.is_empty());
    }

    #[test]
    fn an_empty_file_is_an_empty_config() {
        assert_eq!(toml::from_str("").ok(), Some(ModsConfig::default()));
    }
}

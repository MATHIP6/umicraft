// SPDX-License-Identifier: MIT OR Apache-2.0

//! Reading and changing a mod's player-facing state: the [`ModContext`] half of
//! [`ModHost::config`](crate::ModHost::config) and friends.

use std::collections::BTreeMap;

use umicraft_builtin_mods::{HudPlacement, SettingValue};

use super::config::ExternalModConfig;
use crate::context::ModContext;
use crate::error::ConfigureError;

fn unknown(kind: &'static str, key: &str) -> ConfigureError {
    ConfigureError::UnknownKey {
        kind,
        key: key.to_owned(),
    }
}

/// Offers each entry to `apply`, which returns whether a declaration claimed it;
/// the others go to `retained` so they survive a round trip through the config file.
fn route<V: Clone>(
    entries: &BTreeMap<String, V>,
    retained: &mut BTreeMap<String, V>,
    mut apply: impl FnMut(&str, &V) -> bool,
) {
    for (key, value) in entries {
        if !apply(key, value) {
            retained.insert(key.clone(), value.clone());
        }
    }
}

impl ModContext {
    /// The mod's current choices: the player's switch, every declared setting,
    /// widget placement and key binding, plus the entries of an earlier config that
    /// no declaration claims.
    pub fn config(&self) -> ExternalModConfig {
        let declared = self.ui.declared();
        let mut config = self.ui.retained().clone();
        config.enabled = Some(self.is_wanted());
        for slot in declared.settings() {
            config
                .settings
                .insert(slot.spec().key.clone(), slot.value());
        }
        for widget in declared.widgets() {
            config
                .placements
                .insert(widget.key().to_owned(), widget.placement());
        }
        for key in declared.keys() {
            config.keys.insert(key.key().to_owned(), key.current());
        }
        config
    }

    /// Applies saved choices: values are sanitised against the declarations (a slider
    /// is clamped, an unknown choice becomes the default, a placement is clamped),
    /// entries no declaration claims are kept for [`config`](Self::config), and the
    /// player's switch is set unless the mod failed.
    pub fn apply_config(&self, config: &ExternalModConfig) {
        let declared = self.ui.declared();
        let mut retained = self.ui.retained();
        route(&config.settings, &mut retained.settings, |key, value| {
            declared.setting(key).map(|slot| slot.set(value)).is_some()
        });
        route(
            &config.placements,
            &mut retained.placements,
            |key, value| {
                declared
                    .widget(key)
                    .map(|widget| widget.set_placement(*value))
                    .is_some()
            },
        );
        route(&config.keys, &mut retained.keys, |key, value| {
            declared.key(key).map(|slot| slot.set(value)).is_some()
        });
        if let Some(enabled) = config.enabled {
            // A failed mod stays off; there is nothing to report to a config loader.
            self.set_wanted(enabled);
        }
    }

    /// Changes setting `key`; the value is sanitised like a loaded one.
    ///
    /// # Errors
    ///
    /// [`ConfigureError::UnknownKey`] if the mod declared no such setting.
    pub fn set_setting(&self, key: &str, value: &SettingValue) -> Result<(), ConfigureError> {
        let slot = self.ui.declared().setting(key);
        slot.ok_or_else(|| unknown("setting", key))?.set(value);
        Ok(())
    }

    /// Moves widget `widget`; the placement is sanitised (scale clamped).
    ///
    /// # Errors
    ///
    /// [`ConfigureError::UnknownKey`] if the mod declared no such widget.
    pub fn set_placement(
        &self,
        widget: &str,
        placement: HudPlacement,
    ) -> Result<(), ConfigureError> {
        let slot = self.ui.declared().widget(widget);
        slot.ok_or_else(|| unknown("widget", widget))?
            .set_placement(placement);
        Ok(())
    }

    /// Binds key binding `key` to the key called `binding` (empty for unbound).
    ///
    /// # Errors
    ///
    /// [`ConfigureError::UnknownKey`] if the mod declared no such key binding.
    pub fn set_key(&self, key: &str, binding: &str) -> Result<(), ConfigureError> {
        let slot = self.ui.declared().key(key);
        slot.ok_or_else(|| unknown("key binding", key))?
            .set(binding);
        Ok(())
    }
}

#[cfg(test)]
mod tests;

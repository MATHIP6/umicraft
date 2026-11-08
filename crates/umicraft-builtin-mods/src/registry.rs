// SPDX-License-Identifier: MIT OR Apache-2.0
//! The registry: owns every built-in mod and its state.

use std::collections::BTreeMap;

use thiserror::Error;

use crate::config::{ModConfig, ModsConfig};
use crate::context::ModContext;
use crate::hud::{HudContent, HudPlacement};
use crate::id::ModId;
use crate::meta::ModMeta;
use crate::settings::{ModSettings, SettingValue, SettingsError};
use crate::traits::{BuiltinMod, ModInput};

/// Why a registry operation failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RegistryError {
    /// Two mods use the same id.
    #[error("two mods share the id `{0}`")]
    DuplicateId(ModId),
    /// No mod has this id.
    #[error("unknown mod `{0}`")]
    UnknownMod(String),
    /// The mod rejected the setting.
    #[error(transparent)]
    Setting(#[from] SettingsError),
}

/// A mod and everything the registry keeps about it.
struct Entry {
    module: Box<dyn BuiltinMod>,
    enabled: bool,
    settings: ModSettings,
    placement: HudPlacement,
    content: HudContent,
}

impl Entry {
    fn new(module: Box<dyn BuiltinMod>) -> Self {
        let meta = module.meta();
        let settings = ModSettings::defaults(module.settings());
        let placement = module
            .hud()
            .map(|h| h.default_placement)
            .unwrap_or_default();
        Self {
            enabled: meta.default_enabled,
            module,
            settings,
            placement,
            content: HudContent::default(),
        }
    }

    fn is_hud(&self) -> bool {
        self.module.hud().is_some()
    }
}

/// A read-only view of a mod, for the menu.
#[derive(Debug, Clone, Copy)]
pub struct ModView<'a> {
    /// The static description.
    pub meta: &'static ModMeta,
    /// Whether the mod is on.
    pub enabled: bool,
    /// Current values and schema of the settings.
    pub settings: &'a ModSettings,
    /// Where the widget is, if the mod has one.
    pub placement: Option<HudPlacement>,
}

/// A widget to draw this frame.
#[derive(Debug, Clone, Copy)]
pub struct HudWidget<'a> {
    /// Which mod it belongs to.
    pub id: ModId,
    /// Where and how big.
    pub placement: HudPlacement,
    /// What to draw.
    pub content: &'a HudContent,
}

/// All built-in mods, their enabled state, settings and placements.
pub struct ModRegistry {
    entries: Vec<Entry>,
    /// Saved tables of ids no mod here claims, written back untouched.
    unknown: BTreeMap<String, ModConfig>,
}

impl ModRegistry {
    /// Builds a registry from mods, each at its defaults.
    ///
    /// # Errors
    ///
    /// [`RegistryError::DuplicateId`] if two mods have the same id.
    pub fn new(mods: Vec<Box<dyn BuiltinMod>>) -> Result<Self, RegistryError> {
        let mut entries: Vec<Entry> = Vec::with_capacity(mods.len());
        for module in mods {
            let id = module.meta().id;
            if entries.iter().any(|e| e.module.meta().id == id) {
                return Err(RegistryError::DuplicateId(id));
            }
            entries.push(Entry::new(module));
        }
        Ok(Self {
            entries,
            unknown: BTreeMap::new(),
        })
    }

    /// Views of all mods, in registration order.
    pub fn mods(&self) -> impl Iterator<Item = ModView<'_>> {
        self.entries.iter().map(|e| ModView {
            meta: e.module.meta(),
            enabled: e.enabled,
            settings: &e.settings,
            placement: e.is_hud().then_some(e.placement),
        })
    }

    /// Turns a mod on or off.
    ///
    /// # Errors
    ///
    /// [`RegistryError::UnknownMod`] if no mod has this id.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), RegistryError> {
        self.entry_mut(id)?.enabled = enabled;
        Ok(())
    }

    /// Changes a setting of a mod.
    ///
    /// # Errors
    ///
    /// [`RegistryError::UnknownMod`] or [`RegistryError::Setting`].
    pub fn set_setting(
        &mut self,
        id: &str,
        key: &str,
        value: &SettingValue,
    ) -> Result<(), RegistryError> {
        Ok(self.entry_mut(id)?.settings.set(key, value)?)
    }

    /// Moves or resizes a HUD widget. The placement is sanitized first.
    ///
    /// # Errors
    ///
    /// [`RegistryError::UnknownMod`] if no mod has this id.
    pub fn set_placement(
        &mut self,
        id: &str,
        placement: HudPlacement,
    ) -> Result<(), RegistryError> {
        self.entry_mut(id)?.placement = placement.sanitized();
        Ok(())
    }

    /// Puts a HUD widget back where its mod puts it by default.
    ///
    /// # Errors
    ///
    /// [`RegistryError::UnknownMod`] if no mod has this id.
    pub fn reset_placement(&mut self, id: &str) -> Result<(), RegistryError> {
        let entry = self.entry_mut(id)?;
        entry.placement = entry
            .module
            .hud()
            .map(|h| h.default_placement)
            .unwrap_or_default();
        Ok(())
    }

    /// Runs one frame of every enabled mod and refreshes its widget text.
    pub fn update(&mut self, ctx: &ModContext) {
        for entry in self.entries.iter_mut().filter(|e| e.enabled) {
            entry.module.update(ctx, &entry.settings);
            if entry.is_hud() {
                entry.content.clear();
                entry.module.draw(ctx, &entry.settings, &mut entry.content);
            }
        }
    }

    /// The widgets to draw: enabled HUD mods, in registration order.
    pub fn hud_widgets(&self) -> impl Iterator<Item = HudWidget<'_>> {
        self.entries
            .iter()
            .filter(|e| e.enabled && e.is_hud())
            .map(|e| HudWidget {
                id: e.module.meta().id,
                placement: e.placement,
                content: &e.content,
            })
    }

    /// Lets every enabled mod rewrite the movement input.
    pub fn modify_input(&mut self, input: &mut ModInput) {
        for entry in self.entries.iter_mut().filter(|e| e.enabled) {
            entry.module.modify_input(input, &entry.settings);
        }
    }

    /// Applies saved state over the defaults. Invalid values are repaired; tables of
    /// unknown mods are kept for [`config`](Self::config).
    pub fn apply_config(&mut self, config: &ModsConfig) {
        self.unknown.clear();
        for (id, saved) in &config.0 {
            match self
                .entries
                .iter_mut()
                .find(|e| e.module.meta().id.as_str() == id)
            {
                Some(entry) => {
                    if let Some(enabled) = saved.enabled {
                        entry.enabled = enabled;
                    }
                    entry.settings.load(&saved.settings);
                    if let (Some(placement), true) = (saved.placement, entry.is_hud()) {
                        entry.placement = placement.sanitized();
                    }
                }
                None => {
                    self.unknown.insert(id.clone(), saved.clone());
                }
            }
        }
    }

    /// The state to save: every mod, plus the tables of unknown mods.
    #[must_use]
    pub fn config(&self) -> ModsConfig {
        let mut tables = self.unknown.clone();
        for entry in &self.entries {
            tables.insert(
                entry.module.meta().id.as_str().to_owned(),
                ModConfig {
                    enabled: Some(entry.enabled),
                    settings: entry.settings.to_map(),
                    placement: entry.is_hud().then_some(entry.placement),
                },
            );
        }
        ModsConfig(tables)
    }

    fn entry_mut(&mut self, id: &str) -> Result<&mut Entry, RegistryError> {
        self.entries
            .iter_mut()
            .find(|e| e.module.meta().id.as_str() == id)
            .ok_or_else(|| RegistryError::UnknownMod(id.to_owned()))
    }
}

#[cfg(test)]
mod tests;

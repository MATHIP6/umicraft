// SPDX-License-Identifier: MIT OR Apache-2.0

//! Read-only views for the client's mods menu and controls page.

use semver::Version;
use umicraft_builtin_mods::{HudPlacement, SettingValue};

use super::settings::SettingSpec;
use crate::context::ModContext;
use crate::manifest::{Manifest, ModId, Permissions};

/// A declared setting and its current value.
#[derive(Debug, Clone)]
pub struct SettingView<'a> {
    /// The declaration (key, label, kind, default).
    pub spec: &'a SettingSpec,
    /// The current value, always valid for `spec`.
    pub value: SettingValue,
}

/// A declared HUD widget and where it is.
#[derive(Debug, Clone)]
pub struct WidgetView<'a> {
    /// The widget's key, unique within its mod.
    pub key: &'a str,
    /// Label for the menu and the HUD editor.
    pub label: &'a str,
    /// Where the player put it.
    pub placement: HudPlacement,
    /// Where the mod put it by default, for a "reset" button.
    pub default_placement: HudPlacement,
}

/// A declared key binding, listed on the controls page under its mod's name.
#[derive(Debug, Clone)]
pub struct KeyBinding<'a> {
    /// The mod that declared it.
    pub mod_id: &'a ModId,
    /// The binding's id, unique within its mod.
    pub key: &'a str,
    /// Label for the controls page.
    pub label: &'a str,
    /// The key name the mod suggests (empty for unbound).
    pub default_binding: &'a str,
    /// The key name the player chose (the default until changed).
    pub current: String,
}

/// One loaded external mod, as the mods menu shows it.
#[derive(Debug, Clone)]
pub struct ModView<'a> {
    /// The mod's id.
    pub id: &'a ModId,
    /// Display name from `mod.toml`.
    pub name: &'a str,
    /// One-line description from `mod.toml`, if any.
    pub description: Option<&'a str>,
    /// The mod's own version.
    pub version: &'a Version,
    /// The player's switch (what the menu's toggle shows); the mod only runs when
    /// this is true, `failed` is false and `held_by_compat` is false.
    pub enabled: bool,
    /// Whether the mod failed and stays off until the client restarts.
    pub failed: bool,
    /// What the mod's manifest asks for, for the menu's badges.
    pub permissions: Permissions,
    /// Whether the mod may change packets or movement (`network_packets` or
    /// `movement`); the menu badges it.
    pub alters_game: bool,
    /// Whether server-compat mode holds the mod off, so its switch has no effect.
    pub held_by_compat: bool,
    context: &'a ModContext,
}

impl<'a> ModView<'a> {
    pub(crate) fn new(manifest: &'a Manifest, context: &'a ModContext) -> Self {
        Self {
            id: &manifest.id,
            name: &manifest.name,
            description: manifest.description.as_deref(),
            version: &manifest.version,
            enabled: context.is_wanted(),
            failed: context.is_failed(),
            permissions: context.permissions(),
            alters_game: context.alters_game(),
            held_by_compat: context.is_held_by_compat(),
            context,
        }
    }

    /// The declared settings with their current values, in declaration order.
    pub fn settings(&self) -> impl Iterator<Item = SettingView<'a>> {
        let declared = self.context.ui.declared();
        declared.settings().iter().map(|slot| SettingView {
            spec: slot.spec(),
            value: slot.value(),
        })
    }

    /// The declared HUD widgets with their placements.
    pub fn widgets(&self) -> impl Iterator<Item = WidgetView<'a>> {
        let declared = self.context.ui.declared();
        declared.widgets().iter().map(|widget| WidgetView {
            key: widget.key(),
            label: widget.label(),
            placement: widget.placement(),
            default_placement: widget.default_placement(),
        })
    }

    /// The declared key bindings.
    pub fn keys(&self) -> impl Iterator<Item = KeyBinding<'a>> {
        let mod_id = self.id;
        let declared = self.context.ui.declared();
        declared.keys().iter().map(move |key| KeyBinding {
            mod_id,
            key: key.key(),
            label: key.label(),
            default_binding: key.default_binding(),
            current: key.current(),
        })
    }
}

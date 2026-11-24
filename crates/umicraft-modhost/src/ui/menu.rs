// SPDX-License-Identifier: MIT OR Apache-2.0

//! Categories and modules (API 1.2, ADR-0095): a mod's own tabs and cards in the
//! mods menu.
//!
//! A **category** is a tab a mod creates; it can never be a built-in tab. A
//! **module** is a card of its own with the player's switch, in the tab of its
//! category or under "External". Settings and buttons may sit on a module's card,
//! and a widget or a post-process pass may be attached to a module so it only shows
//! while the module is on.
//!
//! The switch is an [`AtomicBool`]: `module_enabled` reads it from any thread
//! without a lock, like a setting.

use std::sync::atomic::{AtomicBool, Ordering};

use umicraft_modapi_sys::{UMI_ATTACH_HUD_WIDGET, UMI_ATTACH_POST_PASS};

use super::settings::{SettingSpec, SettingSpecKind};
use super::validate::{check_key, check_len, check_text};
use super::{MAX_CATEGORY_BYTES, MAX_DESCRIPTION_BYTES, MAX_LABEL_BYTES};
use crate::context::ModContext;
use crate::error::{ConfigureError, DeclareError};

/// A tab a mod declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CategorySlot {
    key: String,
    label: String,
}

impl CategorySlot {
    /// Validates and builds a category.
    pub(crate) fn new(key: &str, label: &str) -> Result<Self, DeclareError> {
        check_key("category key", key)?;
        check_text("category label", label, MAX_CATEGORY_BYTES)?;
        Ok(Self {
            key: key.to_owned(),
            label: label.to_owned(),
        })
    }

    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }
}

/// A module as a mod declares it, its texts borrowed from the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleDef<'a> {
    /// Key, unique within the mod; the player's switch is saved under it.
    pub key: &'a str,
    /// Name of the card.
    pub label: &'a str,
    /// One line under the name; may be empty.
    pub description: &'a str,
    /// Key of one of the mod's categories, or empty for the "External" tab.
    pub category: &'a str,
    /// State until the player changes it.
    pub default_on: bool,
}

/// A declared module and the player's switch.
#[derive(Debug)]
pub(crate) struct ModuleSlot {
    key: String,
    label: String,
    description: String,
    category: Option<usize>,
    on: AtomicBool,
}

impl ModuleSlot {
    /// Validates the texts of `def` and builds the slot, in category `category`.
    pub(crate) fn new(def: &ModuleDef<'_>, category: Option<usize>) -> Result<Self, DeclareError> {
        check_key("module key", def.key)?;
        check_text("module label", def.label, MAX_LABEL_BYTES)?;
        check_len("module description", def.description, MAX_DESCRIPTION_BYTES)?;
        Ok(Self {
            key: def.key.to_owned(),
            label: def.label.to_owned(),
            description: def.description.to_owned(),
            category,
            on: AtomicBool::new(def.default_on),
        })
    }

    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    pub(crate) fn description(&self) -> &str {
        &self.description
    }

    /// Index of its category among the mod's, `None` for "External".
    pub(crate) fn category(&self) -> Option<usize> {
        self.category
    }

    /// The player's switch. Lock-free.
    pub(crate) fn is_on(&self) -> bool {
        self.on.load(Ordering::Relaxed)
    }

    /// Sets the switch; returns whether it changed.
    pub(crate) fn set_on(&self, on: bool) -> bool {
        self.on.swap(on, Ordering::Relaxed) != on
    }
}

/// What `module_attach` ties to a module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Attach {
    /// A HUD widget, drawn only while the module is on.
    Widget,
    /// A post-process pass, applied only while the module is on.
    PostPass,
}

impl Attach {
    /// The kind for an ABI `UMI_ATTACH_*` value.
    pub(crate) fn from_raw(kind: u32) -> Result<Self, DeclareError> {
        match kind {
            UMI_ATTACH_HUD_WIDGET => Ok(Self::Widget),
            UMI_ATTACH_POST_PASS => Ok(Self::PostPass),
            _ => Err(DeclareError::Invalid {
                what: "module attachment",
                why: "unknown kind",
            }),
        }
    }
}

impl ModContext {
    /// Declares the category `key`, a tab named `label`.
    ///
    /// # Errors
    ///
    /// [`DeclareError::Late`] after init, [`DeclareError::TooMany`] past
    /// [`MAX_CATEGORIES`](super::MAX_CATEGORIES), [`DeclareError::Duplicate`] for a
    /// key already declared, and the key and label checks.
    pub fn register_category(&self, key: &str, label: &str) -> Result<(), DeclareError> {
        self.ui
            .register(|declared| declared.add_category(CategorySlot::new(key, label)?))
    }

    /// Declares a module.
    ///
    /// # Errors
    ///
    /// As [`register_category`](Self::register_category) (limit
    /// [`MAX_MODULES`](super::MAX_MODULES)), plus [`DeclareError::Invalid`] for a
    /// category the mod did not declare.
    pub fn register_module(&self, def: &ModuleDef<'_>) -> Result<(), DeclareError> {
        self.ui.register(|declared| declared.add_module(def))
    }

    /// Declares a setting on the card of `module` (empty: the mod's own card).
    ///
    /// # Errors
    ///
    /// As [`register_setting`](Self::register_setting), plus
    /// [`DeclareError::Invalid`] for a module the mod did not declare.
    pub fn register_setting_in(&self, module: &str, spec: SettingSpec) -> Result<(), DeclareError> {
        self.ui
            .register(|declared| declared.add_setting(spec, module))
    }

    /// Declares a button `key` on the card of `module` (empty: the mod's own card):
    /// `label` on its row, `text` on the button.
    ///
    /// # Errors
    ///
    /// As [`register_setting_in`](Self::register_setting_in); the text is 1 to 32
    /// bytes.
    pub fn register_button(
        &self,
        module: &str,
        key: &str,
        label: &str,
        text: &str,
    ) -> Result<(), DeclareError> {
        let spec = SettingSpec {
            key: key.to_owned(),
            label: label.to_owned(),
            kind: SettingSpecKind::Button {
                text: text.to_owned(),
            },
        };
        self.register_setting_in(module, spec)
    }

    /// Ties the widget or pass `key` to `module`.
    ///
    /// # Errors
    ///
    /// [`DeclareError::Late`] after init, [`DeclareError::Invalid`] for an unknown
    /// module, widget or pass, [`DeclareError::Duplicate`] for something attached
    /// already.
    pub(crate) fn module_attach(
        &self,
        module: &str,
        kind: Attach,
        key: &str,
    ) -> Result<(), DeclareError> {
        self.ui
            .register(|declared| declared.attach(module, kind, key))
    }

    /// The player's switch of `module`, from any thread; `None` for an unknown
    /// module. During init it reads what the mod declared so far.
    pub fn module_enabled(&self, module: &str) -> Option<bool> {
        self.ui.module_on(module)
    }

    /// The key of the module whose card holds button `key`, empty for the mod's own
    /// card.
    ///
    /// # Errors
    ///
    /// [`ConfigureError::UnknownKey`] if the mod declared no such button.
    pub fn button_module(&self, key: &str) -> Result<String, ConfigureError> {
        let declared = self.ui.declared();
        let slot = declared
            .setting(key)
            .filter(|slot| slot.spec().is_button())
            .ok_or_else(|| ConfigureError::UnknownKey {
                kind: "button",
                key: key.to_owned(),
            })?;
        let module = slot
            .module()
            .and_then(|index| declared.modules().get(index))
            .map_or("", |module| module.key());
        Ok(module.to_owned())
    }

    /// Sets the player's switch of `module`; returns whether it changed.
    ///
    /// # Errors
    ///
    /// [`ConfigureError::UnknownKey`] if the mod declared no such module.
    pub fn set_module(&self, module: &str, on: bool) -> Result<bool, ConfigureError> {
        let declared = self.ui.declared();
        let (_, slot) = declared
            .module(module)
            .ok_or_else(|| ConfigureError::UnknownKey {
                kind: "module",
                key: module.to_owned(),
            })?;
        Ok(slot.set_on(on))
    }
}

#[cfg(test)]
mod tests;

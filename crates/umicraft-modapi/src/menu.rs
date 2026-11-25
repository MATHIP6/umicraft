// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tabs, modules and buttons in the mods menu (API 1.2, ADR-0095).
//!
//! A mod lists itself under the "External" tab with one card. It may also create
//! up to two tabs of its own ([`InitCx::category`]) and up to sixteen modules
//! ([`InitCx::module`]): each module is a card with its own switch, in its tab or
//! under "External". [`InitCx::card`] then declares settings and buttons on a
//! module's card and ties widgets and post-process passes to it.
//!
//! ```ignore
//! let pvp = cx.category("pvp", "PvP")?;
//! let radar = cx.module(ModuleSpec::new("radar", "Radar").category(pvp))?;
//! let mut card = cx.card(radar);
//! let range = card.slider("range", "Range", 8.0, 64.0, 8.0, 32.0)?;
//! card.button("clear", "Marks", "Clear")?;
//! card.attach_widget("radar")?;
//! ```

use umicraft_modapi_sys::{
    UmiModuleDef, UmiSettingDef, UmiSettingKind, UMI_ATTACH_HUD_WIDGET, UMI_ATTACH_POST_PASS,
    UMI_ATTACH_WORLD_LAYER, UMI_MODULE_DEFAULT_ON,
};

use crate::host::{self, utf8};
use crate::key::InlineKey;
use crate::post::key_too_long;
use crate::setting::{definition, utf8_all};
use crate::{InitCx, ModError, PostPass, Setting};

/// A tab of the mods menu the mod created with [`InitCx::category`].
///
/// It is shown once a module uses it, after the built-in tabs; tabs of
/// different mods with the same label are one. A mod cannot put a card in a
/// built-in tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Category {
    key: InlineKey,
}

impl Category {
    pub(crate) fn new(key: &str) -> Option<Self> {
        InlineKey::new(key).map(|key| Self { key })
    }

    /// The category's key.
    pub fn key(&self) -> &str {
        self.key.as_str()
    }
}

/// What [`InitCx::module`] declares: a key, a name and, optionally, a
/// description, a category and the default state (on).
#[derive(Debug, Clone, Copy)]
pub struct ModuleSpec<'a> {
    key: &'a str,
    label: &'a str,
    description: &'a str,
    category: Option<Category>,
    default_on: bool,
}

impl<'a> ModuleSpec<'a> {
    /// A module `key` (`^[a-z][a-z0-9_]{0,63}$`, unique within the mod) whose card
    /// is named `label`, under "External", on until the player switches it off.
    pub fn new(key: &'a str, label: &'a str) -> Self {
        Self {
            key,
            label,
            description: "",
            category: None,
            default_on: true,
        }
    }

    /// The line under the card's name (at most 256 bytes).
    #[must_use]
    pub fn description(self, description: &'a str) -> Self {
        Self {
            description,
            ..self
        }
    }

    /// Lists the card in the tab of `category` instead of "External".
    #[must_use]
    pub fn category(self, category: Category) -> Self {
        Self {
            category: Some(category),
            ..self
        }
    }

    /// Whether the module is on until the player changes it.
    #[must_use]
    pub fn default_on(self, default_on: bool) -> Self {
        Self { default_on, ..self }
    }

    pub(crate) fn key(&self) -> &str {
        self.key
    }

    pub(crate) fn default_is_on(&self) -> bool {
        self.default_on
    }

    /// The ABI definition, borrowing the texts (the category's key from `self`) for
    /// one call: `self` must outlive it.
    pub(crate) fn raw(&self) -> UmiModuleDef {
        let category = self.category.as_ref().map_or("", Category::key);
        UmiModuleDef {
            size: u32::try_from(size_of::<UmiModuleDef>()).unwrap_or(u32::MAX),
            flags: if self.default_on {
                UMI_MODULE_DEFAULT_ON
            } else {
                0
            },
            key: utf8(self.key),
            label: utf8(self.label),
            description: utf8(self.description),
            category: utf8(category),
        }
    }
}

/// A module the mod declared with [`InitCx::module`]: a card with the player's
/// switch.
///
/// `Copy`, `Send` and `Sync`: capture it in any handler. The module runs only
/// while the mod runs; check [`is_enabled`](Self::is_enabled) where its feature
/// acts, or attach its widgets and passes with [`Card::attach_widget`] and
/// [`Card::attach_post_pass`] so the host hides them. A change of the switch
/// arrives as [`ModuleToggled`](crate::ModuleToggled).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Module {
    key: InlineKey,
    default_on: bool,
}

impl Module {
    pub(crate) fn new(key: &str, default_on: bool) -> Option<Self> {
        InlineKey::new(key).map(|key| Self { key, default_on })
    }

    /// The module's key, as [`ModuleToggled::module`](crate::ModuleToggled::module)
    /// and [`ButtonPressed::module`](crate::ButtonPressed::module) give it.
    pub fn key(&self) -> &str {
        self.key.as_str()
    }

    /// The player's switch, from any thread, without a lock.
    ///
    /// Never fails: if the host cannot answer (before `init`), the default given
    /// at registration is returned.
    pub fn is_enabled(&self) -> bool {
        let mut on = 0;
        let asked =
            host::call(|host, me| (host.module_enabled)(me, utf8(self.key()), &raw mut on).is_ok());
        if asked == Ok(true) {
            on != 0
        } else {
            self.default_on
        }
    }
}

/// Declares settings and buttons on one card: the mod's own
/// ([`InitCx::toggle`] and friends use it) or a module's ([`InitCx::card`]).
///
/// Setting and button keys are unique across the whole mod, whichever card
/// shows them.
#[derive(Debug)]
pub struct Card<'c, 'a> {
    cx: &'c mut InitCx<'a>,
    module: Option<Module>,
}

impl<'c, 'a> Card<'c, 'a> {
    pub(crate) fn new(cx: &'c mut InitCx<'a>, module: Option<Module>) -> Self {
        Self { cx, module }
    }

    fn module_key(&self) -> &str {
        self.module.as_ref().map_or("", Module::key)
    }

    /// Declares an on/off setting on this card.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] for an invalid or duplicate `key`.
    pub fn toggle(
        &mut self,
        key: &'static str,
        label: &str,
        default: bool,
    ) -> Result<Setting<bool>, ModError> {
        let def = UmiSettingDef {
            toggle_default: u8::from(default),
            ..definition(UmiSettingKind::TOGGLE, key, label)
        };
        self.register(&def)?;
        Ok(Setting::new(key, def.kind, |v| v.toggle != 0, default))
    }

    /// Declares a number in `[min, max]` moved by `step` on this card.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] for an invalid or duplicate `key`, a
    /// non-positive `step` or a `default` outside the range.
    pub fn slider(
        &mut self,
        key: &'static str,
        label: &str,
        min: f32,
        max: f32,
        step: f32,
        default: f32,
    ) -> Result<Setting<f32>, ModError> {
        let def = UmiSettingDef {
            slider_min: min,
            slider_max: max,
            slider_step: step,
            slider_default: default,
            ..definition(UmiSettingKind::SLIDER, key, label)
        };
        self.register(&def)?;
        Ok(Setting::new(key, def.kind, |v| v.slider, default))
    }

    /// Declares a choice among `options` on this card; the value is the
    /// selected index.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] for an invalid or duplicate `key`, no
    /// options or a `default_index` past the last one.
    pub fn choice(
        &mut self,
        key: &'static str,
        label: &str,
        options: &[&str],
        default_index: u32,
    ) -> Result<Setting<u32>, ModError> {
        let labels = utf8_all(options);
        let def = UmiSettingDef {
            choices: labels.as_ptr(),
            choices_len: labels.len(),
            choice_default: default_index,
            ..definition(UmiSettingKind::CHOICE, key, label)
        };
        self.register(&def)?;
        Ok(Setting::new(key, def.kind, |v| v.choice, default_index))
    }

    /// Declares a colour, `0xRRGGBB`, on this card.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] for an invalid or duplicate `key`.
    pub fn color(
        &mut self,
        key: &'static str,
        label: &str,
        default: u32,
    ) -> Result<Setting<u32>, ModError> {
        let def = UmiSettingDef {
            color_default: default,
            ..definition(UmiSettingKind::COLOR, key, label)
        };
        self.register(&def)?;
        Ok(Setting::new(key, def.kind, |v| v.color, default))
    }

    /// Declares a button on this card: `label` on its row, `text` (1 to 32
    /// bytes) on the button. A click arrives as
    /// [`ButtonPressed`](crate::ButtonPressed), on the render thread. A button
    /// holds no value and is not saved.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] for an invalid or duplicate `key` (buttons
    /// share the settings' keys) or an empty or over-long `text`.
    pub fn button(&mut self, key: &str, label: &str, text: &str) -> Result<(), ModError> {
        let status = (self.cx.host().register_button)(
            self.cx.me(),
            utf8(self.module_key()),
            utf8(key),
            utf8(label),
            utf8(text),
        );
        ModError::check(status)
    }

    /// Shows the HUD widget `key` (declared before with
    /// [`InitCx::hud_widget`]) only while this card's module is on; the HUD
    /// editor hides it too while off.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] with `INVALID_ARGUMENT` on the mod's own card
    /// or for an unknown widget, `CONFLICT` for a widget attached already.
    pub fn attach_widget(&mut self, key: &str) -> Result<(), ModError> {
        self.attach(UMI_ATTACH_HUD_WIDGET, key)
    }

    /// Applies `pass` only while this card's module is on.
    ///
    /// # Errors
    /// As [`attach_widget`](Self::attach_widget).
    pub fn attach_post_pass(&mut self, pass: &PostPass) -> Result<(), ModError> {
        self.attach(UMI_ATTACH_POST_PASS, pass.key())
    }

    /// Draws the world layer `key` only while this card's module is on (API 1.4).
    ///
    /// # Errors
    /// As [`attach_widget`](Self::attach_widget).
    pub fn attach_world_layer(&mut self, key: &str) -> Result<(), ModError> {
        self.attach(UMI_ATTACH_WORLD_LAYER, key)
    }

    fn attach(&mut self, kind: u32, key: &str) -> Result<(), ModError> {
        let status =
            (self.cx.host().module_attach)(self.cx.me(), utf8(self.module_key()), kind, utf8(key));
        ModError::check(status)
    }

    fn register(&mut self, def: &UmiSettingDef) -> Result<(), ModError> {
        let (host, me) = (self.cx.host(), self.cx.me());
        let status = match &self.module {
            None => (host.register_setting)(me, def),
            Some(module) => (host.register_setting_in)(me, utf8(module.key()), def),
        };
        ModError::check(status)
    }
}

impl<'a> InitCx<'a> {
    /// Creates a tab of the mods menu named `label` (at most 32 bytes); at most
    /// two per mod. Its cards are the modules declared with
    /// [`ModuleSpec::category`].
    ///
    /// # Errors
    /// [`ModError::HostRefused`]: `INVALID_ARGUMENT` for an invalid key or
    /// label, `DENIED` past two, `CONFLICT` for a key used already.
    pub fn category(&mut self, key: &str, label: &str) -> Result<Category, ModError> {
        let category = Category::new(key).ok_or_else(key_too_long)?;
        ModError::check((self.host().register_category)(
            self.me(),
            utf8(key),
            utf8(label),
        ))?;
        Ok(category)
    }

    /// Declares a module: a card of its own with a switch; at most sixteen per
    /// mod. Declare its settings and buttons with [`card`](Self::card).
    ///
    /// # Errors
    /// [`ModError::HostRefused`]: `INVALID_ARGUMENT` for an invalid key or text
    /// or a category of another mod, `DENIED` past sixteen, `CONFLICT` for a
    /// key used already.
    pub fn module(&mut self, spec: ModuleSpec<'_>) -> Result<Module, ModError> {
        let module = Module::new(spec.key(), spec.default_is_on()).ok_or_else(key_too_long)?;
        let raw = spec.raw();
        ModError::check((self.host().register_module)(self.me(), &raw const raw))?;
        Ok(module)
    }

    /// The card of `module`, to declare settings and buttons on it and to tie
    /// widgets and passes to it.
    pub fn card(&mut self, module: Module) -> Card<'_, 'a> {
        Card::new(self, Some(module))
    }

    /// Declares a button on the mod's own card; see [`Card::button`].
    ///
    /// # Errors
    /// As [`Card::button`].
    pub fn button(&mut self, key: &str, label: &str, text: &str) -> Result<(), ModError> {
        Card::new(self, None).button(key, label, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spec_builds_the_abi_definition() {
        let pvp = Category::new("pvp").unwrap();
        let spec = ModuleSpec::new("radar", "Radar")
            .description("Players around")
            .category(pvp)
            .default_on(false);
        let raw = spec.raw();
        assert_eq!(raw.size as usize, size_of::<UmiModuleDef>());
        assert_eq!(raw.flags, 0);
        assert_eq!((raw.key.len, raw.category.len), (5, 3));
        assert_eq!(raw.description.len, "Players around".len());
        assert_eq!(ModuleSpec::new("a", "A").raw().flags, UMI_MODULE_DEFAULT_ON);
        assert_eq!(ModuleSpec::new("a", "A").raw().category.len, 0);
    }

    /// This test binary never runs `init`, so there is no host: the default
    /// answers.
    #[test]
    fn a_module_without_a_host_reports_its_default() {
        assert!(Module::new("radar", true).unwrap().is_enabled());
        assert!(!Module::new("radar", false).unwrap().is_enabled());
        assert!(Module::new(&"a".repeat(65), true).is_none());
        assert_eq!(
            Category::new("pvp").map(|c| c.key().to_owned()),
            Some("pvp".into())
        );
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The lists of what one mod declared, and the checks that guard them.

use super::hud::WidgetSlot;
use super::keys::KeySlot;
use super::settings::{SettingSlot, SettingSpec};
use super::validate::check_room;
use super::{MAX_COMMANDS, MAX_KEYS, MAX_SETTINGS, MAX_WIDGETS};
use crate::error::DeclareError;
use crate::post_pass::{PostPassSlot, MAX_POST_PASSES};

/// Everything a mod declared: settings, widgets, key bindings and command names.
///
/// Built while `umi_mod_init` runs, then frozen: after that only the atomics and
/// mutexes inside the slots change, never the lists themselves.
#[derive(Debug, Default)]
pub(crate) struct Declared {
    settings: Vec<SettingSlot>,
    widgets: Vec<WidgetSlot>,
    keys: Vec<KeySlot>,
    commands: Vec<String>,
    post_passes: Vec<PostPassSlot>,
}

fn duplicate(what: &'static str, key: &str) -> DeclareError {
    DeclareError::Duplicate {
        what,
        value: key.to_owned(),
    }
}

impl Declared {
    /// An empty declaration, usable in a `static`.
    pub(crate) const fn new() -> Self {
        Self {
            settings: Vec::new(),
            widgets: Vec::new(),
            keys: Vec::new(),
            commands: Vec::new(),
            post_passes: Vec::new(),
        }
    }

    /// Adds a validated setting.
    pub(crate) fn add_setting(&mut self, spec: SettingSpec) -> Result<(), DeclareError> {
        spec.validate()?;
        check_room("settings", self.settings.len(), MAX_SETTINGS)?;
        if self.setting(&spec.key).is_some() {
            return Err(duplicate("setting key", &spec.key));
        }
        self.settings.push(SettingSlot::new(spec));
        Ok(())
    }

    /// Adds a widget.
    pub(crate) fn add_widget(&mut self, widget: WidgetSlot) -> Result<(), DeclareError> {
        check_room("widgets", self.widgets.len(), MAX_WIDGETS)?;
        if self.widget(widget.key()).is_some() {
            return Err(duplicate("widget key", widget.key()));
        }
        self.widgets.push(widget);
        Ok(())
    }

    /// Adds a post-process pass.
    pub(crate) fn add_post_pass(&mut self, pass: PostPassSlot) -> Result<(), DeclareError> {
        check_room("post passes", self.post_passes.len(), MAX_POST_PASSES)?;
        if self.post_pass(pass.key()).is_some() {
            return Err(duplicate("post pass key", pass.key()));
        }
        self.post_passes.push(pass);
        Ok(())
    }

    /// Adds a key binding.
    pub(crate) fn add_key(&mut self, key: KeySlot) -> Result<(), DeclareError> {
        check_room("key bindings", self.keys.len(), MAX_KEYS)?;
        if self.key(key.key()).is_some() {
            return Err(duplicate("key binding id", key.key()));
        }
        self.keys.push(key);
        Ok(())
    }

    /// Whether another command may be declared.
    pub(crate) fn check_command_room(&self) -> Result<(), DeclareError> {
        check_room("commands", self.commands.len(), MAX_COMMANDS)
    }

    /// Remembers a command name once the shared table accepted it, so it can be
    /// withdrawn if `umi_mod_init` fails.
    pub(crate) fn record_command(&mut self, name: &str) {
        self.commands.push(name.to_owned());
    }

    /// The names of the commands this mod put in the shared table.
    pub(crate) fn commands(&self) -> &[String] {
        &self.commands
    }

    pub(crate) fn settings(&self) -> &[SettingSlot] {
        &self.settings
    }

    pub(crate) fn widgets(&self) -> &[WidgetSlot] {
        &self.widgets
    }

    pub(crate) fn post_passes(&self) -> &[PostPassSlot] {
        &self.post_passes
    }

    pub(crate) fn post_pass(&self, key: &str) -> Option<&PostPassSlot> {
        self.post_passes.iter().find(|p| p.key() == key)
    }

    pub(crate) fn keys(&self) -> &[KeySlot] {
        &self.keys
    }

    pub(crate) fn setting(&self, key: &str) -> Option<&SettingSlot> {
        self.settings.iter().find(|s| s.spec().key == key)
    }

    pub(crate) fn widget(&self, key: &str) -> Option<&WidgetSlot> {
        self.widgets.iter().find(|w| w.key() == key)
    }

    pub(crate) fn key(&self, key: &str) -> Option<&KeySlot> {
        self.keys.iter().find(|k| k.key() == key)
    }
}

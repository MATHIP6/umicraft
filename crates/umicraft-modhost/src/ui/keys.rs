// SPDX-License-Identifier: MIT OR Apache-2.0

//! Key bindings a mod declares.

use std::sync::{Mutex, PoisonError};

use super::validate::{check_key, check_len, check_text};
use super::{MAX_BINDING_BYTES, MAX_LABEL_BYTES};
use crate::error::DeclareError;

/// A key binding: its declaration and the name the player bound to it.
///
/// Binding names are the client's key names (`KeyG`, `Mouse4`); the host does not
/// interpret them, it only stores them (bounded) for the controls page.
#[derive(Debug)]
pub(crate) struct KeySlot {
    key: String,
    label: String,
    default: String,
    current: Mutex<String>,
}

impl KeySlot {
    /// Validates and builds a binding; `default` may be empty (unbound).
    pub(crate) fn new(key: &str, label: &str, default: &str) -> Result<Self, DeclareError> {
        check_key("key binding id", key)?;
        check_text("key binding label", label, MAX_LABEL_BYTES)?;
        check_len("default key name", default, MAX_BINDING_BYTES)?;
        Ok(Self {
            key: key.to_owned(),
            label: label.to_owned(),
            default: default.to_owned(),
            current: Mutex::new(default.to_owned()),
        })
    }

    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    pub(crate) fn default_binding(&self) -> &str {
        &self.default
    }

    /// The name the player bound, or the default.
    pub(crate) fn current(&self) -> String {
        self.current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Binds `name`; a name over the limit (a hand-edited config) restores the default.
    pub(crate) fn set(&self, name: &str) {
        let name = if name.len() <= MAX_BINDING_BYTES {
            name
        } else {
            &self.default
        };
        name.clone_into(&mut self.current.lock().unwrap_or_else(PoisonError::into_inner));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_binding_starts_at_its_default_and_can_be_rebound() {
        let slot = KeySlot::new("toggle", "Toggle", "KeyG").unwrap();
        assert_eq!(slot.current(), "KeyG");
        slot.set("Mouse4");
        assert_eq!(slot.current(), "Mouse4");
        slot.set("");
        assert_eq!(slot.current(), "");
        slot.set(&"x".repeat(MAX_BINDING_BYTES + 1));
        assert_eq!(slot.current(), "KeyG");
    }

    #[test]
    fn invalid_declarations_are_refused() {
        assert!(KeySlot::new("Bad", "Label", "").is_err());
        assert!(KeySlot::new("ok", "", "").is_err());
        assert!(KeySlot::new("ok", "Label", &"x".repeat(MAX_BINDING_BYTES + 1)).is_err());
        assert!(KeySlot::new("ok", "Label", "").is_ok());
    }
}

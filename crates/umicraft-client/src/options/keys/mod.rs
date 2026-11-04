// SPDX-License-Identifier: MIT OR Apache-2.0

//! The key bindings: which key triggers which [`Action`].

use std::collections::BTreeMap;

use winit::keyboard::KeyCode;

mod action;
mod names;
#[cfg(test)]
mod tests;

pub use action::Action;
pub use names::{is_reserved, key_from_name, key_label, key_name};

/// The key bound to every [`Action`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBindings {
    keys: [KeyCode; Action::ALL.len()],
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            keys: Action::ALL.map(Action::default_key),
        }
    }
}

impl KeyBindings {
    /// The key bound to `action`.
    #[must_use]
    pub fn get(&self, action: Action) -> KeyCode {
        self.keys[action.index()]
    }

    /// Binds `key` to `action`. A reserved or unbindable key is refused and
    /// `false` is returned; two actions may share a key, the controls screen
    /// flags it (see [`Self::conflicts`]).
    pub fn set(&mut self, action: Action, key: KeyCode) -> bool {
        if is_reserved(key) || key_name(key).is_none() {
            return false;
        }
        self.keys[action.index()] = key;
        true
    }

    /// Restores every default key.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The other actions that use the same key as `action`.
    #[must_use]
    pub fn conflicts(&self, action: Action) -> Vec<Action> {
        let key = self.get(action);
        Action::ALL
            .iter()
            .copied()
            .filter(|other| *other != action && self.get(*other) == key)
            .collect()
    }

    /// The first action bound to `key`.
    #[must_use]
    pub fn action_for(&self, key: KeyCode) -> Option<Action> {
        Action::ALL
            .iter()
            .copied()
            .find(|action| self.get(*action) == key)
    }

    /// Every action bound to `key`, in listing order.
    pub fn actions_for(&self, key: KeyCode) -> impl Iterator<Item = Action> + '_ {
        Action::ALL
            .iter()
            .copied()
            .filter(move |action| self.get(*action) == key)
    }

    /// The `[keys]` section: action id to key name.
    pub(super) fn to_map(&self) -> BTreeMap<String, String> {
        Action::ALL
            .iter()
            .filter_map(|action| {
                key_name(self.get(*action)).map(|name| (action.id().to_string(), name.to_string()))
            })
            .collect()
    }

    /// Reads the `[keys]` section. An action that is missing, or whose key is
    /// unknown or reserved, keeps its default.
    pub(super) fn from_map(map: &BTreeMap<String, String>) -> Self {
        let mut bindings = Self::default();
        for action in Action::ALL {
            if let Some(key) = map.get(action.id()).and_then(|name| key_from_name(name)) {
                bindings.set(action, key);
            }
        }
        bindings
    }
}

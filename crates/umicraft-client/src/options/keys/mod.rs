// SPDX-License-Identifier: MIT OR Apache-2.0

//! The key bindings: which key or mouse button triggers which [`Action`].

use std::collections::BTreeMap;

mod action;
mod binding;
mod names;
#[cfg(test)]
mod tests;

pub use action::Action;
pub use binding::Binding;
pub use names::{is_reserved, key_from_name, key_label, key_name};

/// The [`Binding`] of every [`Action`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBindings {
    bindings: [Binding; Action::ALL.len()],
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            bindings: Action::ALL.map(Action::default_binding),
        }
    }
}

impl KeyBindings {
    /// The binding of `action`.
    #[must_use]
    pub fn get(&self, action: Action) -> Binding {
        self.bindings[action.index()]
    }

    /// Binds `binding` to `action`. A reserved or unbindable key is refused
    /// and `false` is returned; two actions may share a binding, the
    /// controls screen flags it (see [`Self::conflicts`]).
    pub fn set(&mut self, action: Action, binding: impl Into<Binding>) -> bool {
        let binding = binding.into();
        if !binding.is_bindable() {
            return false;
        }
        self.bindings[action.index()] = binding;
        true
    }

    /// Restores every default binding.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Whether every action has its default binding.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The other actions that use the same binding as `action`.
    #[must_use]
    pub fn conflicts(&self, action: Action) -> Vec<Action> {
        let binding = self.get(action);
        self.actions_for(binding)
            .filter(|other| *other != action)
            .collect()
    }

    /// The first action bound to `binding`.
    #[must_use]
    pub fn action_for(&self, binding: impl Into<Binding>) -> Option<Action> {
        self.actions_for(binding).next()
    }

    /// Every action bound to `binding`, in listing order.
    pub fn actions_for(&self, binding: impl Into<Binding>) -> impl Iterator<Item = Action> + '_ {
        let binding = binding.into();
        Action::ALL
            .iter()
            .copied()
            .filter(move |action| self.get(*action) == binding)
    }

    /// The `[keys]` section: action id to key or `MouseN` name.
    pub(in crate::options) fn to_map(&self) -> BTreeMap<String, String> {
        Action::ALL
            .iter()
            .filter_map(|action| {
                self.get(*action)
                    .name()
                    .map(|name| (action.id().to_string(), name))
            })
            .collect()
    }

    /// Reads the `[keys]` section. An action that is missing, or whose key
    /// is unknown or reserved, keeps its default, so a file written before
    /// mouse bindings existed loads unchanged.
    pub(in crate::options) fn from_map(map: &BTreeMap<String, String>) -> Self {
        let mut bindings = Self::default();
        for action in Action::ALL {
            if let Some(binding) = map
                .get(action.id())
                .and_then(|name| Binding::from_name(name))
            {
                bindings.set(action, binding);
            }
        }
        bindings
    }
}

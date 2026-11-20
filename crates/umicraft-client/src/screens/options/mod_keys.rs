// SPDX-License-Identifier: MIT OR Apache-2.0

//! The key bindings of external mods on the controls page (F3.4).
//!
//! The page does not know the mod host: the window hands it the bindings of the
//! enabled mods as plain [`ModKeyGroup`]s, and collects what the player changed as
//! [`ModKeyChange`]s, which it writes through to the host and saves. A binding is
//! stored by the same names as the built-in ones (`KeyG`, `Mouse4`), so one
//! rebinding path serves both.

use super::controls_page::KeyEntry;
use super::OptionsScreen;
use crate::options::Binding;

/// One key binding a mod declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModKey {
    /// The mod that declared it.
    pub mod_id: String,
    /// The binding's id within its mod.
    pub key: String,
    /// The label the mod gave it (literal text).
    pub label: String,
    /// The key name the mod suggests; empty for unbound.
    pub default: String,
    /// The key name the player chose; empty for unbound.
    pub current: String,
}

impl ModKey {
    /// Whether the player moved it off the mod's suggestion.
    pub fn is_changed(&self) -> bool {
        self.current != self.default
    }

    /// The binding it currently has, `None` for unbound or an unknown name.
    pub fn binding(&self) -> Option<Binding> {
        Binding::from_name(&self.current)
    }

    /// The change that puts it back to the mod's suggestion.
    pub fn reset(&self) -> ModKeyChange {
        ModKeyChange {
            mod_id: self.mod_id.clone(),
            key: self.key.clone(),
            binding: self.default.clone(),
        }
    }
}

/// The bindings of one mod, listed under a heading named after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModKeyGroup {
    /// The mod's display name.
    pub name: String,
    /// Its bindings.
    pub keys: Vec<ModKey>,
}

/// A binding the player changed on the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModKeyChange {
    /// The mod.
    pub mod_id: String,
    /// The binding's id.
    pub key: String,
    /// The new key name; empty for unbound.
    pub binding: String,
}

/// The rows of `groups`: a heading per mod, then its bindings.
pub(super) fn mod_entries(groups: &[ModKeyGroup]) -> impl Iterator<Item = KeyEntry<'_>> {
    groups.iter().flat_map(|group| {
        std::iter::once(KeyEntry::ModCategory(&group.name))
            .chain(group.keys.iter().map(KeyEntry::ModKey))
    })
}

/// A binding waiting for its key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ModCapture {
    mod_id: String,
    key: String,
}

impl OptionsScreen {
    /// Gives the controls page the bindings of the enabled mods. The window sets them
    /// each frame the page is open, so they follow the mods' switches.
    pub fn set_mod_keys(&mut self, groups: Vec<ModKeyGroup>) {
        self.mod_keys = groups;
    }

    /// The changes the player made since the last call, for the window to apply.
    pub fn take_mod_key_changes(&mut self) -> Vec<ModKeyChange> {
        std::mem::take(&mut self.mod_key_changes)
    }

    /// Whether any binding, built-in or a mod's, waits for its key.
    #[must_use]
    pub fn is_capturing(&self) -> bool {
        self.capturing.is_some() || self.mod_capture.is_some()
    }

    /// Starts waiting for a key for the binding `key` of `mod_id`.
    pub(super) fn start_mod_capture(&mut self, key: &ModKey) {
        self.capturing = None;
        self.mod_capture = Some(ModCapture {
            mod_id: key.mod_id.clone(),
            key: key.key.clone(),
        });
    }

    /// Binds `binding` to the mod binding that waits for one. A key the client keeps
    /// for itself is refused and the capture goes on. Returns whether it was bound.
    pub(super) fn capture_mod(&mut self, binding: Binding) -> bool {
        let Some(name) = binding.name().filter(|_| binding.is_bindable()) else {
            return false;
        };
        let Some(ModCapture { mod_id, key }) = self.mod_capture.take() else {
            return false;
        };
        self.mod_key_changes.push(ModKeyChange {
            mod_id,
            key,
            binding: name,
        });
        true
    }

    /// Whether `key` is the binding waiting for its key.
    pub(super) fn is_mod_capture_of(&self, key: &ModKey) -> bool {
        self.mod_capture
            .as_ref()
            .is_some_and(|waiting| waiting.mod_id == key.mod_id && waiting.key == key.key)
    }
}

#[cfg(test)]
mod tests {
    use winit::keyboard::KeyCode;

    use super::*;

    fn key(current: &str) -> ModKey {
        ModKey {
            mod_id: "hello".into(),
            key: "toggle".into(),
            label: "Toggle".into(),
            default: "KeyG".into(),
            current: current.into(),
        }
    }

    #[test]
    fn a_binding_is_changed_when_it_differs_from_the_suggestion() {
        assert!(!key("KeyG").is_changed());
        assert!(key("KeyH").is_changed());
        assert!(key("").is_changed(), "unbound is a change too");
        assert_eq!(key("KeyH").reset().binding, "KeyG");
    }

    #[test]
    fn the_stored_name_is_the_one_of_the_builtin_bindings() {
        assert_eq!(key("KeyH").binding(), Some(Binding::Key(KeyCode::KeyH)));
        assert_eq!(key("Mouse4").binding(), Some(Binding::Mouse(3)));
        assert_eq!(key("").binding(), None);
    }

    #[test]
    fn the_rows_are_a_heading_then_the_bindings_of_each_mod() {
        let groups = vec![ModKeyGroup {
            name: "Hello".into(),
            keys: vec![key("KeyG")],
        }];
        let entries: Vec<_> = mod_entries(&groups).collect();
        assert_eq!(entries.len(), 2);
        assert!(matches!(entries[0], KeyEntry::ModCategory("Hello")));
    }

    #[test]
    fn a_captured_key_becomes_a_change_and_a_reserved_one_is_refused() {
        let mut screen = OptionsScreen::new();
        screen.start_mod_capture(&key("KeyG"));
        assert!(screen.is_capturing());
        assert!(!screen.capture_mod(Binding::Key(KeyCode::Escape)));
        assert!(screen.is_capturing(), "the capture goes on");
        assert!(screen.capture_mod(Binding::Mouse(3)));
        assert!(!screen.is_capturing());
        assert_eq!(
            screen.take_mod_key_changes(),
            [ModKeyChange {
                mod_id: "hello".into(),
                key: "toggle".into(),
                binding: "Mouse4".into()
            }]
        );
        assert!(screen.take_mod_key_changes().is_empty());
    }
}

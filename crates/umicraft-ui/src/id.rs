// SPDX-License-Identifier: MIT OR Apache-2.0

//! Stable widget identifiers.
//!
//! Immediate-mode UI has no retained widget objects, so a widget's identity is
//! derived from its position in the ID stack and from a label. The derivation
//! uses FNV-1a: it is deterministic across runs and does not depend on the
//! standard library's `DefaultHasher`, which is explicitly unspecified.

/// FNV-1a 64-bit offset basis.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64-bit prime.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The identifier of one widget in the current frame.
///
/// IDs are stable while the widget tree and its labels are stable. Two calls
/// to [`WidgetId::from_label`] with the same string return the same ID; a
/// child ID also depends on its parent's raw value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct WidgetId(u64);

impl WidgetId {
    /// Wraps a raw value, for tests and for integration with external input.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw hash value.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Derives an ID from a label and the root stack.
    #[must_use]
    pub fn from_label(label: &str) -> Self {
        Self::from_raw(fnv1a(FNV_OFFSET, label.as_bytes()))
    }

    /// Derives a child ID from this ID and a label.
    #[must_use]
    pub fn child(self, label: &str) -> Self {
        let mut hash = fnv1a(FNV_OFFSET, &self.0.to_le_bytes());
        hash = fnv1a(hash, label.as_bytes());
        Self::from_raw(hash)
    }
}

/// Hashes `bytes` into `hash` with FNV-1a.
fn fnv1a(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::WidgetId;

    #[test]
    fn labels_are_stable_and_distinct() {
        assert_eq!(
            WidgetId::from_label("button"),
            WidgetId::from_label("button")
        );
        assert_ne!(
            WidgetId::from_label("button"),
            WidgetId::from_label("button ")
        );
    }

    #[test]
    fn children_depend_on_the_parent() {
        let root = WidgetId::from_label("root");
        let other = WidgetId::from_label("other");
        assert_ne!(root.child("item"), other.child("item"));
        assert_eq!(root.child("item"), root.child("item"));
    }

    #[test]
    fn raw_round_trips() {
        let id = WidgetId::from_raw(42);
        assert_eq!(id.raw(), 42);
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! A key stored inline in a `Copy` handle.

/// The longest key the host accepts (`^[a-z][a-z0-9_]{0,63}$`).
pub(crate) const MAX_KEY_LEN: usize = 64;

/// A key copied into a fixed array, so a handle that names something the mod
/// registered ([`PostPass`](crate::PostPass), [`Module`](crate::Module),
/// [`Category`](crate::Category)) stays `Copy + Send + Sync` without borrowing or
/// leaking a string: a runtime key has no `'static` lifetime, and a leak would grow
/// on every mod reload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InlineKey {
    bytes: [u8; MAX_KEY_LEN],
    len: usize,
}

impl InlineKey {
    /// Copies `key`, or `None` when it is too long to be a valid key.
    pub(crate) fn new(key: &str) -> Option<Self> {
        let source = key.as_bytes();
        let mut bytes = [0; MAX_KEY_LEN];
        bytes.get_mut(..source.len())?.copy_from_slice(source);
        Some(Self {
            bytes,
            len: source.len(),
        })
    }

    /// The key.
    pub(crate) fn as_str(&self) -> &str {
        // The bytes were copied from a `&str` in `new`, whole.
        std::str::from_utf8(&self.bytes[..self.len]).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_round_trips_up_to_the_limit() {
        assert_eq!(
            InlineKey::new("sepia").map(|k| k.as_str().to_owned()),
            Some("sepia".to_owned())
        );
        let longest = "a".repeat(MAX_KEY_LEN);
        assert!(InlineKey::new(&longest).is_some());
        assert!(InlineKey::new(&longest.repeat(2)).is_none());
        assert_eq!(InlineKey::new("").map(|k| k.as_str().len()), Some(0));
    }
}

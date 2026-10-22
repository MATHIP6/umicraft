// SPDX-License-Identifier: MIT OR Apache-2.0

//! Deterministic mapping from texture keys to texture-array layers.
//!
//! The atlas builder and the mesher must agree on layer indices. Both use
//! [`AtlasLayout`], which sorts keys and reserves layer zero for a procedural
//! missing-texture checker. The missing layer is original Umicraft art.

use std::collections::HashMap;

/// Texture key used for missing or unknown block textures.
pub const MISSING_TEXTURE: &str = "umicraft/missing";

/// Texture-key to layer mapping shared by the atlas builder and the mesher.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AtlasLayout {
    keys: Vec<String>,
    index: HashMap<String, u32>,
}

impl AtlasLayout {
    /// Builds a layout from `keys`.
    ///
    /// The result is independent of input order: [`MISSING_TEXTURE`] is layer
    /// zero and every other key follows in byte-wise sorted order. Duplicate
    /// keys are ignored.
    #[must_use]
    pub fn from_keys<I, S>(keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut unique: Vec<String> = keys.into_iter().map(Into::into).collect();
        unique.retain(|key| key != MISSING_TEXTURE);
        unique.sort_unstable();
        unique.dedup();

        let mut all = Vec::with_capacity(unique.len() + 1);
        all.push(MISSING_TEXTURE.to_string());
        all.extend(unique);

        let index = all
            .iter()
            .enumerate()
            .map(|(layer, key)| (key.clone(), layer as u32))
            .collect();
        Self { keys: all, index }
    }

    /// Layer for `key`, or `None` when the atlas does not contain it.
    #[must_use]
    pub fn layer(&self, key: &str) -> Option<u32> {
        self.index.get(key).copied()
    }

    /// Ordered texture keys; index `n` is layer `n`.
    #[must_use]
    pub fn keys(&self) -> &[String] {
        &self.keys
    }

    /// Number of layers including the missing layer.
    #[must_use]
    pub fn len(&self) -> u32 {
        self.keys.len() as u32
    }

    /// True when the layout has no layers (only possible for a default value).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_texture_is_layer_zero_and_keys_are_sorted() {
        let layout = AtlasLayout::from_keys(["blocks/stone", "blocks/dirt", "blocks/stone"]);
        assert_eq!(layout.layer(MISSING_TEXTURE), Some(0));
        assert_eq!(layout.layer("blocks/dirt"), Some(1));
        assert_eq!(layout.layer("blocks/stone"), Some(2));
        assert_eq!(layout.len(), 3);
        assert_eq!(layout.keys()[2], "blocks/stone");
    }

    #[test]
    fn an_unknown_texture_has_no_layer() {
        let layout = AtlasLayout::from_keys(["blocks/stone"]);
        assert_eq!(layout.layer("blocks/nope"), None);
    }
}

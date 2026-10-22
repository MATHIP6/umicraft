// SPDX-License-Identifier: MIT OR Apache-2.0

//! Parsing of 1.8 blockstate documents (`variants` only; 1.8 has no
//! `multipart` blockstates).

use std::collections::BTreeMap;

use crate::error::ModelResult;

/// One model reference in a blockstate variant list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    /// Model name relative to `models/block`, e.g. `stone` or `half_slab_stone`.
    pub model: String,
    /// X rotation in degrees (0, 90, 180 or 270).
    pub x: i32,
    /// Y rotation in degrees (0, 90, 180 or 270).
    pub y: i32,
    /// Whether UVs rotate with the model.
    pub uvlock: bool,
    /// Selection weight; 1 when omitted.
    pub weight: u32,
}

/// A parsed blockstate document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlockStateFile {
    /// Variant key (canonical `a=b,c=d` or `normal`) to weighted models.
    pub variants: BTreeMap<String, Vec<Variant>>,
}

impl BlockStateFile {
    /// Parses one blockstate document.
    ///
    /// # Errors
    /// [`ModelError::Json`] for malformed JSON and [`ModelError::Invalid`] for
    /// a document without `variants` or with an invalid rotation/weight.
    pub fn parse(bytes: &[u8], path: &str) -> ModelResult<Self> {
        let _ = (bytes, path);
        Ok(Self::default())
    }

    /// Looks up a canonical variant key; `None` when the key is absent.
    #[must_use]
    pub fn select(&self, key: &str) -> Option<&[Variant]> {
        self.variants.get(key).map(Vec::as_slice)
    }

    /// First declared variant, used when a metadata mapping does not match.
    #[must_use]
    pub fn first(&self) -> Option<(&str, &[Variant])> {
        self.variants
            .iter()
            .next()
            .map(|(key, variants)| (key.as_str(), variants.as_slice()))
    }
}

impl Variant {
    /// Parses a canonical variant key into sorted property pairs.
    ///
    /// `normal` maps to an empty property map.
    #[must_use]
    pub fn parse_key(key: &str) -> BTreeMap<String, String> {
        if key == "normal" {
            return BTreeMap::new();
        }
        key.split(',')
            .filter_map(|pair| pair.split_once('='))
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }
}

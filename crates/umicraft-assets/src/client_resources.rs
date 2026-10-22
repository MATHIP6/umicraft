// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bulk read of the blockstate, model and texture files of a verified client
//! jar.
//!
//! The jar is read once into a virtual resource map; the model parser and the
//! atlas builder then work purely in memory. This avoids re-reading and
//! re-inflating the 100 MB jar for every one of the ~2,000 entries.

use std::collections::BTreeMap;

use umicraft_model::{ModelError, ModelResult, ResourceSource};

use crate::atlas::BlockAtlas;
use crate::client_jar::ClientJar;
use crate::{AssetsError, AssetsResult};

/// Every blockstate, block model and block/item texture of one client jar.
#[derive(Debug, Clone)]
pub struct ClientResources {
    files: BTreeMap<String, Vec<u8>>,
    atlas: BlockAtlas,
}

impl ClientResources {
    /// Reads the M5 subset of `jar` and builds its texture array.
    ///
    /// # Errors
    /// [`AssetsError`] when the jar cannot be read, an image is malformed, or
    /// the atlas is inconsistent.
    pub fn load(_jar: &ClientJar) -> AssetsResult<Self> {
        Err(AssetsError::Missing {
            what: "M5 client resources (not implemented yet)".to_string(),
            path: "client jar".to_string(),
        })
    }

    /// Reads one virtual resource path.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }

    /// Number of loaded files.
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// The decoded texture array.
    #[must_use]
    pub fn atlas(&self) -> &BlockAtlas {
        &self.atlas
    }
}

impl ResourceSource for ClientResources {
    fn read(&self, path: &str) -> ModelResult<Vec<u8>> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| ModelError::Resource {
                path: path.to_string(),
                message: "the verified client jar does not contain this entry".to_string(),
            })
    }

    fn list(&self, prefix: &str) -> Vec<String> {
        self.files
            .keys()
            .filter(|path| path.starts_with(prefix))
            .cloned()
            .collect()
    }
}

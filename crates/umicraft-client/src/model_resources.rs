// SPDX-License-Identifier: MIT OR Apache-2.0

//! One-time loading of the 1.8 block models and the real texture array.
//!
//! [`ModelResources::load`] verifies the user's official 1.8.8 client jar,
//! reads the blockstate, block-model, block-texture and item-texture entries
//! once, builds the deterministic [`BlockAtlas`] and bakes the
//! [`BlockModelSet`] from [`definitions`]. The resulting [`ModelMeshContext`]
//! is shared by every meshing worker through an [`Arc`], so a chunk job never
//! touches the disk.
//!
//! The bundle is also the single source of the acceptance counts printed by
//! `--block-demo`: texture entries are counted from the same in-memory file
//! list that the atlas was built from.
//!
//! Legal boundary (spec §3): the jar is only read from the user's installation
//! at runtime. No Mojang asset is embedded in this crate or copied into the
//! repository/release.

use std::sync::Arc;

use thiserror::Error;
use umicraft_assets::{AssetsError, BlockAtlas, ClientResources};
use umicraft_mesh::ModelMeshContext;
use umicraft_model::{definitions, BlockModelSet, ModelError, ResourceSource};

use crate::official;

/// Virtual prefix of the block textures inside the client jar.
const BLOCK_TEXTURES_PREFIX: &str = "assets/minecraft/textures/blocks/";
/// Virtual prefix of the item textures inside the client jar.
const ITEM_TEXTURES_PREFIX: &str = "assets/minecraft/textures/items/";

/// Number of texture entries of one kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextureEntries {
    /// Decoded PNG layers.
    pub png: usize,
    /// Animation metadata files; these add no atlas layer.
    pub mcmeta: usize,
}

impl TextureEntries {
    /// Total files of this kind, including the metadata.
    #[must_use]
    pub fn total(self) -> usize {
        self.png + self.mcmeta
    }
}

/// The deterministic counts of one loaded resource set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelStats {
    /// Blockstate documents parsed from the jar.
    pub blockstates: usize,
    /// Model documents under `models/block`.
    pub source_models: usize,
    /// Block texture PNGs and their `.mcmeta` files.
    pub block_textures: TextureEntries,
    /// Item texture PNGs and their `.mcmeta` files.
    pub item_textures: TextureEntries,
    /// Texture-array layers, including the missing-texture checker.
    pub layers: u32,
    /// Atlas layer width in texels.
    pub atlas_width: u32,
    /// Atlas layer height in texels.
    pub atlas_height: u32,
    /// Exact RGBA8 payload size of the atlas.
    pub atlas_bytes: usize,
    /// Wire block states that resolved to a baked model.
    pub block_states: usize,
    /// Metadata mappings that used a blockstate document's first variant.
    pub fallback_variants: usize,
}

/// Every failure of the official block-model loader.
#[derive(Debug, Error)]
pub enum ModelResourcesError {
    /// Neither `$HOME/.minecraft` nor the platform equivalent exists.
    #[error("no official Minecraft installation found for the 1.8.8 block models and atlas")]
    NoInstallation,
    /// The installation layout, version JSON or client jar failed verification.
    #[error(transparent)]
    Assets(#[from] AssetsError),
    /// A blockstate/model failed to parse or bake.
    #[error(transparent)]
    Model(#[from] ModelError),
}

/// The owned resources and the shared model-meshing context of one run.
#[derive(Debug, Clone)]
pub struct ModelResources {
    resources: ClientResources,
    models: Arc<BlockModelSet>,
    context: Arc<ModelMeshContext>,
}

impl ModelResources {
    /// Loads the 1.8.8 models and atlas from the installation found in the
    /// environment.
    ///
    /// # Errors
    /// [`ModelResourcesError::NoInstallation`] when there is no installation,
    /// or the asset/model error of the first corrupt or missing resource.
    pub fn load() -> Result<Self, ModelResourcesError> {
        let install = umicraft_assets::install::OfficialInstall::from_env()
            .ok_or(ModelResourcesError::NoInstallation)?;
        Self::load_from_install(&install)
    }

    /// Loads the resources, logging one warning and returning `None` on any
    /// failure.
    ///
    /// The windowed client and the terrain screenshot use this entry point:
    /// a missing installation, corrupt jar or unsupported model leaves the
    /// immutable bundle absent, so the legacy mesh/texture path runs instead
    /// of mixing real-layer meshes with the procedural texture array.
    #[must_use]
    pub fn load_or_warn() -> Option<Self> {
        Self::load_or_warn_from(umicraft_assets::install::OfficialInstall::from_env())
    }

    /// Reads and bakes one installation.
    ///
    /// The jar is read in one bulk pass; the model set and the texture layout
    /// both come from that same [`ClientResources`] snapshot.
    ///
    /// # Errors
    /// The asset, PNG/atlas or model error of the first invalid resource.
    pub fn load_from_install(
        install: &umicraft_assets::install::OfficialInstall,
    ) -> Result<Self, ModelResourcesError> {
        let jar = official::open_client_jar(install)?;
        let resources = ClientResources::load(&jar)?;
        let models = Arc::new(BlockModelSet::build(&resources, definitions())?);
        let context = Arc::new(ModelMeshContext {
            models: Arc::clone(&models),
            atlas: Arc::new(resources.atlas().layout().clone()),
        });
        tracing::info!(
            root = %install.root().display(),
            blockstates = models.blockstate_count(),
            source_models = models.source_model_count(),
            layers = resources.atlas().layers(),
            "loaded the 1.8 block models and texture atlas"
        );
        Ok(Self {
            resources,
            models,
            context,
        })
    }

    /// Loads from an optional installation, warning exactly once on failure.
    fn load_or_warn_from(
        install: Option<umicraft_assets::install::OfficialInstall>,
    ) -> Option<Self> {
        let Some(install) = install else {
            tracing::warn!(
                "no official Minecraft installation found; using procedural terrain textures"
            );
            return None;
        };
        match Self::load_from_install(&install) {
            Ok(resources) => Some(resources),
            Err(error) => {
                tracing::warn!(
                    %error,
                    root = %install.root().display(),
                    "could not load the 1.8 block models and atlas; using procedural terrain textures"
                );
                None
            }
        }
    }

    /// The verified in-memory resources and their atlas.
    #[must_use]
    pub fn resources(&self) -> &ClientResources {
        &self.resources
    }

    /// The decoded texture array.
    #[must_use]
    pub fn atlas(&self) -> &BlockAtlas {
        self.resources.atlas()
    }

    /// The baked model set.
    #[must_use]
    pub fn models(&self) -> &BlockModelSet {
        &self.models
    }

    /// A clone of the immutable context shared with the mesh workers.
    #[must_use]
    pub fn context(&self) -> Arc<ModelMeshContext> {
        Arc::clone(&self.context)
    }

    /// The counts used by the `--block-demo` report.
    #[must_use]
    pub fn stats(&self) -> ModelStats {
        let atlas = self.atlas();
        ModelStats {
            blockstates: self.models.blockstate_count(),
            source_models: self.models.source_model_count(),
            block_textures: count_textures(&self.resources, BLOCK_TEXTURES_PREFIX),
            item_textures: count_textures(&self.resources, ITEM_TEXTURES_PREFIX),
            layers: atlas.layers(),
            atlas_width: atlas.width(),
            atlas_height: atlas.height(),
            atlas_bytes: atlas.pixels().len(),
            block_states: self.models.state_count(),
            fallback_variants: self.models.fallback_count(),
        }
    }
}

/// Counts the PNG and `.mcmeta` entries below `prefix`.
fn count_textures(resources: &ClientResources, prefix: &str) -> TextureEntries {
    let mut entries = TextureEntries::default();
    for path in resources.list(prefix) {
        if path.ends_with(".png") {
            entries.png += 1;
        } else if path.ends_with(".png.mcmeta") {
            entries.mcmeta += 1;
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_entries_count_the_metadata_separately() {
        let entries = TextureEntries {
            png: 373,
            mcmeta: 9,
        };
        assert_eq!(entries.total(), 382);
    }

    #[test]
    fn no_installation_warns_once_and_returns_none() {
        let (resources, messages) =
            crate::test_support::capture(|| ModelResources::load_or_warn_from(None));
        assert!(resources.is_none());
        assert_eq!(messages.len(), 1, "expected one warning, got {messages:?}");
        assert!(messages[0].contains("procedural terrain textures"));
    }
}

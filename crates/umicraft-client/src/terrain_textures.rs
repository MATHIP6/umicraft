// SPDX-License-Identifier: MIT OR Apache-2.0

//! The terrain textures: the texture array the GPU samples and the mesher
//! that writes layer indices into it, always chosen together (ADR-0012).
//!
//! Two sets exist. The **procedural** set is original Umicraft art: the
//! renderer's built-in 19-layer array and the legacy cube mesher. The
//! **official** set comes from the user's 1.8.8 client jar: the
//! [`BlockAtlas`] and the model mesher's [`ModelMeshContext`]. A mesh of one
//! set drawn with the array of the other samples the wrong textures without
//! any error, so the two halves only travel together:
//!
//! 1. [`TerrainTextures::load`] picks the set once at start-up;
//! 2. [`TerrainTextures::install`] binds its array on a
//!    [`BlockTextureTarget`] (the renderer) and falls back to the procedural
//!    set as a whole when the GPU rejects the official array;
//! 3. the resulting [`InstalledTerrain`] is the only source of the
//!    [`TerrainMesher`] the mesh workers use.

use std::sync::Arc;

use umicraft_assets::BlockAtlas;
use umicraft_mesh::{mesh_chunk, mesh_chunk_with_models, ChunkSnapshot, Mesh, ModelMeshContext};
use umicraft_render::{BlockAtlasError, Renderer};
use umicraft_world::ChunkPos;

use crate::model_resources::ModelResources;

/// Where the terrain texture array is bound: the renderer, or a test double.
pub trait BlockTextureTarget {
    /// Binds a layer-major RGBA8 texture array of `layers` layers.
    ///
    /// # Errors
    /// [`BlockAtlasError`] when the target cannot hold the array; the
    /// previously bound array stays in place.
    fn set_block_atlas(
        &mut self,
        width: u32,
        height: u32,
        layers: u32,
        pixels: &[u8],
    ) -> Result<(), BlockAtlasError>;

    /// Binds the built-in procedural array.
    fn use_procedural_blocks(&mut self);
}

impl BlockTextureTarget for Renderer {
    fn set_block_atlas(
        &mut self,
        width: u32,
        height: u32,
        layers: u32,
        pixels: &[u8],
    ) -> Result<(), BlockAtlasError> {
        Renderer::set_block_atlas(self, width, height, layers, pixels)
    }

    fn use_procedural_blocks(&mut self) {
        Renderer::use_procedural_blocks(self);
    }
}

/// A terrain texture set that is not bound yet.
#[derive(Debug)]
pub enum TerrainTextures {
    /// The original procedural array and the legacy cube mesher.
    Procedural,
    /// The user's 1.8.8 atlas and the model mesher that indexes it.
    Official {
        /// The texture array to bind.
        atlas: BlockAtlas,
        /// The model-meshing context whose layout matches `atlas`.
        context: Arc<ModelMeshContext>,
    },
}

impl TerrainTextures {
    /// Loads the official set with optional resource packs, or falls back to procedural.
    #[must_use]
    pub fn load_with_packs(paths: &[std::path::PathBuf]) -> Self {
        match ModelResources::load_with_pack_paths(paths) {
            Ok(resources) => Self::official(&resources),
            Err(error) => {
                tracing::warn!(
                    %error,
                    "could not load official 1.8 models or packs; using procedural terrain textures"
                );
                Self::Procedural
            }
        }
    }

    /// Loads from an optional installation, used by tests.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn load_from(install: Option<umicraft_assets::install::OfficialInstall>) -> Self {
        let Some(install) = install else {
            tracing::warn!(
                "no official Minecraft installation found; using procedural terrain textures"
            );
            return Self::Procedural;
        };
        match ModelResources::load_from_install(&install) {
            Ok(resources) => Self::official(&resources),
            Err(error) => {
                tracing::warn!(
                    %error,
                    root = %install.root().display(),
                    "could not load the 1.8 block models and atlas; using procedural terrain textures"
                );
                Self::Procedural
            }
        }
    }

    /// Loads the official set from the installation found in the
    /// environment, or picks the procedural set with exactly one warning.
    #[must_use]
    pub fn load() -> Self {
        Self::load_with_packs(&[])
    }

    /// The official set of already loaded resources.
    ///
    /// Copies the atlas pixels once, so the caller may keep the resources
    /// for their counts (the `--block-demo` report).
    #[must_use]
    pub fn official(resources: &ModelResources) -> Self {
        Self::Official {
            atlas: resources.atlas().clone(),
            context: resources.context(),
        }
    }



    /// Binds this set's array on `target` and returns the matching mesher.
    ///
    /// A rejected official array logs one warning and installs the
    /// procedural set instead, so the target and the mesher never disagree.
    pub fn install(self, target: &mut impl BlockTextureTarget) -> InstalledTerrain {
        match self {
            Self::Procedural => install_procedural(target),
            Self::Official { atlas, context } => {
                let bound = target.set_block_atlas(
                    atlas.width(),
                    atlas.height(),
                    atlas.layers(),
                    atlas.pixels(),
                );
                match bound {
                    Ok(()) => {
                        tracing::info!(
                            layers = atlas.layers(),
                            "uploaded the 1.8 block texture array"
                        );
                        InstalledTerrain {
                            mesher: TerrainMesher(MesherKind::Models(context)),
                        }
                    }
                    Err(error) => {
                        tracing::warn!(
                            %error,
                            "block texture array rejected; using procedural terrain textures"
                        );
                        install_procedural(target)
                    }
                }
            }
        }
    }
}

fn install_procedural(target: &mut impl BlockTextureTarget) -> InstalledTerrain {
    target.use_procedural_blocks();
    InstalledTerrain {
        mesher: TerrainMesher(MesherKind::Procedural),
    }
}

/// A terrain texture set whose array is bound.
///
/// It is the only way to obtain a [`TerrainMesher`], so a mesh pool always
/// writes layers of the array the renderer samples.
#[derive(Debug, Clone)]
pub struct InstalledTerrain {
    mesher: TerrainMesher,
}

impl InstalledTerrain {
    /// Whether the official 1.8.8 array is bound.
    #[must_use]
    pub fn is_official(&self) -> bool {
        matches!(self.mesher.0, MesherKind::Models(_))
    }

    /// The mesher that matches the bound array.
    #[must_use]
    pub fn mesher(&self) -> TerrainMesher {
        self.mesher.clone()
    }
}

/// Meshes chunks for the installed terrain texture set.
///
/// Cheap to clone: the official variant shares one immutable context.
#[derive(Debug, Clone)]
pub struct TerrainMesher(MesherKind);

#[derive(Debug, Clone)]
enum MesherKind {
    Procedural,
    Models(Arc<ModelMeshContext>),
}

impl TerrainMesher {
    /// Meshes one chunk column from its immutable snapshot.
    #[must_use]
    pub fn mesh(&self, pos: ChunkPos, snapshot: &ChunkSnapshot) -> Mesh {
        match &self.0 {
            MesherKind::Procedural => mesh_chunk(pos, snapshot),
            MesherKind::Models(context) => mesh_chunk_with_models(pos, snapshot, context),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_model::{AtlasLayout, BlockModelSet, ModelResult, ResourceSource};
    use umicraft_world::{BlockState, World};

    use crate::test_support::TextureTarget as Target;

    struct NoResources;

    impl ResourceSource for NoResources {
        fn read(&self, path: &str) -> ModelResult<Vec<u8>> {
            Err(umicraft_model::ModelError::Resource {
                path: path.to_string(),
                message: "no resources in this test".to_string(),
            })
        }

        fn list(&self, _prefix: &str) -> Vec<String> {
            Vec::new()
        }
    }

    /// An official set with an empty model set and a one-layer atlas.
    fn official() -> TerrainTextures {
        let layout = AtlasLayout::default();
        let atlas = BlockAtlas::new(1, 1, vec![255; 4], layout.clone()).expect("atlas");
        let models = BlockModelSet::build(&NoResources, &[]).expect("empty model set");
        TerrainTextures::Official {
            atlas,
            context: Arc::new(ModelMeshContext {
                models: Arc::new(models),
                atlas: Arc::new(layout),
                tints: None,
            }),
        }
    }

    /// One stone block, meshed by `mesher`.
    fn stone_mesh(mesher: &TerrainMesher) -> Mesh {
        let mut world = World::new(0);
        let pos = ChunkPos::new(0, 0);
        let payload = vec![0u8; umicraft_world::chunk::payload_len(1, true, true)];
        world
            .apply_chunk(
                pos,
                umicraft_world::ChunkPayload {
                    primary_bit_mask: 1,
                    ground_up: true,
                    data: &payload,
                },
            )
            .expect("an empty column");
        world.set_block(
            umicraft_world::BlockPos::new(1, 1, 1),
            BlockState::new(1, 0),
        );
        mesher.mesh(pos, &ChunkSnapshot::from_world(&world, pos))
    }

    #[test]
    fn an_accepted_official_array_pairs_with_the_model_mesher() {
        let mut target = Target::default();
        let installed = official().install(&mut target);
        assert!(installed.is_official());
        assert_eq!(target.atlas_layers, Some(1));
        assert_eq!(target.procedural_calls, 0);
        // The model path only writes layers of that one-layer atlas.
        let mesh = stone_mesh(&installed.mesher());
        assert!(!mesh.vertices.is_empty());
        assert!(mesh.vertices.iter().all(|vertex| vertex.layer == 0));
    }

    #[test]
    fn a_rejected_official_array_falls_back_to_the_procedural_set() {
        let mut target = Target {
            reject: true,
            ..Target::default()
        };
        let (installed, messages) =
            crate::test_support::capture(|| official().install(&mut target));
        assert!(!installed.is_official());
        assert_eq!(target.atlas_layers, None);
        assert_eq!(target.procedural_calls, 1, "the procedural array is bound");
        assert_eq!(messages.len(), 1, "expected one warning, got {messages:?}");
        assert!(messages[0].contains("procedural terrain textures"));
        // The mesher is the procedural one: stone uses its stone layer.
        let mesh = stone_mesh(&installed.mesher());
        let stone = umicraft_mesh::texture_layer(BlockState::new(1, 0), umicraft_mesh::Face::Up);
        assert!(mesh.vertices.iter().all(|vertex| vertex.layer == stone));
    }

    #[test]
    fn the_procedural_set_binds_the_procedural_array_explicitly() {
        let mut target = Target {
            atlas_layers: Some(600),
            ..Target::default()
        };
        let installed = TerrainTextures::Procedural.install(&mut target);
        assert!(!installed.is_official());
        assert_eq!(target.atlas_layers, None);
        assert_eq!(target.procedural_calls, 1);
    }

    #[test]
    fn no_installation_warns_once_and_picks_the_procedural_set() {
        let (textures, messages) =
            crate::test_support::capture(|| TerrainTextures::load_from(None));
        assert!(matches!(textures, TerrainTextures::Procedural));
        assert_eq!(messages.len(), 1, "expected one warning, got {messages:?}");
        assert!(messages[0].contains("procedural terrain textures"));
    }
}

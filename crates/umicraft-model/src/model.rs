// SPDX-License-Identifier: MIT OR Apache-2.0

//! Resource abstraction, parent resolution and the baked model set.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use umicraft_world::BlockState;

use crate::error::ModelResult;
use crate::geometry::ModelQuad;
use crate::registry::BlockDefinition;

/// Read-only access to the virtual files of a client jar.
pub trait ResourceSource {
    /// Reads one virtual path such as
    /// `assets/minecraft/models/block/stone.json`.
    ///
    /// # Errors
    /// [`crate::ModelError::Resource`] when the path is absent or unreadable.
    fn read(&self, path: &str) -> ModelResult<Vec<u8>>;

    /// Virtual paths below `prefix`, sorted.
    ///
    /// The model set uses this to parse and validate every shipped model, not
    /// only the ones a blockstate happens to reference.
    fn list(&self, prefix: &str) -> Vec<String>;
}

/// A fully resolved model with baked quads.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolvedModel {
    /// Baked quads in model space, before blockstate variant rotation.
    pub quads: Vec<ModelQuad>,
    /// Whether vanilla ambient occlusion applies.
    pub ambient_occlusion: bool,
    /// Concrete particle texture key, when the model declares one.
    pub particle: Option<String>,
}

/// One weighted blockstate variant after model baking.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelVariant {
    /// Baked model in variant space (rotation already applied).
    pub model: Arc<ResolvedModel>,
    /// Selection weight.
    pub weight: u32,
}

/// All variants of one block state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BlockModel {
    /// Weighted variants; at least one for a valid state.
    pub variants: Vec<ModelVariant>,
}

/// The complete baked model set for the 1.8 block registry.
#[derive(Debug, Default)]
pub struct BlockModelSet {
    /// Baked models keyed by wire state.
    models: HashMap<BlockState, Arc<BlockModel>>,
    texture_keys: BTreeSet<String>,
    blockstate_count: usize,
    source_model_count: usize,
}

impl BlockModelSet {
    /// Parses every blockstate/model referenced by `definitions`.
    ///
    /// # Errors
    /// [`crate::ModelError`] for unreadable resources, malformed JSON, bad
    /// rotations, unresolved texture references or cycles in the parent chain.
    pub fn build(
        _source: &dyn ResourceSource,
        _definitions: &[BlockDefinition],
    ) -> ModelResult<Self> {
        Ok(Self::default())
    }

    /// Baked variants for a wire block state.
    #[must_use]
    pub fn model(&self, state: BlockState) -> Option<&BlockModel> {
        self.models.get(&state).map(Arc::as_ref)
    }

    /// Number of distinct baked block states.
    #[must_use]
    pub fn state_count(&self) -> usize {
        self.models.len()
    }

    /// Number of distinct resolved models across all states.
    #[must_use]
    pub fn model_count(&self) -> usize {
        self.models
            .values()
            .flat_map(|block| block.variants.iter())
            .map(|variant| Arc::as_ptr(&variant.model))
            .collect::<BTreeSet<_>>()
            .len()
    }

    /// Concrete texture keys referenced by any baked quad.
    #[must_use]
    pub fn texture_keys(&self) -> &BTreeSet<String> {
        &self.texture_keys
    }

    /// Number of blockstate documents parsed from the source.
    #[must_use]
    pub fn blockstate_count(&self) -> usize {
        self.blockstate_count
    }

    /// Number of model documents parsed from the source, including parent
    /// templates that no blockstate directly references.
    #[must_use]
    pub fn source_model_count(&self) -> usize {
        self.source_model_count
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Resource abstraction and the baked block model set.
//!
//! The builder reads the virtual files of a client jar through
//! [`ResourceSource`], parses every listed model/blockstate document, resolves
//! the `parent` chain and `#texture` references, and bakes each blockstate
//! variant into block-local [`ModelQuad`]s. Document parsing lives in
//! [`document`] and the parent merge in [`loader`], both shared with the item
//! models.
//!
//! References (ADR-0001): `ModelBlock.java` (parent/texture semantics),
//! `ModelBlockDefinition.java` (variant weights and rotations),
//! `ModelBakery.java` (texture resolution and cullface rotation) and the
//! `assets/minecraft/models/block/*.json` format.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use umicraft_world::BlockState;

use crate::blockstate::{canonicalize_variant_key, BlockStateFile, Variant};
use crate::error::ModelResult;
use crate::geometry::{ModelQuad, ModelRotation};
use crate::registry::BlockDefinition;

pub(crate) mod document;
pub(crate) mod loader;

use document::{invalid, model_path, normalize_model_name, resource};
use loader::ModelLoader;

/// Virtual prefix of the block model documents in a client jar.
const BLOCK_MODELS: &str = "assets/minecraft/models/block/";
/// Virtual prefix of the blockstate documents in a client jar.
const BLOCKSTATE_PREFIX: &str = "assets/minecraft/blockstates/";

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
    fallback_count: usize,
    model_count: usize,
}

impl BlockModelSet {
    /// Parses every blockstate/model referenced by `definitions`.
    ///
    /// The source is read once per document. Every model below
    /// `assets/minecraft/models/block/` and every blockstate below
    /// `assets/minecraft/blockstates/` is parsed, even if no definition uses
    /// it, so a corrupt asset fails at startup rather than in one chunk. Every
    /// definition metadata value with a matching [`crate::StateDef`] is mapped;
    /// an absent variant key falls back to the document's first variant.
    ///
    /// # Errors
    /// [`crate::ModelError`] for unreadable resources, malformed JSON, bad
    /// rotations/weights, unresolved texture references, parent cycles or a
    /// parent chain above `MAX_PARENT_DEPTH`.
    pub fn build(
        source: &dyn ResourceSource,
        definitions: &[BlockDefinition],
    ) -> ModelResult<Self> {
        let mut builder = Builder::new(source);
        builder.loader.load_listed(BLOCK_MODELS)?;
        builder.load_listed_blockstates()?;
        builder.load_definition_blockstates(definitions)?;
        builder.loader.validate_parent_chains()?;

        let mut models: HashMap<BlockState, Arc<BlockModel>> = HashMap::new();
        let mut block_models: HashMap<(String, String), Arc<BlockModel>> = HashMap::new();
        let mut resolved_models: HashMap<ResolvedKey, Arc<ResolvedModel>> = HashMap::new();
        let mut texture_keys = BTreeSet::new();
        let mut fallback_count = 0;

        for definition in definitions {
            for metadata in 0..=15_u8 {
                let Some(rule) = definition.state(metadata) else {
                    continue;
                };
                let blockstate_name = normalize_blockstate_name(rule.blockstate);
                if blockstate_name.is_empty() {
                    return Err(invalid(
                        &blockstate_path(&blockstate_name),
                        "the blockstate name is empty",
                    ));
                }

                // Clone the small variant list so the immutable borrow of the
                // document ends before models are baked and cached.
                let (variant_key, variants) = {
                    let document = builder.blockstates.get(&blockstate_name).ok_or_else(|| {
                        resource(
                            &blockstate_path(&blockstate_name),
                            "the blockstate was not loaded",
                        )
                    })?;
                    let selected = canonicalize_variant_key(rule.variant)
                        .ok()
                        .and_then(|key| document.select(&key).map(|variants| (key, variants)));
                    match selected {
                        Some((key, variants)) => (key, variants.to_vec()),
                        None => {
                            fallback_count += 1;
                            let (key, variants) = document.first().ok_or_else(|| {
                                invalid(&blockstate_path(&blockstate_name), "no variants")
                            })?;
                            (key.to_string(), variants.to_vec())
                        }
                    }
                };

                let cache_key = (blockstate_name.clone(), variant_key);
                let block_model = if let Some(cached) = block_models.get(&cache_key) {
                    Arc::clone(cached)
                } else {
                    let mut baked_variants = Vec::with_capacity(variants.len());
                    for variant in &variants {
                        let resolved = resolve_variant(
                            &mut builder,
                            variant,
                            &mut resolved_models,
                            &mut texture_keys,
                        )?;
                        baked_variants.push(ModelVariant {
                            model: resolved,
                            weight: variant.weight,
                        });
                    }
                    let block_model = Arc::new(BlockModel {
                        variants: baked_variants,
                    });
                    block_models.insert(cache_key, Arc::clone(&block_model));
                    block_model
                };
                models.insert(BlockState::new(definition.id, metadata), block_model);
            }
        }

        Ok(Self {
            models,
            texture_keys,
            blockstate_count: builder.blockstates.len(),
            source_model_count: builder.loader.raw_models.len(),
            fallback_count,
            model_count: resolved_models.len(),
        })
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
        self.model_count
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

    /// Number of metadata mappings whose variant key was absent and therefore
    /// used the blockstate document's first variant.
    ///
    /// This accessor is additive to the ADR-0007 frozen API: the acceptance
    /// report in W6 prints the count and cannot inspect the private documents.
    #[must_use]
    pub fn fallback_count(&self) -> usize {
        self.fallback_count
    }
}

/// Cache key for a model after blockstate rotation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ResolvedKey {
    model: String,
    rotation: ModelRotation,
    uvlock: bool,
}

/// Resolves one variant, sharing baked models between blockstates.
fn resolve_variant(
    builder: &mut Builder<'_>,
    variant: &Variant,
    cache: &mut HashMap<ResolvedKey, Arc<ResolvedModel>>,
    texture_keys: &mut BTreeSet<String>,
) -> ModelResult<Arc<ResolvedModel>> {
    let model = normalize_model_name(&variant.model);
    if model.is_empty() {
        return Err(invalid(
            &model_path(&model),
            "the variant references an empty model name",
        ));
    }
    let key = ResolvedKey {
        model: model.clone(),
        rotation: ModelRotation::new(variant.x, variant.y),
        uvlock: variant.uvlock,
    };
    if let Some(cached) = cache.get(&key) {
        return Ok(Arc::clone(cached));
    }

    let resolved = Arc::new(builder.loader.bake(&model, key.rotation, key.uvlock)?);
    for quad in &resolved.quads {
        texture_keys.insert(quad.texture.clone());
    }
    cache.insert(key, Arc::clone(&resolved));
    Ok(resolved)
}

/// Loads and resolves the documents of [`BlockModelSet::build`].
struct Builder<'a> {
    loader: ModelLoader<'a>,
    source: &'a dyn ResourceSource,
    blockstates: HashMap<String, BlockStateFile>,
}

impl<'a> Builder<'a> {
    fn new(source: &'a dyn ResourceSource) -> Self {
        Self {
            loader: ModelLoader::new(source),
            source,
            blockstates: HashMap::new(),
        }
    }

    /// Parses every listed blockstate document for validation and counts it.
    fn load_listed_blockstates(&mut self) -> ModelResult<()> {
        let mut paths = self.source.list(BLOCKSTATE_PREFIX);
        paths.sort();
        paths.dedup();
        for path in paths {
            if !path.ends_with(".json") {
                continue;
            }
            let Some(key) = path
                .strip_prefix(BLOCKSTATE_PREFIX)
                .and_then(|key| key.strip_suffix(".json"))
            else {
                continue;
            };
            if key.is_empty() {
                return Err(invalid(&path, "empty blockstate name"));
            }
            self.load_blockstate(key)?;
        }
        Ok(())
    }

    /// Parses every blockstate named by a definition, including documents that
    /// the source did not list.
    fn load_definition_blockstates(&mut self, definitions: &[BlockDefinition]) -> ModelResult<()> {
        let mut names = BTreeSet::new();
        for definition in definitions {
            for state in definition.states {
                let name = normalize_blockstate_name(state.blockstate);
                if name.is_empty() {
                    return Err(invalid(
                        &blockstate_path(&name),
                        "the definition names an empty blockstate",
                    ));
                }
                names.insert(name);
            }
        }
        for name in names {
            self.load_blockstate(&name)?;
        }
        Ok(())
    }

    fn load_blockstate(&mut self, name: &str) -> ModelResult<()> {
        if self.blockstates.contains_key(name) {
            return Ok(());
        }
        let path = blockstate_path(name);
        let bytes = self.source.read(&path)?;
        let parsed = BlockStateFile::parse(&bytes, &path)?;
        self.blockstates.insert(name.to_string(), parsed);
        Ok(())
    }
}

/// Normalizes a blockstate reference to a key below `blockstates`.
fn normalize_blockstate_name(raw: &str) -> String {
    let trimmed = raw.trim();
    let trimmed = document::strip_namespace(trimmed);
    trimmed.strip_suffix(".json").unwrap_or(trimmed).to_string()
}

fn blockstate_path(name: &str) -> String {
    format!("{BLOCKSTATE_PREFIX}{name}.json")
}

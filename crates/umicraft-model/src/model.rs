// SPDX-License-Identifier: MIT OR Apache-2.0

//! Resource abstraction, parent resolution and the baked model set.
//!
//! The builder reads the virtual files of a client jar through
//! [`ResourceSource`], parses every listed model/blockstate document, resolves
//! the `parent` chain and `#texture` references, and bakes each blockstate
//! variant into block-local [`ModelQuad`]s.
//!
//! References (ADR-0001): `ModelBlock.java` (parent/texture semantics),
//! `ModelBlockDefinition.java` (variant weights and rotations),
//! `ModelBakery.java` (texture resolution and cullface rotation) and the
//! `assets/minecraft/models/block/*.json` format.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use serde_json::{Map, Value};
use umicraft_world::BlockState;

use crate::blockstate::{canonicalize_variant_key, BlockStateFile, Variant};
use crate::error::{ModelError, ModelResult};
use crate::geometry::{
    bake_quad, ElementRotation, Face, ModelQuad, ModelRotation, QuadSpec, RotationAxis,
};
use crate::registry::BlockDefinition;

/// Virtual prefix of the block model documents in a client jar.
const MODEL_PREFIX: &str = "assets/minecraft/models/block/";
/// Virtual prefix of the blockstate documents in a client jar.
const BLOCKSTATE_PREFIX: &str = "assets/minecraft/blockstates/";
/// Hard limit for the number of models in a `parent` chain; real 1.8 chains
/// are shorter than ten.
const MAX_PARENT_DEPTH: usize = 64;

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
        builder.load_listed_models()?;
        builder.load_listed_blockstates()?;
        builder.load_definition_blockstates(definitions)?;
        builder.validate_parent_chains()?;

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
            source_model_count: builder.raw_models.len(),
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

    let resolved = Arc::new(builder.bake(&model, key.rotation, key.uvlock)?);
    for quad in &resolved.quads {
        texture_keys.insert(quad.texture.clone());
    }
    cache.insert(key, Arc::clone(&resolved));
    Ok(resolved)
}

/// Loads and resolves the documents of [`BlockModelSet::build`].
struct Builder<'a> {
    source: &'a dyn ResourceSource,
    raw_models: HashMap<String, RawModel>,
    merged_models: HashMap<String, Arc<MergedModel>>,
    blockstates: HashMap<String, BlockStateFile>,
}

impl<'a> Builder<'a> {
    fn new(source: &'a dyn ResourceSource) -> Self {
        Self {
            source,
            raw_models: HashMap::new(),
            merged_models: HashMap::new(),
            blockstates: HashMap::new(),
        }
    }

    /// Parses every listed block model for validation and counts it.
    fn load_listed_models(&mut self) -> ModelResult<()> {
        let mut paths = self.source.list(MODEL_PREFIX);
        paths.sort();
        paths.dedup();
        for path in paths {
            if !path.ends_with(".json") {
                continue;
            }
            let Some(key) = path
                .strip_prefix(MODEL_PREFIX)
                .and_then(|key| key.strip_suffix(".json"))
            else {
                continue;
            };
            if key.is_empty() {
                return Err(invalid(&path, "empty model name"));
            }
            self.load_raw_model(key)?;
        }
        Ok(())
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

    /// Resolves every listed model's parent chain so cycles and missing
    /// parents are reported even for models no blockstate currently uses.
    fn validate_parent_chains(&mut self) -> ModelResult<()> {
        let names: Vec<String> = self.raw_models.keys().cloned().collect();
        for name in names {
            self.merged(&name, &mut Vec::new())?;
        }
        Ok(())
    }

    fn load_raw_model(&mut self, name: &str) -> ModelResult<()> {
        if self.raw_models.contains_key(name) {
            return Ok(());
        }
        let path = model_path(name);
        let bytes = self.source.read(&path)?;
        let raw = RawModel::parse(&bytes, &path)?;
        self.raw_models.insert(name.to_string(), raw);
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

    /// Merges one model with its ancestors, caching the result by model name.
    fn merged(&mut self, name: &str, stack: &mut Vec<String>) -> ModelResult<Arc<MergedModel>> {
        if let Some(cached) = self.merged_models.get(name) {
            return Ok(Arc::clone(cached));
        }
        if stack.iter().any(|entry| entry == name) {
            return Err(ModelError::ParentCycle {
                path: name.to_string(),
            });
        }

        self.load_raw_model(name)?;
        let raw = self
            .raw_models
            .get(name)
            .ok_or_else(|| resource(&model_path(name), "the model was not loaded"))?
            .clone();
        stack.push(name.to_string());

        let RawModel {
            parent,
            textures,
            elements,
            ambient_occlusion,
        } = raw;
        let (mut merged_textures, merged_elements, merged_ao, parent_depth) =
            if let Some(parent_name) = parent {
                let parent = self.merged(&parent_name, stack)?;
                let ao = ambient_occlusion.unwrap_or(parent.ambient_occlusion);
                let own_elements = elements.map(Arc::new);
                (
                    parent.textures.clone(),
                    own_elements.or_else(|| parent.elements.clone()),
                    ao,
                    parent.depth,
                )
            } else {
                (
                    BTreeMap::new(),
                    elements.map(Arc::new),
                    ambient_occlusion.unwrap_or(true),
                    0,
                )
            };
        // Child textures override parent textures, as in `ModelBlock`.
        merged_textures.extend(textures);

        // The cached suffix of a parent chain must not hide an over-long
        // chain, so the depth is carried on the merged model itself.
        let depth = parent_depth + 1;
        if depth > MAX_PARENT_DEPTH {
            return Err(ModelError::ParentDepth {
                path: name.to_string(),
                limit: MAX_PARENT_DEPTH,
            });
        }

        stack.pop();
        let merged = Arc::new(MergedModel {
            textures: merged_textures,
            elements: merged_elements,
            ambient_occlusion: merged_ao,
            depth,
        });
        self.merged_models
            .insert(name.to_string(), Arc::clone(&merged));
        Ok(merged)
    }

    /// Bakes every element face of `name` with the given variant rotation.
    fn bake(
        &mut self,
        name: &str,
        rotation: ModelRotation,
        uvlock: bool,
    ) -> ModelResult<ResolvedModel> {
        let merged = self.merged(name, &mut Vec::new())?;
        let mut quads = Vec::new();
        if let Some(elements) = &merged.elements {
            for element in elements.iter() {
                for (face, spec) in &element.faces {
                    let texture = resolve_texture_value(&merged.textures, &spec.texture, name)?;
                    quads.push(bake_quad(&QuadSpec {
                        face: *face,
                        from: element.from,
                        to: element.to,
                        uv: spec.uv,
                        uv_rotation: spec.rotation,
                        cullface: spec.cullface,
                        texture: &texture,
                        tint: spec.tint,
                        shade: element.shade,
                        element_rotation: element.rotation,
                        model_rotation: rotation,
                        uvlock,
                    }));
                }
            }
        }

        let particle = match merged.textures.get("particle") {
            Some(value) => Some(resolve_texture_value(&merged.textures, value, name)?),
            None => None,
        };
        Ok(ResolvedModel {
            quads,
            ambient_occlusion: merged.ambient_occlusion,
            particle,
        })
    }
}

/// A parsed model document before parent merging.
#[derive(Debug, Clone)]
struct RawModel {
    parent: Option<String>,
    textures: BTreeMap<String, String>,
    elements: Option<Vec<Element>>,
    ambient_occlusion: Option<bool>,
}

impl RawModel {
    fn parse(bytes: &[u8], path: &str) -> ModelResult<Self> {
        let value: Value = serde_json::from_slice(bytes).map_err(|source| ModelError::Json {
            path: path.to_string(),
            source,
        })?;
        let object = value
            .as_object()
            .ok_or_else(|| invalid(path, "expected a JSON object"))?;

        let parent = match object.get("parent") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if value.trim().is_empty() => None,
            Some(Value::String(value)) => {
                let normalized = normalize_model_name(value);
                if normalized.is_empty() {
                    return Err(invalid(path, "`parent` is empty"));
                }
                Some(normalized)
            }
            Some(_) => return Err(invalid(path, "`parent` must be a string")),
        };

        let textures = parse_textures(object, path)?;
        let elements = match object.get("elements") {
            None | Some(Value::Null) => None,
            Some(Value::Array(items)) => Some(
                items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| parse_element(item, path, index))
                    .collect::<ModelResult<Vec<_>>>()?,
            ),
            Some(_) => return Err(invalid(path, "`elements` must be an array")),
        };
        let ambient_occlusion = match object.get("ambientocclusion") {
            None => None,
            Some(Value::Bool(value)) => Some(*value),
            Some(_) => return Err(invalid(path, "`ambientocclusion` must be a boolean")),
        };

        Ok(Self {
            parent,
            textures,
            elements,
            ambient_occlusion,
        })
    }
}

/// A model after parent merging.
#[derive(Debug)]
struct MergedModel {
    textures: BTreeMap<String, String>,
    elements: Option<Arc<Vec<Element>>>,
    ambient_occlusion: bool,
    /// Number of `parent` links between this model and the root model.
    depth: usize,
}

/// One `elements` entry.
#[derive(Debug, Clone)]
struct Element {
    from: [f64; 3],
    to: [f64; 3],
    rotation: Option<ElementRotation>,
    faces: BTreeMap<Face, FaceSpec>,
    shade: bool,
}

/// One face of an element.
#[derive(Debug, Clone, PartialEq)]
struct FaceSpec {
    texture: String,
    uv: Option<[f64; 4]>,
    cullface: Option<Face>,
    rotation: u8,
    tint: Option<u32>,
}

fn parse_textures(
    object: &Map<String, Value>,
    path: &str,
) -> ModelResult<BTreeMap<String, String>> {
    let mut textures = BTreeMap::new();
    let Some(value) = object.get("textures") else {
        return Ok(textures);
    };
    let entries = value
        .as_object()
        .ok_or_else(|| invalid(path, "`textures` must be a JSON object"))?;
    for (key, value) in entries {
        let Some(texture) = value.as_str() else {
            return Err(invalid(path, format!("texture `{key}` must be a string")));
        };
        if texture.trim().is_empty() {
            return Err(invalid(path, format!("texture `{key}` is empty")));
        }
        textures.insert(key.clone(), texture.trim().to_string());
    }
    Ok(textures)
}

fn parse_element(value: &Value, path: &str, index: usize) -> ModelResult<Element> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid(path, format!("element {index} must be a JSON object")))?;
    let from = parse_position(
        object
            .get("from")
            .ok_or_else(|| invalid(path, format!("element {index} has no `from`")))?,
        path,
        "from",
        true,
    )?;
    let to = parse_position(
        object
            .get("to")
            .ok_or_else(|| invalid(path, format!("element {index} has no `to`")))?,
        path,
        "to",
        true,
    )?;
    let rotation = match object.get("rotation") {
        None | Some(Value::Null) => None,
        Some(value) => Some(parse_rotation(value, path)?),
    };
    let faces_object = object
        .get("faces")
        .ok_or_else(|| invalid(path, format!("element {index} has no `faces`")))?
        .as_object()
        .ok_or_else(|| invalid(path, format!("element {index} `faces` must be an object")))?;
    if faces_object.is_empty() {
        return Err(invalid(
            path,
            format!("element {index} has no faces (expected 1 to 6)"),
        ));
    }

    let mut faces = BTreeMap::new();
    for (name, value) in faces_object {
        let face = Face::parse(name)
            .ok_or_else(|| invalid(path, format!("element {index} has unknown face `{name}`")))?;
        faces.insert(face, parse_face(value, path)?);
    }
    let shade = match object.get("shade") {
        None => true,
        Some(Value::Bool(value)) => *value,
        Some(_) => {
            return Err(invalid(
                path,
                format!("element {index} `shade` must be a boolean"),
            ))
        }
    };

    Ok(Element {
        from,
        to,
        rotation,
        faces,
        shade,
    })
}

fn parse_rotation(value: &Value, path: &str) -> ModelResult<ElementRotation> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid(path, "`rotation` must be an object"))?;
    let origin = parse_position(
        object
            .get("origin")
            .ok_or_else(|| invalid(path, "`rotation.origin` is missing"))?,
        path,
        "rotation.origin",
        false,
    )?;
    let axis = match object.get("axis").and_then(Value::as_str) {
        Some("x") => RotationAxis::X,
        Some("y") => RotationAxis::Y,
        Some("z") => RotationAxis::Z,
        Some(other) => {
            return Err(invalid(
                path,
                format!("`rotation.axis` is `{other}`, expected x, y or z"),
            ))
        }
        None => return Err(invalid(path, "`rotation.axis` must be a string")),
    };
    let angle = required_number(object, "angle", path)?;
    if angle != 0.0 && angle.abs() != 22.5 && angle.abs() != 45.0 {
        return Err(invalid(
            path,
            format!("rotation angle {angle} is not one of -45/-22.5/0/22.5/45"),
        ));
    }
    let rescale = match object.get("rescale") {
        None => false,
        Some(Value::Bool(value)) => *value,
        Some(_) => return Err(invalid(path, "`rotation.rescale` must be a boolean")),
    };
    Ok(ElementRotation {
        origin,
        axis,
        angle,
        rescale,
    })
}

fn parse_face(value: &Value, path: &str) -> ModelResult<FaceSpec> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid(path, "a face must be a JSON object"))?;
    let texture = object
        .get("texture")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(path, "a face has no string `texture`"))?
        .trim()
        .to_string();
    if texture.is_empty() {
        return Err(invalid(path, "a face has an empty `texture`"));
    }
    let uv = match object.get("uv") {
        None | Some(Value::Null) => None,
        Some(value) => Some(parse_uv(value, path)?),
    };
    let cullface = match object.get("cullface") {
        None | Some(Value::Null) => None,
        Some(Value::String(name)) => Some(
            Face::parse(name)
                .ok_or_else(|| invalid(path, format!("unknown `cullface` `{name}`")))?,
        ),
        Some(_) => return Err(invalid(path, "`cullface` must be a string")),
    };
    let rotation = match object.get("rotation") {
        None => 0,
        Some(value) => {
            let degrees = value
                .as_i64()
                .ok_or_else(|| invalid(path, "face `rotation` must be an integer"))?;
            match degrees {
                0 => 0,
                90 => 1,
                180 => 2,
                270 => 3,
                _ => {
                    return Err(invalid(
                        path,
                        format!("face rotation {degrees} is not 0/90/180/270"),
                    ))
                }
            }
        }
    };
    let tint = match object.get("tintindex") {
        None => None,
        Some(value) => {
            let tint = value
                .as_i64()
                .ok_or_else(|| invalid(path, "`tintindex` must be an integer"))?;
            if tint < -1 {
                return Err(invalid(path, "`tintindex` must be -1 or greater"));
            }
            u32::try_from(tint).ok()
        }
    };

    Ok(FaceSpec {
        texture,
        uv,
        cullface,
        rotation,
        tint,
    })
}

fn parse_position(value: &Value, path: &str, label: &str, bounded: bool) -> ModelResult<[f64; 3]> {
    let array = value
        .as_array()
        .ok_or_else(|| invalid(path, format!("`{label}` must be an array of three numbers")))?;
    if array.len() != 3 {
        return Err(invalid(
            path,
            format!("`{label}` needs 3 values, found {}", array.len()),
        ));
    }
    let mut position = [0.0; 3];
    for (index, value) in array.iter().enumerate() {
        let number = number_value(value, path, label)?;
        if bounded && !(-16.0..=32.0).contains(&number) {
            return Err(invalid(
                path,
                format!("`{label}` value {number} is outside -16..32"),
            ));
        }
        position[index] = number;
    }
    Ok(position)
}

fn parse_uv(value: &Value, path: &str) -> ModelResult<[f64; 4]> {
    let array = value
        .as_array()
        .ok_or_else(|| invalid(path, "`uv` must be an array of four numbers"))?;
    if array.len() != 4 {
        return Err(invalid(
            path,
            format!("`uv` needs 4 values, found {}", array.len()),
        ));
    }
    let mut uv = [0.0; 4];
    for (index, value) in array.iter().enumerate() {
        uv[index] = number_value(value, path, "uv")?;
    }
    Ok(uv)
}

fn required_number(object: &Map<String, Value>, key: &str, path: &str) -> ModelResult<f64> {
    let value = object
        .get(key)
        .ok_or_else(|| invalid(path, format!("missing `{key}`")))?;
    number_value(value, path, key)
}

fn number_value(value: &Value, path: &str, label: &str) -> ModelResult<f64> {
    let number = value
        .as_f64()
        .filter(|number| number.is_finite())
        .ok_or_else(|| invalid(path, format!("`{label}` must be a finite number")))?;
    Ok(number)
}

/// Resolves a face texture (`#all` or a concrete key) to a concrete key by
/// following `#` references transitively.
fn resolve_texture_value(
    textures: &BTreeMap<String, String>,
    value: &str,
    model: &str,
) -> ModelResult<String> {
    let mut visited = BTreeSet::new();
    let mut current = value;
    loop {
        let Some(reference) = current.strip_prefix('#') else {
            let concrete = strip_namespace(current);
            if concrete.is_empty() {
                return Err(ModelError::UnresolvedTexture {
                    reference: value.to_string(),
                    path: model_path(model),
                });
            }
            return Ok(concrete.to_string());
        };
        if !visited.insert(reference.to_string()) {
            return Err(ModelError::UnresolvedTexture {
                reference: format!("#{reference}"),
                path: model_path(model),
            });
        }
        current = textures
            .get(reference)
            .ok_or_else(|| ModelError::UnresolvedTexture {
                reference: format!("#{reference}"),
                path: model_path(model),
            })?;
    }
}

/// Strips the `minecraft:` namespace from a texture or model name.
fn strip_namespace(value: &str) -> &str {
    value.strip_prefix("minecraft:").unwrap_or(value)
}

/// Normalizes a model reference to a key below `models/block`.
fn normalize_model_name(raw: &str) -> String {
    let trimmed = raw.trim();
    let trimmed = strip_namespace(trimmed);
    let trimmed = trimmed.strip_prefix("block/").unwrap_or(trimmed);
    trimmed.strip_suffix(".json").unwrap_or(trimmed).to_string()
}

/// Normalizes a blockstate reference to a key below `blockstates`.
fn normalize_blockstate_name(raw: &str) -> String {
    let trimmed = raw.trim();
    let trimmed = strip_namespace(trimmed);
    trimmed.strip_suffix(".json").unwrap_or(trimmed).to_string()
}

fn model_path(name: &str) -> String {
    format!("{MODEL_PREFIX}{name}.json")
}

fn blockstate_path(name: &str) -> String {
    format!("{BLOCKSTATE_PREFIX}{name}.json")
}

fn invalid(path: &str, message: impl Into<String>) -> ModelError {
    ModelError::Invalid {
        path: path.to_string(),
        message: message.into(),
    }
}

fn resource(path: &str, message: impl Into<String>) -> ModelError {
    ModelError::Resource {
        path: path.to_string(),
        message: message.into(),
    }
}

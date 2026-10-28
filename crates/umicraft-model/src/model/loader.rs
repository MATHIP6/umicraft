// SPDX-License-Identifier: MIT OR Apache-2.0

//! Model loading, `parent` merging and baking, shared by blocks and items.
//!
//! A model is addressed by its key below `assets/minecraft/models/`
//! (`block/cube_all`, `item/iron_sword`). `builtin/*` parents have no file:
//! they are markers (MCP `ModelBakery` special-cases `builtin/generated`,
//! `builtin/entity` and `builtin/missing`), kept on the merged model.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use crate::display::DisplayTransforms;
use crate::error::{ModelError, ModelResult};
use crate::geometry::{bake_quad, ModelRotation, QuadSpec};

use super::document::{invalid, model_path, resolve_texture_value, resource, Element, RawModel};
use super::{ResolvedModel, ResourceSource};

/// Virtual prefix of the model documents in a client jar.
pub(crate) const MODELS_ROOT: &str = "assets/minecraft/models/";
/// Hard limit for the number of models in a `parent` chain; real 1.8 chains
/// are shorter than ten.
const MAX_PARENT_DEPTH: usize = 64;

/// The renderer a `builtin/*` ancestor asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    /// `builtin/generated`: the quads are extruded from the `layerN` sprites.
    Generated,
    /// `builtin/entity`: drawn by a tile-entity renderer (chest, banner, head).
    Entity,
}

/// Reads and merges model documents from a [`ResourceSource`].
pub(crate) struct ModelLoader<'a> {
    source: &'a dyn ResourceSource,
    pub(crate) raw_models: HashMap<String, RawModel>,
    merged_models: HashMap<String, Arc<MergedModel>>,
}

impl<'a> ModelLoader<'a> {
    pub(crate) fn new(source: &'a dyn ResourceSource) -> Self {
        Self {
            source,
            raw_models: HashMap::new(),
            merged_models: HashMap::new(),
        }
    }

    /// Parses every model document below `prefix` (a path under
    /// [`MODELS_ROOT`]) for validation and counts it.
    pub(crate) fn load_listed(&mut self, prefix: &str) -> ModelResult<()> {
        let mut paths = self.source.list(prefix);
        paths.sort();
        paths.dedup();
        for path in paths {
            if !path.ends_with(".json") {
                continue;
            }
            let Some(key) = path
                .strip_prefix(MODELS_ROOT)
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

    /// Resolves every listed model's parent chain so cycles and missing
    /// parents are reported even for models no blockstate currently uses.
    pub(crate) fn validate_parent_chains(&mut self) -> ModelResult<()> {
        let names: Vec<String> = self.raw_models.keys().cloned().collect();
        for name in names {
            self.merged(&name, &mut Vec::new())?;
        }
        Ok(())
    }

    pub(crate) fn load_raw_model(&mut self, name: &str) -> ModelResult<()> {
        if self.raw_models.contains_key(name) {
            return Ok(());
        }
        let path = model_path(name);
        let bytes = self.source.read(&path)?;
        let raw = RawModel::parse(&bytes, &path)?;
        self.raw_models.insert(name.to_string(), raw);
        Ok(())
    }

    /// Merges one model with its ancestors, caching the result by model name.
    pub(crate) fn merged(
        &mut self,
        name: &str,
        stack: &mut Vec<String>,
    ) -> ModelResult<Arc<MergedModel>> {
        if let Some(cached) = self.merged_models.get(name) {
            return Ok(Arc::clone(cached));
        }
        if let Some(builtin) = builtin_marker(name) {
            return Ok(Arc::new(MergedModel::builtin(builtin)));
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
        let parent = match &raw.parent {
            Some(parent_name) => Some(self.merged(parent_name, stack)?),
            None => None,
        };
        stack.pop();

        let merged = Arc::new(MergedModel::inherit(raw, parent.as_deref()));
        // The cached suffix of a parent chain must not hide an over-long
        // chain, so the depth is carried on the merged model itself.
        if merged.depth > MAX_PARENT_DEPTH {
            return Err(ModelError::ParentDepth {
                path: name.to_string(),
                limit: MAX_PARENT_DEPTH,
            });
        }
        self.merged_models
            .insert(name.to_string(), Arc::clone(&merged));
        Ok(merged)
    }

    /// Bakes every element face of `name` with the given variant rotation.
    pub(crate) fn bake(
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

/// A model after parent merging.
#[derive(Debug)]
pub(crate) struct MergedModel {
    pub(crate) textures: BTreeMap<String, String>,
    pub(crate) elements: Option<Arc<Vec<Element>>>,
    pub(crate) ambient_occlusion: bool,
    pub(crate) display: DisplayTransforms,
    pub(crate) builtin: Option<Builtin>,
    /// Number of `parent` links between this model and the root model.
    depth: usize,
}

impl MergedModel {
    fn builtin(builtin: Builtin) -> Self {
        Self {
            textures: BTreeMap::new(),
            elements: None,
            ambient_occlusion: true,
            display: DisplayTransforms::default(),
            builtin: Some(builtin),
            depth: 0,
        }
    }

    /// Applies a model document on top of its merged parent: textures and
    /// display transforms override per key (MCP `ModelBlock`), elements and
    /// the built-in marker are replaced or inherited as a whole.
    fn inherit(raw: RawModel, parent: Option<&Self>) -> Self {
        let Some(parent) = parent else {
            return Self {
                textures: raw.textures,
                elements: raw.elements.map(Arc::new),
                ambient_occlusion: raw.ambient_occlusion.unwrap_or(true),
                display: raw.display,
                builtin: None,
                depth: 1,
            };
        };
        let mut textures = parent.textures.clone();
        textures.extend(raw.textures);
        Self {
            textures,
            elements: raw
                .elements
                .map(Arc::new)
                .or_else(|| parent.elements.clone()),
            ambient_occlusion: raw.ambient_occlusion.unwrap_or(parent.ambient_occlusion),
            display: raw.display.over(&parent.display),
            builtin: parent.builtin,
            depth: parent.depth + 1,
        }
    }
}

/// The marker model for a `builtin/*` name, `None` for any other name.
fn builtin_marker(name: &str) -> Option<Builtin> {
    match name.strip_prefix("builtin/")? {
        "generated" => Some(Builtin::Generated),
        "entity" => Some(Builtin::Entity),
        _ => None,
    }
}

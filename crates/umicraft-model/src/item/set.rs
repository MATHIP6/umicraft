// SPDX-License-Identifier: MIT OR Apache-2.0

//! The baked item model set: every model the 1.8 client registers.
//!
//! Mirrors MCP `ItemModelMesher`: a model is found by `(item id, metadata)`
//! in the name table, or by code for the five mesh-definition items, and the
//! name selects `models/item/<name>.json`.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use crate::display::{DisplayContext, DisplayTransforms, ItemTransform};
use crate::error::ModelResult;
use crate::geometry::{ModelQuad, ModelRotation};
use crate::model::loader::{bake_elements, Builtin, MergedModel, ModelLoader};
use crate::model::ResourceSource;

use super::generated::{generate_elements, SpriteMask, MAX_LAYERS};
use super::names::ITEM_MODEL_NAMES;
use super::special::{special_model_name, SPECIAL_MODEL_NAMES};

/// How an item model is drawn.
#[derive(Debug, Clone, PartialEq)]
pub enum ItemGeometry {
    /// Baked quads in model space (`0.0..=1.0`), the common case.
    Quads(Vec<ModelQuad>),
    /// `builtin/entity`: drawn by a dedicated renderer (chest, banner, skull).
    Entity,
}

/// One baked item model.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemModel {
    /// The geometry.
    pub geometry: ItemGeometry,
    /// The poses per [`DisplayContext`].
    pub display: DisplayTransforms,
    /// Whether the GUI draws it as a lit 3D object (blocks) instead of a flat
    /// sprite (`ModelBlock.gui3d`, false for `builtin/generated`).
    pub gui_3d: bool,
}

impl ItemModel {
    /// The pose of this model in `context`.
    #[must_use]
    pub fn transform(&self, context: DisplayContext) -> ItemTransform {
        self.display.get(context)
    }
}

/// Looks up a sprite's visible pixels by texture key (`items/iron_sword`).
pub type SpriteLookup<'a> = &'a dyn Fn(&str) -> Option<SpriteMask>;

/// All baked item models, addressed like the vanilla mesher.
#[derive(Debug, Default)]
pub struct ItemModelSet {
    by_name: HashMap<&'static str, Arc<ItemModel>>,
    texture_keys: BTreeSet<String>,
}

impl ItemModelSet {
    /// Parses and bakes every item model of the name table.
    ///
    /// `sprites` describes the silhouette of each `builtin/generated` layer;
    /// an unknown texture is treated as fully opaque.
    ///
    /// # Errors
    /// [`crate::ModelError`] for an unreadable or malformed model, an
    /// unresolved texture or a parent cycle.
    pub fn build(source: &dyn ResourceSource, sprites: SpriteLookup<'_>) -> ModelResult<Self> {
        let mut loader = ModelLoader::new(source);
        let mut set = Self::default();
        let names = ITEM_MODEL_NAMES
            .iter()
            .map(|&(_, _, name)| name)
            .chain(SPECIAL_MODEL_NAMES);
        for name in names {
            if !set.by_name.contains_key(name) {
                let model = set.bake(&mut loader, name, sprites)?;
                set.by_name.insert(name, Arc::new(model));
            }
        }
        Ok(set)
    }

    fn bake(
        &mut self,
        loader: &mut ModelLoader<'_>,
        name: &str,
        sprites: SpriteLookup<'_>,
    ) -> ModelResult<ItemModel> {
        let key = format!("item/{name}");
        let merged = loader.merged(&key, &mut Vec::new())?;
        let own_gui_3d = loader.raw_models.get(&key).is_none_or(|raw| raw.gui3d);
        let geometry = match merged.builtin {
            Some(Builtin::Entity) => ItemGeometry::Entity,
            Some(Builtin::Generated) => {
                ItemGeometry::Quads(self.bake_generated(&merged, &key, sprites)?)
            }
            None => ItemGeometry::Quads(loader.bake(&key, ModelRotation::new(0, 0), false)?.quads),
        };
        if let ItemGeometry::Quads(quads) = &geometry {
            self.texture_keys
                .extend(quads.iter().map(|quad| quad.texture.clone()));
        }
        Ok(ItemModel {
            geometry,
            display: merged.display.clone(),
            gui_3d: own_gui_3d && merged.builtin != Some(Builtin::Generated),
        })
    }

    /// Extrudes the `layerN` sprites of a `builtin/generated` model.
    fn bake_generated(
        &self,
        merged: &MergedModel,
        key: &str,
        sprites: SpriteLookup<'_>,
    ) -> ModelResult<Vec<ModelQuad>> {
        let masks: Vec<SpriteMask> = (0..MAX_LAYERS)
            .map_while(|index| merged.textures.get(&format!("layer{index}")))
            .map(|texture| layer_mask(texture, &merged.textures, key, sprites))
            .collect::<ModelResult<_>>()?;
        let elements = generate_elements(&masks);
        bake_elements(
            &elements,
            &merged.textures,
            key,
            ModelRotation::new(0, 0),
            false,
        )
    }

    /// The model registered for an item and its damage value.
    ///
    /// Applies the mesher's rules: a damageable item ignores its damage
    /// (`ItemModelMesher.getMetadata`, pass `damageable`), the five
    /// mesh-definition items choose by code, and anything else is looked up
    /// by `(id, metadata)`. `None` stands for the vanilla missing model.
    #[must_use]
    pub fn model(&self, item_id: u16, damage: u16, damageable: bool) -> Option<&ItemModel> {
        let name = special_model_name(item_id, damage)
            .or_else(|| table_name(item_id, if damageable { 0 } else { damage }))?;
        self.by_name.get(name).map(Arc::as_ref)
    }

    /// The model with a given name, e.g. the in-use `bow_pulling_1`.
    #[must_use]
    pub fn model_by_name(&self, name: &str) -> Option<&ItemModel> {
        self.by_name.get(name).map(Arc::as_ref)
    }

    /// Concrete texture keys referenced by any baked quad.
    #[must_use]
    pub fn texture_keys(&self) -> &BTreeSet<String> {
        &self.texture_keys
    }

    /// Number of distinct baked models.
    #[must_use]
    pub fn model_count(&self) -> usize {
        self.by_name.len()
    }
}

/// The silhouette of the sprite a `layerN` entry resolves to.
fn layer_mask(
    texture: &str,
    textures: &std::collections::BTreeMap<String, String>,
    key: &str,
    sprites: SpriteLookup<'_>,
) -> ModelResult<SpriteMask> {
    let resolved = crate::model::document::resolve_texture_value(textures, texture, key)?;
    Ok(sprites(&resolved).unwrap_or_else(|| SpriteMask::opaque(16, 16)))
}

fn table_name(item_id: u16, meta: u16) -> Option<&'static str> {
    let index = ITEM_MODEL_NAMES
        .binary_search_by_key(&(item_id, meta), |&(id, meta, _)| (id, meta))
        .ok()?;
    Some(ITEM_MODEL_NAMES[index].2)
}

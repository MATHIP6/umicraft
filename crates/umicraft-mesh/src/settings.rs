// SPDX-License-Identifier: MIT OR Apache-2.0

//! The video options that change what the mesher emits: `Smooth Lighting`,
//! `Other Block Variants` (`GameSettings.allowBlockAlternatives`) and the
//! OptiFine-style trees, biome colour, Better Grass and Better Snow options
//! (ADR-0089).

use umicraft_model::{BlockModel, ModelVariant};

use crate::tint::TintMode;
use crate::Lighting;

/// How leaf blocks are meshed (OptiFine "Trees", `BlockLeaves` / `SmartLeaves`).
///
/// The mesher keeps the leaf texture cut out in every mode: opaque Fast
/// leaves (transparent texels drawn solid) are a shader concern and not part
/// of the geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leaves {
    /// Faces between two leaf blocks are culled (`isCullFacesLeaves` with
    /// Fast graphics).
    Fast,
    /// Cut-out leaves with the faces between two leaf blocks culled.
    Smart,
    /// Cut-out leaves, every face drawn (vanilla fancy leaves).
    Fancy,
}

/// OptiFine "Better Grass": grass-like side faces show the top texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BetterGrass {
    /// Vanilla side textures.
    Off,
    /// Every side shows the top texture.
    Fast,
    /// Only a side whose neighbour column continues the same block below
    /// shows the top texture.
    Fancy,
}

/// OptiFine "Connected Textures": block faces take the tiles of the pack's
/// CTM rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectedTextures {
    /// The vanilla textures.
    Off,
    /// Rules apply; the 47-tile method tests only the four edge neighbours.
    Fast,
    /// Rules apply; the 47-tile method also tests the four corners.
    Fancy,
}

/// What the player's options ask of the mesher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshSettings {
    /// How quads are lit (`Smooth Lighting`).
    pub lighting: Lighting,
    /// Whether a state with several weighted variants picks one by block
    /// position (`allowBlockAlternatives`, "Other Block Variants"). Off, every
    /// block of the state uses the same variant, see [`base_variant`].
    pub block_alternatives: bool,
    /// How leaves are culled ([`Leaves`]).
    pub leaves: Leaves,
    /// Whether the swamp tints grass, foliage and water its own way
    /// (`Config.isSwampColors`).
    pub swamp_colors: bool,
    /// Whether biome colours are averaged over 3×3 columns
    /// (`Config.isSmoothBiomes`).
    pub smooth_biomes: bool,
    /// Better Grass mode.
    pub better_grass: BetterGrass,
    /// Whether plants and fences next to snow layers get a snow layer of
    /// their own (`BetterSnow`).
    pub better_snow: bool,
    /// Connected Textures mode.
    pub connected_textures: ConnectedTextures,
    /// Whether faces of textures with a natural rule turn or mirror
    /// (`Config.isNaturalTextures`).
    pub natural_textures: bool,
}

impl MeshSettings {
    /// The biome-colour switches, for [`crate::tint`].
    pub(crate) const fn tint_mode(self) -> TintMode {
        TintMode {
            swamp_colors: self.swamp_colors,
            smooth_biomes: self.smooth_biomes,
        }
    }
}

impl Default for MeshSettings {
    /// The reference (vanilla) defaults: smooth lighting, block alternatives,
    /// swamp colours and smooth biomes on, fancy leaves, no Better Grass or
    /// Better Snow. The client picks its own defaults (Smart trees).
    fn default() -> Self {
        Self {
            lighting: Lighting::Smooth,
            block_alternatives: true,
            leaves: Leaves::Fancy,
            swamp_colors: true,
            smooth_biomes: true,
            better_grass: BetterGrass::Off,
            better_snow: false,
            connected_textures: ConnectedTextures::Off,
            natural_textures: false,
        }
    }
}

/// The variant `WeightedBakedModel` falls back to when alternatives are off:
/// its `baseModel`, the first of the models sorted by descending weight, then
/// by ascending quad count (`MyWeighedRandomItem.compareTo`); `Collections.sort`
/// is stable, so a full tie keeps the document order.
#[must_use]
pub(crate) fn base_variant(model: &BlockModel) -> Option<&ModelVariant> {
    let key =
        |variant: &ModelVariant| (std::cmp::Reverse(variant.weight), variant.model.quads.len());
    model
        .variants
        .iter()
        .fold(None, |best: Option<&ModelVariant>, candidate| match best {
            Some(best) if key(best) <= key(candidate) => Some(best),
            _ => Some(candidate),
        })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use umicraft_model::{ModelQuad, ResolvedModel};

    use super::*;

    fn quad() -> ModelQuad {
        ModelQuad {
            positions: [[0.0; 3]; 4],
            normal: [0.0, 1.0, 0.0],
            uvs: [[0.0; 2]; 4],
            texture: "blocks/stone".to_string(),
            cullface: None,
            shade: true,
            tint: None,
        }
    }

    fn variant(weight: u32, quads: usize) -> ModelVariant {
        let model = ResolvedModel {
            quads: vec![quad(); quads],
            ..ResolvedModel::default()
        };
        ModelVariant {
            model: Arc::new(model),
            weight,
        }
    }

    fn model(variants: Vec<ModelVariant>) -> BlockModel {
        BlockModel { variants }
    }

    #[test]
    fn the_heaviest_variant_is_the_base() {
        let set = model(vec![variant(1, 6), variant(3, 6), variant(2, 6)]);
        assert_eq!(base_variant(&set).map(|v| v.weight), Some(3));
    }

    #[test]
    fn a_weight_tie_prefers_fewer_quads_then_document_order() {
        let set = model(vec![variant(2, 6), variant(2, 4), variant(2, 4)]);
        let base = base_variant(&set).expect("a variant");
        assert!(
            std::ptr::eq(base, &set.variants[1]),
            "first of the 4-quad ties"
        );
    }

    #[test]
    fn an_empty_model_has_no_base() {
        assert!(base_variant(&model(vec![])).is_none());
    }
}

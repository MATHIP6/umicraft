// SPDX-License-Identifier: MIT OR Apache-2.0

//! The video options that change what the mesher emits: `Smooth Lighting`
//! and `Other Block Variants` (`GameSettings.allowBlockAlternatives`).

use umicraft_model::{BlockModel, ModelVariant};

use crate::Lighting;

/// What the player's options ask of the mesher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshSettings {
    /// How quads are lit (`Smooth Lighting`).
    pub lighting: Lighting,
    /// Whether a state with several weighted variants picks one by block
    /// position (`allowBlockAlternatives`, "Other Block Variants"). Off, every
    /// block of the state uses the same variant, see [`base_variant`].
    pub block_alternatives: bool,
}

impl Default for MeshSettings {
    /// The reference defaults: smooth lighting and block alternatives on.
    fn default() -> Self {
        Self {
            lighting: Lighting::Smooth,
            block_alternatives: true,
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

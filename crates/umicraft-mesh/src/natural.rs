// SPDX-License-Identifier: MIT OR Apache-2.0

//! Natural textures in the model mesher (OptiFine `NaturalTextures`,
//! ADR-0088/0089).
//!
//! [`NaturalTable::build`] resolves the texture names of `natural.properties`
//! to atlas layers once per resource load. The mesher then calls
//! [`natural_uvs`] for each quad: a direct index by the quad's layer decides
//! whether the texture has a rule, so a quad of any other texture costs one
//! bounds-checked load. The rule runs after connected textures, on the layer
//! the quad shows by then.

use umicraft_model::{AtlasLayout, ModelQuad};
use umicraft_packext::natural::{Natural, NaturalRules};

use crate::ctm::{quad_face, side_of};

/// The natural-texture kind of each atlas layer. Immutable and shared by every
/// meshing worker.
#[derive(Debug, Clone)]
pub struct NaturalTable {
    by_layer: Vec<Option<Natural>>,
    rules: usize,
}

impl NaturalTable {
    /// Resolves `rules` against `atlas`. A name that is not a block texture of
    /// the atlas is skipped, as OptiFine warns and moves on. `None` when no
    /// rule is left, so a table always has work to do.
    #[must_use]
    pub fn build(rules: &NaturalRules, atlas: &AtlasLayout) -> Option<Self> {
        let mut by_layer = vec![None; atlas.layer_count() as usize];
        let mut count = 0;
        for (name, natural) in &rules.entries {
            let Some(layer) = atlas.layer(&format!("blocks/{name}")) else {
                tracing::warn!(texture = %name, "natural textures: texture not found");
                continue;
            };
            if let Some(slot) = by_layer.get_mut(layer as usize) {
                *slot = Some(*natural);
                count += 1;
            }
        }
        (count > 0).then_some(Self {
            by_layer,
            rules: count,
        })
    }

    /// The number of textures with a rule.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rules
    }

    /// Always `false`: a table exists only when it has a rule.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules == 0
    }

    fn rule(&self, layer: u32) -> Option<Natural> {
        self.by_layer.get(layer as usize).copied().flatten()
    }
}

/// The texture coordinates `quad` should use, given the `layer` it shows and
/// the block `position`. Returns the quad's own coordinates when the layer has
/// no rule.
pub(crate) fn natural_uvs(
    table: &NaturalTable,
    position: [i32; 3],
    quad: &ModelQuad,
    layer: u32,
) -> [[f32; 2]; 4] {
    let Some(natural) = table.rule(layer) else {
        return quad.uvs;
    };
    let side = quad_face(quad).map_or(-1, |face| side_of(face) as i32);
    natural.variant(position, side).apply(quad.uvs)
}

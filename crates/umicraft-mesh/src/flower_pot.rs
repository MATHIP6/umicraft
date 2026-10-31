// SPDX-License-Identifier: MIT OR Apache-2.0

//! The plant in a flower pot (P7d).
//!
//! In 1.8 the pot's contents live in the tile entity, and
//! `BlockFlowerPot.getActualState` turns them into the `contents` blockstate
//! property. The chunk mesher has no tile entity, so it meshes the empty pot
//! and the client adds the plant here, once per frame, through the same
//! dynamic terrain path as the blocks a piston moves.
//!
//! The `contents=<plant>` models are the whole pot plus the plant. The pot
//! parts are already in the chunk mesh, so they are dropped here: drawing the
//! coplanar faces twice would z-fight with a different light.

use std::sync::Arc;

use umicraft_model::{ModelVariant, ResolvedModel};
use umicraft_world::{BlockState, LightLevel};

use crate::models::select_variant;
use crate::moving::push_variant;
use crate::{Mesh, ModelMeshContext};

/// Block id of the flower pot.
const FLOWER_POT: u16 = 140;
/// The textures of the pot itself in every `contents` model.
const POT_TEXTURES: [&str; 2] = ["blocks/flower_pot", "blocks/dirt"];

/// Appends the plant of the `contents` model (`rose`, `cactus`, ...) to `mesh`
/// for the pot at `position`. An unknown contents name adds nothing.
pub fn push_flower_pot_plant(
    mesh: &mut Mesh,
    contents: &str,
    position: [i32; 3],
    light: LightLevel,
    context: &ModelMeshContext,
) {
    let key = format!("contents={contents}");
    let Some(model) = context.models.model_by_key("flower_pot", &key) else {
        return;
    };
    let Some(variant) = select_variant(model, position) else {
        return;
    };
    let plant = ModelVariant {
        model: Arc::new(ResolvedModel {
            quads: variant
                .model
                .quads
                .iter()
                .filter(|quad| !POT_TEXTURES.contains(&quad.texture.as_str()))
                .cloned()
                .collect(),
            ambient_occlusion: variant.model.ambient_occlusion,
            particle: None,
        }),
        weight: variant.weight,
    };
    let state = BlockState::new(FLOWER_POT, 0);
    push_variant(mesh, &plant, state, position, [0.0; 3], light, context);
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use umicraft_model::{
        AtlasLayout, BlockDefinition, BlockModelSet, ModelError, ModelResult, ResourceSource,
        StateDef,
    };

    use super::*;

    struct Memory(BTreeMap<String, Vec<u8>>);

    impl ResourceSource for Memory {
        fn read(&self, path: &str) -> ModelResult<Vec<u8>> {
            self.0
                .get(path)
                .cloned()
                .ok_or_else(|| ModelError::Resource {
                    path: path.to_string(),
                    message: "synthetic resource is missing".to_string(),
                })
        }

        fn list(&self, prefix: &str) -> Vec<String> {
            self.0
                .keys()
                .filter(|path| path.starts_with(prefix))
                .cloned()
                .collect()
        }
    }

    /// A pot made of one pot face and one dirt face, plus a one-face plant.
    fn context() -> ModelMeshContext {
        static STATES: [StateDef; 1] = [StateDef {
            mask: 0,
            value: 0,
            blockstate: "flower_pot",
            variant: "contents=empty",
        }];
        static DEFS: [BlockDefinition; 1] = [BlockDefinition {
            id: 140,
            name: "flower_pot",
            states: &STATES,
        }];
        let face = |texture: &str| {
            format!(
                r##"{{ "from": [0, 0, 0], "to": [16, 4, 16],
                    "faces": {{ "up": {{ "texture": "{texture}" }} }} }}"##
            )
        };
        let model = format!(
            r#"{{ "textures": {{ "particle": "blocks/flower_pot" }},
                "elements": [{}, {}, {}] }}"#,
            face("blocks/flower_pot"),
            face("blocks/dirt"),
            face("blocks/flower_rose")
        );
        let mut files = BTreeMap::new();
        files.insert(
            "assets/minecraft/models/block/pot.json".into(),
            model.into(),
        );
        files.insert(
            "assets/minecraft/blockstates/flower_pot.json".into(),
            br#"{ "variants": { "contents=empty": { "model": "pot" },
                "contents=rose": { "model": "pot" } } }"#
                .to_vec(),
        );
        let set = BlockModelSet::build(&Memory(files), &DEFS).expect("synthetic pot");
        ModelMeshContext {
            models: Arc::new(set),
            atlas: Arc::new(AtlasLayout::from_keys(["blocks/flower_rose"])),
            tints: None,
        }
    }

    #[test]
    fn only_the_plant_faces_of_the_model_are_drawn() {
        let mut mesh = Mesh::default();
        push_flower_pot_plant(
            &mut mesh,
            "rose",
            [3, 64, 3],
            LightLevel::FULL_SKY,
            &context(),
        );
        assert_eq!(mesh.opaque_face_count(), 1);
    }

    #[test]
    fn unknown_contents_add_nothing() {
        let mut mesh = Mesh::default();
        push_flower_pot_plant(
            &mut mesh,
            "diamond",
            [0, 0, 0],
            LightLevel::FULL_SKY,
            &context(),
        );
        assert!(mesh.is_empty());
    }
}

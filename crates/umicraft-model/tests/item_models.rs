// SPDX-License-Identifier: MIT OR Apache-2.0

//! Item model set tests on synthetic documents (no Mojang assets).

use std::collections::BTreeMap;

use umicraft_model::item::{ItemGeometry, ItemModelSet, SpriteMask};
use umicraft_model::{display::DisplayContext, ModelError, ModelResult, ResourceSource};

/// Serves the documents it knows and a plain generated item for any other
/// `models/item/*` path, so the full name table can be baked.
struct Source(BTreeMap<&'static str, &'static str>);

const FALLBACK: &str = r#"{"parent": "item/generated", "textures": {"layer0": "items/fallback"}}"#;

impl ResourceSource for Source {
    fn read(&self, path: &str) -> ModelResult<Vec<u8>> {
        let key = path
            .strip_prefix("assets/minecraft/models/")
            .and_then(|rest| rest.strip_suffix(".json"));
        let document = key.and_then(|key| match self.0.get(key) {
            Some(document) => Some(*document),
            None => key.starts_with("item/").then_some(FALLBACK),
        });
        document
            .map(|document| document.as_bytes().to_vec())
            .ok_or_else(|| ModelError::Resource {
                path: path.to_string(),
                message: "missing test resource".to_string(),
            })
    }

    fn list(&self, _prefix: &str) -> Vec<String> {
        Vec::new()
    }
}

fn source() -> Source {
    Source(BTreeMap::from([
        (
            "item/generated",
            r#"{"parent": "builtin/generated",
                "display": {"gui": {"rotation": [1, 2, 3]}, "ground": {"scale": [0.5, 0.5, 0.5]}}}"#,
        ),
        (
            "item/handheld",
            r#"{"parent": "item/generated",
                "display": {"gui": {"rotation": [9, 9, 9]}}}"#,
        ),
        (
            "item/iron_sword",
            r#"{"parent": "item/handheld", "textures": {"layer0": "items/iron_sword"}}"#,
        ),
        (
            "item/stone",
            r#"{"parent": "block/stone",
                "display": {"gui": {"rotation": [30, 225, 0]}}}"#,
        ),
        (
            "block/stone",
            r##"{"textures": {"all": "blocks/stone"},
                "elements": [{"from": [0, 0, 0], "to": [16, 16, 16],
                  "faces": {"down": {"texture": "#all"}, "up": {"texture": "#all"}}}]}"##,
        ),
        ("item/chest", r#"{"parent": "builtin/entity"}"#),
        // The real jar parents the compass and the clock on these markers.
        (
            "item/compass",
            r#"{"parent": "builtin/compass", "textures": {"layer0": "items/compass"}}"#,
        ),
    ]))
}

fn sprites(key: &str) -> Option<SpriteMask> {
    (key == "items/iron_sword").then(|| {
        let mut rgba = vec![0_u8; 16 * 16 * 4];
        rgba[(8 * 16 + 8) * 4 + 3] = 255;
        SpriteMask::from_rgba(16, 16, &rgba).expect("16x16")
    })
}

fn set() -> ItemModelSet {
    ItemModelSet::build(&source(), &sprites).expect("builds")
}

#[test]
fn a_generated_item_is_extruded_from_its_sprite() {
    let set = set();
    let sword = set.model(267, 0, true).expect("iron sword");
    let ItemGeometry::Quads(quads) = &sword.geometry else {
        panic!("generated items have quads");
    };
    // front and back planes plus the four sides of the single pixel
    assert_eq!(quads.len(), 6);
    assert!(quads.iter().all(|quad| quad.texture == "items/iron_sword"));
    assert!(!sword.gui_3d);
}

#[test]
fn display_poses_come_from_the_nearest_ancestor_that_sets_them() {
    let set = set();
    let sword = set.model(267, 0, true).expect("iron sword");
    assert_eq!(
        sword.transform(DisplayContext::Gui).rotation,
        [9.0, 9.0, 9.0]
    );
    assert_eq!(sword.transform(DisplayContext::Ground).scale, [0.5; 3]);
}

#[test]
fn a_block_item_uses_the_block_elements_and_a_lit_gui() {
    let set = set();
    let stone = set.model(1, 0, false).expect("stone");
    let ItemGeometry::Quads(quads) = &stone.geometry else {
        panic!("block items have quads");
    };
    assert_eq!(quads.len(), 2);
    assert!(stone.gui_3d);
    assert!(set.texture_keys().contains("blocks/stone"));
}

#[test]
fn builtin_entity_models_have_no_quads() {
    let set = set();
    let chest = set.model(54, 0, false).expect("chest");
    assert_eq!(chest.geometry, ItemGeometry::Entity);
}

#[test]
fn metadata_picks_the_variant_unless_the_item_is_damageable() {
    let set = set();
    assert!(set.model(35, 14, false).is_some(), "red wool");
    assert!(set.model(35, 99, false).is_none(), "unregistered metadata");
    // A damaged sword still shows the sword model.
    assert!(set.model(267, 57, true).is_some());
    assert!(set.model(267, 57, false).is_none());
}

#[test]
fn mesh_definition_items_choose_by_code() {
    let set = set();
    assert!(set.model(373, 0, false).is_some(), "potion");
    assert!(set.model(373, 16384, false).is_some(), "splash potion");
    assert!(set.model_by_name("bow_pulling_2").is_some());
}

#[test]
fn a_missing_model_document_is_a_resource_error() {
    struct Empty;
    impl ResourceSource for Empty {
        fn read(&self, path: &str) -> ModelResult<Vec<u8>> {
            Err(ModelError::Resource {
                path: path.to_string(),
                message: "absent".to_string(),
            })
        }
        fn list(&self, _prefix: &str) -> Vec<String> {
            Vec::new()
        }
    }
    let error = ItemModelSet::build(&Empty, &|_| None).expect_err("must fail");
    assert!(matches!(error, ModelError::Resource { .. }));
}

#[test]
fn the_compass_is_generated_like_a_flat_item() {
    let set = set();
    let compass = set.model(345, 0, false).expect("compass");
    let ItemGeometry::Quads(quads) = &compass.geometry else {
        panic!("the compass is extruded from its sprite");
    };
    assert!(!quads.is_empty());
    assert!(!compass.gui_3d);
}

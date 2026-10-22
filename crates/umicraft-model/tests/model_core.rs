// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration tests for the pure model core: blockstate parsing, parent and
//! texture resolution, the two rotation stages and the baked quad rules.
//!
//! Every fixture is synthetic JSON assembled in this file. No Mojang asset is
//! copied into the repository (ADR-0007, spec §3).

use std::collections::BTreeMap;

use serde_json::{json, Value};
use umicraft_model::{
    BlockDefinition, BlockModelSet, BlockStateFile, ModelError, ModelQuad, ModelResult,
    ResolvedModel, ResourceSource, StateDef,
};
use umicraft_world::BlockState;

/// In-memory [`ResourceSource`] for synthetic documents.
struct Memory {
    files: BTreeMap<String, Vec<u8>>,
}

impl Memory {
    fn new() -> Self {
        Self {
            files: BTreeMap::new(),
        }
    }

    fn model(mut self, name: &str, value: Value) -> Self {
        self.files.insert(
            format!("assets/minecraft/models/block/{name}.json"),
            value.to_string().into_bytes(),
        );
        self
    }

    fn raw_model(mut self, name: &str, json: &str) -> Self {
        self.files.insert(
            format!("assets/minecraft/models/block/{name}.json"),
            json.as_bytes().to_vec(),
        );
        self
    }

    fn blockstate(mut self, name: &str, value: Value) -> Self {
        self.files.insert(
            format!("assets/minecraft/blockstates/{name}.json"),
            value.to_string().into_bytes(),
        );
        self
    }
}

impl ResourceSource for Memory {
    fn read(&self, path: &str) -> ModelResult<Vec<u8>> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| ModelError::Resource {
                path: path.to_string(),
                message: "synthetic resource is missing".to_string(),
            })
    }

    fn list(&self, prefix: &str) -> Vec<String> {
        self.files
            .keys()
            .filter(|path| path.starts_with(prefix))
            .cloned()
            .collect()
    }
}

fn build(source: Memory, definitions: &[BlockDefinition]) -> ModelResult<BlockModelSet> {
    BlockModelSet::build(&source, definitions)
}

fn cube(texture: &str) -> Value {
    let face = |name: &str| json!({ "texture": "#all", "cullface": name });
    json!({
        "textures": { "particle": "#all", "all": texture },
        "elements": [{
            "from": [0, 0, 0],
            "to": [16, 16, 16],
            "faces": {
                "down": face("down"),
                "up": face("up"),
                "north": face("north"),
                "south": face("south"),
                "west": face("west"),
                "east": face("east")
            }
        }]
    })
}

fn slab(from_y: i32, to_y: i32, texture: &str) -> Value {
    json!({
        "textures": { "particle": "#all", "all": texture },
        "elements": [{
            "from": [0, from_y, 0],
            "to": [16, to_y, 16],
            "faces": {
                "down": { "texture": "#all" },
                "up": { "texture": "#all" },
                "north": { "texture": "#all" },
                "south": { "texture": "#all" },
                "west": { "texture": "#all" },
                "east": { "texture": "#all" }
            }
        }]
    })
}

fn resolved(set: &BlockModelSet, id: u16, metadata: u8) -> &ResolvedModel {
    let block = set
        .model(BlockState::new(id, metadata))
        .expect("the synthetic state was mapped");
    block
        .variants
        .first()
        .expect("a block model has at least one variant")
        .model
        .as_ref()
}

fn quads(set: &BlockModelSet, id: u16, metadata: u8) -> &[ModelQuad] {
    &resolved(set, id, metadata).quads
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 1.0e-5,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn parent_textures_merge_and_child_textures_override() {
    static STATES: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "thing",
        variant: "normal",
    }];
    static DEFS: &[BlockDefinition] = &[BlockDefinition {
        id: 1,
        name: "thing",
        states: STATES,
    }];

    let source = Memory::new()
        .model("base", cube("blocks/stone"))
        .model(
            "child",
            json!({ "parent": "base", "textures": { "all": "blocks/dirt" } }),
        )
        .blockstate(
            "thing",
            json!({ "variants": { "normal": { "model": "child" } } }),
        );

    let set = build(source, DEFS).expect("synthetic model set");
    let model = resolved(&set, 1, 0);
    assert!(!model.quads.is_empty());
    assert!(model.quads.iter().all(|quad| quad.texture == "blocks/dirt"));
    assert_eq!(model.particle.as_deref(), Some("blocks/dirt"));
}

#[test]
fn parent_elements_are_inherited_and_child_elements_replace_them() {
    static STATES_A: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "inherited",
        variant: "normal",
    }];
    static STATES_B: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "replaced",
        variant: "normal",
    }];
    static DEFS: &[BlockDefinition] = &[
        BlockDefinition {
            id: 1,
            name: "inherited",
            states: STATES_A,
        },
        BlockDefinition {
            id: 2,
            name: "replaced",
            states: STATES_B,
        },
    ];

    let source = Memory::new()
        .model("base", cube("blocks/stone"))
        .model("inherited", json!({ "parent": "base" }))
        .model(
            "replaced",
            json!({
                "parent": "base",
                "textures": { "all": "blocks/stone" },
                "elements": [{
                    "from": [0, 0, 0],
                    "to": [16, 8, 16],
                    "faces": {
                        "down": { "texture": "#all" },
                        "up": { "texture": "#all" },
                        "north": { "texture": "#all" },
                        "south": { "texture": "#all" },
                        "west": { "texture": "#all" },
                        "east": { "texture": "#all" }
                    }
                }]
            }),
        )
        .blockstate(
            "inherited",
            json!({ "variants": { "normal": { "model": "inherited" } } }),
        )
        .blockstate(
            "replaced",
            json!({ "variants": { "normal": { "model": "replaced" } } }),
        );

    let set = build(source, DEFS).expect("synthetic model set");
    let inherited = quads(&set, 1, 0);
    assert_eq!(inherited.len(), 6);
    assert!(inherited
        .iter()
        .flat_map(|quad| quad.positions)
        .any(|position| position[1] == 1.0));

    let replaced = quads(&set, 2, 0);
    assert_eq!(replaced.len(), 6);
    assert!(replaced
        .iter()
        .flat_map(|quad| quad.positions)
        .all(|position| position[1] <= 0.5 + 1.0e-6));
}

#[test]
fn parent_cycle_is_a_typed_error() {
    static STATES: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "cycle",
        variant: "normal",
    }];
    static DEFS: &[BlockDefinition] = &[BlockDefinition {
        id: 1,
        name: "cycle",
        states: STATES,
    }];

    let source = Memory::new()
        .model("a", json!({ "parent": "b" }))
        .model("b", json!({ "parent": "a" }))
        .blockstate(
            "cycle",
            json!({ "variants": { "normal": { "model": "a" } } }),
        );

    let error = build(source, DEFS).expect_err("a parent cycle must fail");
    assert!(matches!(error, ModelError::ParentCycle { .. }), "{error:?}");
}

#[test]
fn too_long_parent_chain_is_a_typed_error() {
    static STATES: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "deep",
        variant: "normal",
    }];
    static DEFS: &[BlockDefinition] = &[BlockDefinition {
        id: 1,
        name: "deep",
        states: STATES,
    }];

    let mut source = Memory::new();
    for index in 0..70 {
        source = source.model(
            &format!("m{index}"),
            json!({ "parent": format!("m{}", index + 1) }),
        );
    }
    source = source.model("m70", cube("blocks/stone")).blockstate(
        "deep",
        json!({ "variants": { "normal": { "model": "m0" } } }),
    );

    let error = build(source, DEFS).expect_err("a deep parent chain must fail");
    assert!(matches!(error, ModelError::ParentDepth { .. }), "{error:?}");
}

#[test]
fn unresolved_texture_reference_is_a_typed_error() {
    static STATES: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "broken",
        variant: "normal",
    }];
    static DEFS: &[BlockDefinition] = &[BlockDefinition {
        id: 1,
        name: "broken",
        states: STATES,
    }];

    let source = Memory::new()
        .model(
            "broken",
            json!({
                "elements": [{
                    "from": [0, 0, 0],
                    "to": [16, 16, 16],
                    "faces": { "up": { "texture": "#missing" } }
                }]
            }),
        )
        .blockstate(
            "broken",
            json!({ "variants": { "normal": { "model": "broken" } } }),
        );

    let error = build(source, DEFS).expect_err("an unresolved texture must fail");
    match error {
        ModelError::UnresolvedTexture { reference, .. } => assert_eq!(reference, "#missing"),
        other => panic!("expected UnresolvedTexture, got {other:?}"),
    }
}

#[test]
fn unresolved_particle_texture_is_a_typed_error() {
    static STATES: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "broken_particle",
        variant: "normal",
    }];
    static DEFS: &[BlockDefinition] = &[BlockDefinition {
        id: 1,
        name: "broken_particle",
        states: STATES,
    }];

    let source = Memory::new()
        .model(
            "broken_particle",
            json!({
                "textures": { "particle": "#nope" },
                "elements": [{
                    "from": [0, 0, 0],
                    "to": [16, 16, 16],
                    "faces": { "up": { "texture": "blocks/stone" } }
                }]
            }),
        )
        .blockstate(
            "broken_particle",
            json!({ "variants": { "normal": { "model": "broken_particle" } } }),
        );

    let error = build(source, DEFS).expect_err("an unresolved particle must fail");
    assert!(
        matches!(error, ModelError::UnresolvedTexture { .. }),
        "{error:?}"
    );
}

#[test]
fn listed_models_are_validated_even_when_unused() {
    static DEFS: &[BlockDefinition] = &[];

    let source = Memory::new().raw_model("broken", "{ this is not json");
    let error = build(source, DEFS).expect_err("every listed model is parsed");
    assert!(matches!(error, ModelError::Json { .. }), "{error:?}");
}

#[test]
fn missing_definition_blockstate_is_a_resource_error() {
    static STATES: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "absent",
        variant: "normal",
    }];
    static DEFS: &[BlockDefinition] = &[BlockDefinition {
        id: 1,
        name: "absent",
        states: STATES,
    }];

    let error = build(Memory::new(), DEFS).expect_err("a missing blockstate must fail");
    assert!(matches!(error, ModelError::Resource { .. }), "{error:?}");
}

#[test]
fn cube_has_six_outward_normals_and_top_uv_convention() {
    static STATES: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "cube",
        variant: "normal",
    }];
    static DEFS: &[BlockDefinition] = &[BlockDefinition {
        id: 1,
        name: "cube",
        states: STATES,
    }];

    let source = Memory::new()
        .model("cube", cube("blocks/stone"))
        .blockstate(
            "cube",
            json!({ "variants": { "normal": { "model": "cube" } } }),
        );

    let set = build(source, DEFS).expect("synthetic cube");
    let quads = quads(&set, 1, 0);
    assert_eq!(quads.len(), 6);

    let expected = [
        [-1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, -1.0],
        [0.0, 0.0, 1.0],
    ];
    for normal in expected {
        assert!(
            quads.iter().any(|quad| quad.normal == normal),
            "missing normal {normal:?}: {:?}",
            quads.iter().map(|quad| quad.normal).collect::<Vec<_>>()
        );
    }

    let top = quads
        .iter()
        .find(|quad| quad.normal == [0.0, 1.0, 0.0])
        .expect("top face");
    assert_eq!(
        top.uvs,
        [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
        "v = 0 must be the texture top"
    );
    assert!(top.shade);
}

#[test]
fn cross_has_four_unshaded_quads_rotated_by_45_degrees() {
    static STATES: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "cross",
        variant: "normal",
    }];
    static DEFS: &[BlockDefinition] = &[BlockDefinition {
        id: 1,
        name: "cross",
        states: STATES,
    }];

    let cross = json!({
        "textures": { "particle": "#cross", "cross": "blocks/plant" },
        "elements": [
            {
                "from": [0.8, 0, 8],
                "to": [15.2, 16, 8],
                "rotation": { "origin": [8, 8, 8], "axis": "y", "angle": 45, "rescale": true },
                "shade": false,
                "faces": {
                    "north": { "uv": [0, 0, 16, 16], "texture": "#cross" },
                    "south": { "uv": [0, 0, 16, 16], "texture": "#cross" }
                }
            },
            {
                "from": [8, 0, 0.8],
                "to": [8, 16, 15.2],
                "rotation": { "origin": [8, 8, 8], "axis": "y", "angle": 45, "rescale": true },
                "shade": false,
                "faces": {
                    "west": { "uv": [0, 0, 16, 16], "texture": "#cross" },
                    "east": { "uv": [0, 0, 16, 16], "texture": "#cross" }
                }
            }
        ]
    });
    let source = Memory::new().model("cross", cross).blockstate(
        "cross",
        json!({ "variants": { "normal": { "model": "cross" } } }),
    );

    let set = build(source, DEFS).expect("synthetic cross");
    let quads = quads(&set, 1, 0);
    assert_eq!(quads.len(), 4);
    assert!(quads.iter().all(|quad| !quad.shade));
    assert!(quads.iter().all(|quad| quad.texture == "blocks/plant"));
    assert!(quads
        .iter()
        .any(|quad| { quad.normal[0].abs() > 0.1 && quad.normal[2].abs() > 0.1 }));
    assert!(quads
        .iter()
        .flat_map(|quad| quad.positions)
        .any(|position| {
            (position[0] - 0.05).abs() < 1.0e-5 && (position[2] - 0.05).abs() < 1.0e-5
        }));
}

#[test]
fn bottom_and_top_slabs_stay_on_their_halves() {
    static STATES_A: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "bottom",
        variant: "normal",
    }];
    static STATES_B: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "top",
        variant: "normal",
    }];
    static DEFS: &[BlockDefinition] = &[
        BlockDefinition {
            id: 1,
            name: "bottom",
            states: STATES_A,
        },
        BlockDefinition {
            id: 2,
            name: "top",
            states: STATES_B,
        },
    ];

    let source = Memory::new()
        .model("bottom", slab(0, 8, "blocks/stone_slab_top"))
        .model("top", slab(8, 16, "blocks/stone_slab_top"))
        .blockstate(
            "bottom",
            json!({ "variants": { "normal": { "model": "bottom" } } }),
        )
        .blockstate(
            "top",
            json!({ "variants": { "normal": { "model": "top" } } }),
        );

    let set = build(source, DEFS).expect("synthetic slabs");
    let bottom = quads(&set, 1, 0);
    let top = quads(&set, 2, 0);
    assert_eq!(bottom.len(), 6);
    assert_eq!(top.len(), 6);
    assert!(bottom
        .iter()
        .flat_map(|quad| quad.positions)
        .all(|position| position[1] <= 0.5 + 1.0e-6));
    assert!(top
        .iter()
        .flat_map(|quad| quad.positions)
        .all(|position| position[1] >= 0.5 - 1.0e-6));
}

#[test]
fn stairs_have_more_than_six_quads_and_full_height() {
    static STATES: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "stairs",
        variant: "normal",
    }];
    static DEFS: &[BlockDefinition] = &[BlockDefinition {
        id: 1,
        name: "stairs",
        states: STATES,
    }];

    let stairs = json!({
        "textures": { "particle": "#all", "all": "blocks/stone" },
        "elements": [
            {
                "from": [0, 0, 0],
                "to": [16, 8, 16],
                "faces": {
                    "down": { "texture": "#all" },
                    "up": { "texture": "#all" },
                    "north": { "texture": "#all" },
                    "south": { "texture": "#all" },
                    "west": { "texture": "#all" },
                    "east": { "texture": "#all" }
                }
            },
            {
                "from": [8, 8, 0],
                "to": [16, 16, 16],
                "faces": {
                    "down": { "texture": "#all" },
                    "up": { "texture": "#all" },
                    "north": { "texture": "#all" },
                    "south": { "texture": "#all" },
                    "west": { "texture": "#all" },
                    "east": { "texture": "#all" }
                }
            }
        ]
    });
    let source = Memory::new().model("stairs", stairs).blockstate(
        "stairs",
        json!({ "variants": { "normal": { "model": "stairs" } } }),
    );

    let set = build(source, DEFS).expect("synthetic stairs");
    let quads = quads(&set, 1, 0);
    assert!(quads.len() > 6);
    let max_y = quads
        .iter()
        .flat_map(|quad| quad.positions)
        .map(|position| position[1])
        .fold(0.0_f32, f32::max);
    assert_close(max_y, 1.0);
}

#[test]
fn element_rotation_rescale_expands_the_expected_bounds() {
    static STATES_A: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "plain",
        variant: "normal",
    }];
    static STATES_B: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "rescaled",
        variant: "normal",
    }];
    static DEFS: &[BlockDefinition] = &[
        BlockDefinition {
            id: 1,
            name: "plain",
            states: STATES_A,
        },
        BlockDefinition {
            id: 2,
            name: "rescaled",
            states: STATES_B,
        },
    ];

    let plane = |rescale: bool| {
        json!({
            "textures": { "particle": "#all", "all": "blocks/plant" },
            "elements": [{
                "from": [0.8, 0, 8],
                "to": [15.2, 16, 8],
                "rotation": {
                    "origin": [8, 8, 8],
                    "axis": "y",
                    "angle": 45,
                    "rescale": rescale
                },
                "faces": { "north": { "texture": "#all" } }
            }]
        })
    };
    let source = Memory::new()
        .model("plain", plane(false))
        .model("rescaled", plane(true))
        .blockstate(
            "plain",
            json!({ "variants": { "normal": { "model": "plain" } } }),
        )
        .blockstate(
            "rescaled",
            json!({ "variants": { "normal": { "model": "rescaled" } } }),
        );

    let set = build(source, DEFS).expect("synthetic rotated planes");
    let width = |id| {
        let positions = quads(&set, id, 0)
            .iter()
            .flat_map(|quad| quad.positions)
            .collect::<Vec<_>>();
        let min = positions
            .iter()
            .map(|position| position[0])
            .fold(f32::INFINITY, f32::min);
        let max = positions
            .iter()
            .map(|position| position[0])
            .fold(f32::NEG_INFINITY, f32::max);
        max - min
    };
    let plain = width(1);
    let rescaled = width(2);
    assert!(plain < 0.7, "unrescaled width {plain}");
    assert!(rescaled > 0.85, "rescaled width {rescaled}");
}

#[test]
fn blockstate_y_rotation_rotates_cullfaces_and_positions() {
    static STATES: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "rotated",
        variant: "normal",
    }];
    static DEFS: &[BlockDefinition] = &[BlockDefinition {
        id: 1,
        name: "rotated",
        states: STATES,
    }];

    let source = Memory::new()
        .model("cube", cube("blocks/stone"))
        .blockstate(
            "rotated",
            json!({
                "variants": {
                    "normal": { "model": "cube", "x": 0, "y": 90 }
                }
            }),
        );

    let set = build(source, DEFS).expect("synthetic rotated cube");
    let quads = quads(&set, 1, 0);
    assert_eq!(quads.len(), 6);
    assert_eq!(
        quads
            .iter()
            .filter_map(|quad| quad.cullface)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        6
    );

    let east = quads
        .iter()
        .find(|quad| quad.cullface == Some(umicraft_model::Face::East))
        .expect("the north cullface rotates to east");
    assert_eq!(east.normal, [1.0, 0.0, 0.0]);
    for position in east.positions {
        assert_close(position[0], 1.0);
    }
}

#[test]
fn weighted_variants_preserve_order_and_weights() {
    static STATES: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "weighted",
        variant: "normal",
    }];
    static DEFS: &[BlockDefinition] = &[BlockDefinition {
        id: 1,
        name: "weighted",
        states: STATES,
    }];

    let source = Memory::new()
        .model("a", cube("blocks/stone"))
        .model("b", cube("blocks/dirt"))
        .blockstate(
            "weighted",
            json!({
                "variants": {
                    "normal": [
                        { "model": "a", "weight": 1 },
                        { "model": "b", "weight": 2 }
                    ]
                }
            }),
        );

    let set = build(source, DEFS).expect("synthetic weighted variants");
    let block = set.model(BlockState::new(1, 0)).expect("baked state");
    assert_eq!(block.variants.len(), 2);
    assert_eq!(block.variants[0].weight, 1);
    assert_eq!(block.variants[1].weight, 2);
    assert!(block.variants[0]
        .model
        .quads
        .iter()
        .all(|quad| quad.texture == "blocks/stone"));
    assert!(block.variants[1]
        .model
        .quads
        .iter()
        .all(|quad| quad.texture == "blocks/dirt"));
    assert_eq!(set.model_count(), 2);
}

#[test]
fn face_uv_rotation_90_rotates_the_expected_corners() {
    static STATES_A: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "uv0",
        variant: "normal",
    }];
    static STATES_B: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "uv90",
        variant: "normal",
    }];
    static DEFS: &[BlockDefinition] = &[
        BlockDefinition {
            id: 1,
            name: "uv0",
            states: STATES_A,
        },
        BlockDefinition {
            id: 2,
            name: "uv90",
            states: STATES_B,
        },
    ];

    let face = |rotation: i32| {
        json!({
            "textures": { "particle": "#all", "all": "blocks/stone" },
            "elements": [{
                "from": [0, 0, 0],
                "to": [16, 16, 16],
                "faces": {
                    "up": {
                        "texture": "#all",
                        "uv": [0, 0, 16, 16],
                        "rotation": rotation
                    }
                }
            }]
        })
    };
    let source = Memory::new()
        .model("uv0", face(0))
        .model("uv90", face(90))
        .blockstate(
            "uv0",
            json!({ "variants": { "normal": { "model": "uv0" } } }),
        )
        .blockstate(
            "uv90",
            json!({ "variants": { "normal": { "model": "uv90" } } }),
        );

    let set = build(source, DEFS).expect("synthetic rotated UVs");
    assert_eq!(
        quads(&set, 1, 0)[0].uvs,
        [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]
    );
    assert_eq!(
        quads(&set, 2, 0)[0].uvs,
        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]
    );
}

#[test]
fn build_counts_documents_and_maps_every_metadata_value() {
    static STATES_A: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "alpha",
        variant: "a=1,b=2",
    }];
    static STATES_B: &[StateDef] = &[
        StateDef {
            mask: 3,
            value: 0,
            blockstate: "beta",
            variant: "normal",
        },
        StateDef {
            mask: 3,
            value: 1,
            blockstate: "beta",
            variant: "missing",
        },
    ];
    static DEFS: &[BlockDefinition] = &[
        BlockDefinition {
            id: 1,
            name: "alpha_block",
            states: STATES_A,
        },
        BlockDefinition {
            id: 2,
            name: "beta_block",
            states: STATES_B,
        },
    ];

    let source = Memory::new()
        .model("base", cube("blocks/stone"))
        .model(
            "child",
            json!({ "parent": "base", "textures": { "all": "blocks/dirt" } }),
        )
        .model("slab", slab(0, 8, "blocks/stone"))
        .model(
            "unused",
            json!({ "parent": "base", "textures": { "all": "blocks/gravel" } }),
        )
        .blockstate(
            "alpha",
            json!({ "variants": { "b=2,a=1": { "model": "child" } } }),
        )
        .blockstate(
            "beta",
            json!({ "variants": { "normal": { "model": "slab" } } }),
        )
        .blockstate(
            "unused_state",
            json!({ "variants": { "normal": { "model": "unused" } } }),
        );

    let set = build(source, DEFS).expect("synthetic counted set");
    assert_eq!(set.source_model_count(), 4);
    assert_eq!(set.blockstate_count(), 3);
    assert_eq!(set.state_count(), 24, "16 alpha states plus 8 beta states");
    assert_eq!(set.model_count(), 2, "child and slab are baked");
    assert_eq!(set.fallback_count(), 4, "metadata 1, 5, 9 and 13 fall back");
    assert_eq!(set.texture_keys().len(), 2);
    assert!(set.texture_keys().contains("blocks/dirt"));
    assert!(set.texture_keys().contains("blocks/stone"));
    assert!(!set.texture_keys().contains("blocks/gravel"));

    let alpha = quads(&set, 1, 0);
    assert!(alpha.iter().all(|quad| quad.texture == "blocks/dirt"));
    let beta_fallback = quads(&set, 2, 1);
    assert!(beta_fallback
        .iter()
        .all(|quad| quad.texture == "blocks/stone"));
    assert!(set.model(BlockState::new(2, 15)).is_none());
}

#[test]
fn ambient_occlusion_is_inherited_and_can_be_overridden() {
    static STATES_A: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "inherit",
        variant: "normal",
    }];
    static STATES_B: &[StateDef] = &[StateDef {
        mask: 0,
        value: 0,
        blockstate: "override",
        variant: "normal",
    }];
    static DEFS: &[BlockDefinition] = &[
        BlockDefinition {
            id: 1,
            name: "inherit",
            states: STATES_A,
        },
        BlockDefinition {
            id: 2,
            name: "override",
            states: STATES_B,
        },
    ];

    let mut base = cube("blocks/stone");
    base["ambientocclusion"] = json!(false);
    let source = Memory::new()
        .model("base", base)
        .model("inherit", json!({ "parent": "base" }))
        .model(
            "override",
            json!({ "parent": "base", "ambientocclusion": true }),
        )
        .blockstate(
            "inherit",
            json!({ "variants": { "normal": { "model": "inherit" } } }),
        )
        .blockstate(
            "override",
            json!({ "variants": { "normal": { "model": "override" } } }),
        );

    let set = build(source, DEFS).expect("synthetic ambient occlusion");
    assert!(!resolved(&set, 1, 0).ambient_occlusion);
    assert!(resolved(&set, 2, 0).ambient_occlusion);
}

#[test]
fn blockstate_parser_is_public_for_tooling() {
    let document = BlockStateFile::parse(
        br#"{"variants":{"half=bottom,facing=north":{"model":"oak_stairs"}}}"#,
        "assets/minecraft/blockstates/oak_stairs.json",
    )
    .expect("synthetic blockstate");
    let variants = document
        .select("facing=north,half=bottom")
        .expect("canonicalized key");
    assert_eq!(variants[0].model, "oak_stairs");
    assert_eq!(variants[0].weight, 1);
}

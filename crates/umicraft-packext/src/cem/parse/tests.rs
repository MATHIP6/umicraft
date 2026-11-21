// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use super::*;

fn files(list: &[(&str, &str)]) -> HashMap<String, Vec<u8>> {
    list.iter()
        .map(|(p, c)| ((*p).to_string(), c.as_bytes().to_vec()))
        .collect()
}

fn parse(json: &str) -> Result<CemModel, CemError> {
    parse_at("cem/creeper.jem", json, &HashMap::new())
}

fn parse_at(
    path: &str,
    json: &str,
    others: &HashMap<String, Vec<u8>>,
) -> Result<CemModel, CemError> {
    parse_jem(path, json.as_bytes(), &|p| others.get(p).cloned())
}

const BOX: &str = r#"{"coordinates":[-4,-8,-4,8,8,8],"textureOffset":[0,0]}"#;

#[test]
fn a_minimal_model_parses() {
    let model = parse(&format!(
        r#"{{"texture":"c.png","textureSize":[64,32],"shadowSize":0.5,
           "models":[{{"part":"head","boxes":[{BOX}],"translate":[0,6,0]}}]}}"#
    ))
    .unwrap();
    assert_eq!(model.entity, "creeper");
    assert_eq!(model.texture, Some(PackPath::Feature("cem/c.png".into())));
    assert_eq!(model.texture_size, Some([64.0, 32.0]));
    assert_eq!(model.shadow_size, Some(0.5));
    let entry = &model.entries[0];
    assert_eq!(entry.part, "head");
    assert!(!entry.attach);
    assert_eq!(entry.root.translate, [0.0, 6.0, 0.0]);
    assert_eq!(entry.root.texture_size, Some([64.0, 32.0]));
    assert_eq!(entry.root.boxes[0].size, [8.0, 8.0, 8.0]);
    assert_eq!(entry.root.boxes[0].uv, BoxUv::Offset([0.0, 0.0]));
}

#[test]
fn required_keys_are_enforced() {
    assert!(matches!(parse("{}"), Err(CemError::Missing("models"))));
    assert!(matches!(
        parse(r#"{"models":[{}]}"#),
        Err(CemError::Missing("part"))
    ));
    assert!(matches!(parse("[1]"), Err(CemError::Type { .. })));
    assert!(matches!(parse("not json"), Err(CemError::Json(_))));
    assert!(matches!(
        parse(r#"{"models":3}"#),
        Err(CemError::Type { .. })
    ));
    assert!(matches!(
        parse(r#"{"models":[{"part":"a","boxes":[{"textureOffset":[0,0]}]}]}"#),
        Err(CemError::Missing("coordinates"))
    ));
    assert!(matches!(
        parse(r#"{"models":[{"part":"a","boxes":[{"coordinates":[0,0,0,1,1,1]}]}]}"#),
        Err(CemError::NoUv)
    ));
    assert!(matches!(
        parse(r#"{"models":[{"part":"a","translate":[1,2]}]}"#),
        Err(CemError::Length {
            key: "translate",
            ..
        })
    ));
}

#[test]
fn attach_and_animations_keep_file_order() {
    let model = parse(
        r#"{"models":[{"part":"head","attach":true,
            "animations":[{"this.ry":"1","head.rx":"2"},{"var.a":"3"}]}]}"#,
    )
    .unwrap();
    let entry = &model.entries[0];
    assert!(entry.attach);
    let targets: Vec<_> = entry.animations.iter().map(|a| a.target.as_str()).collect();
    assert_eq!(targets, ["this.ry", "head.rx", "var.a"]);
}

#[test]
fn rotation_is_in_degrees_and_invert_axis_flips_components() {
    let model = parse(
        r#"{"models":[{"part":"head","invertAxis":"xy","translate":[1,2,3],
            "rotate":[90,90,90],"boxes":[{"coordinates":[1,2,3,4,5,6],"textureOffset":[0,0]}]}]}"#,
    )
    .unwrap();
    let root = &model.entries[0].root;
    assert_eq!(root.translate, [-1.0, -2.0, 3.0]);
    let half_pi = std::f32::consts::FRAC_PI_2;
    assert_eq!(root.rotate, [-half_pi, -half_pi, half_pi]);
    // c = -c - size on the inverted axes only.
    assert_eq!(root.boxes[0].origin, [-5.0, -7.0, 3.0]);
}

#[test]
fn base_id_copies_missing_keys_but_not_the_id() {
    let model = parse(&format!(
        r#"{{"models":[
            {{"id":"leg","part":"leg1","translate":[2,6,4],"boxes":[{BOX}]}},
            {{"baseId":"leg","part":"leg2","translate":[-2,6,4]}}]}}"#
    ))
    .unwrap();
    let second = &model.entries[1];
    assert_eq!(second.part, "leg2");
    assert_eq!(second.root.translate, [-2.0, 6.0, 4.0], "own key wins");
    assert_eq!(second.root.boxes.len(), 1, "boxes inherited");
    assert_eq!(second.root.id, None, "id is not inherited");
}

#[test]
fn unknown_base_id_is_only_a_warning() {
    let model = parse(r#"{"models":[{"baseId":"nope","part":"head"}]}"#).unwrap();
    assert_eq!(model.entries.len(), 1);
}

#[test]
fn a_jpm_file_is_merged_without_overriding() {
    let others = files(&[(
        "cem/head.jpm",
        &format!(r#"{{"id":"ignored","translate":[9,9,9],"boxes":[{BOX}]}}"#),
    )]);
    let model = parse_at(
        "cem/creeper.jem",
        r#"{"models":[{"part":"head","model":"head.jpm","translate":[0,6,0]}]}"#,
        &others,
    )
    .unwrap();
    let root = &model.entries[0].root;
    assert_eq!(root.translate, [0.0, 6.0, 0.0]);
    assert_eq!(root.boxes.len(), 1);
    assert_eq!(root.id, None);
}

#[test]
fn a_missing_or_broken_jpm_is_skipped() {
    let others = files(&[("cem/bad.jpm", "{")]);
    for name in ["absent.jpm", "bad"] {
        let json = format!(r#"{{"models":[{{"part":"head","model":"{name}"}}]}}"#);
        let model = parse_at("cem/creeper.jem", &json, &others).unwrap();
        assert!(model.entries[0].root.boxes.is_empty());
    }
}

#[test]
fn per_face_uvs_and_aliases() {
    let model = parse(
        r#"{"models":[{"part":"a","boxes":[{"coordinates":[0,0,0,1,1,1],
            "uvDown":[0,0,1,1],"uvFront":[1,1,2,2],"uvRight":[3,3,4,4]}]}]}"#,
    )
    .unwrap();
    let BoxUv::Faces(f) = model.entries[0].root.boxes[0].uv else {
        panic!("faces expected");
    };
    assert_eq!(f[0], Some([0.0, 0.0, 1.0, 1.0]));
    assert_eq!(f[1], None);
    assert_eq!(f[2], Some([1.0, 1.0, 2.0, 2.0]), "uvFront is uvNorth");
    assert_eq!(f[5], Some([3.0, 3.0, 4.0, 4.0]), "uvRight is uvEast");
}

#[test]
fn size_add_mirror_scale_sprites() {
    let model = parse(
        r#"{"models":[{"part":"a","scale":2,"mirrorTexture":"UV","sprites":[{}],
            "boxes":[{"coordinates":[0,0,0,1,1,1],"textureOffset":[0,0],"sizeAdd":0.25}]}]}"#,
    )
    .unwrap();
    let root = &model.entries[0].root;
    assert_eq!(root.scale, 2.0);
    assert!(root.mirror_u && root.mirror_v);
    assert_eq!(root.sprites, 1);
    assert_eq!(root.boxes[0].size_add, 0.25);
}

#[test]
fn submodels_nest_and_inherit_texture_size() {
    let model = parse(&format!(
        r#"{{"textureSize":[64,64],"models":[{{"part":"body","id":"b",
            "submodel":{{"id":"one","boxes":[{BOX}]}},
            "submodels":[{{"id":"two","textureSize":[16,16],
               "submodel":{{"id":"deep"}}}}]}}]}}"#
    ))
    .unwrap();
    let root = &model.entries[0].root;
    assert_eq!(root.children.len(), 2);
    assert_eq!(root.children[0].id.as_deref(), Some("one"));
    assert_eq!(root.children[0].texture_size, Some([64.0, 64.0]));
    assert_eq!(root.children[1].texture_size, Some([16.0, 16.0]));
    assert_eq!(
        root.children[1].children[0].texture_size,
        Some([16.0, 16.0])
    );
    let ids: Vec<_> = model.parts().iter().map(|p| p.id.clone()).collect();
    assert_eq!(
        ids,
        [
            Some("b".into()),
            Some("one".into()),
            Some("two".into()),
            Some("deep".into())
        ],
        "depth first"
    );
}

#[test]
fn nesting_has_a_limit() {
    let mut json = String::from(r#"{"id":"x"}"#);
    for _ in 0..100 {
        json = format!(r#"{{"submodel":{json}}}"#);
    }
    let file = format!(r#"{{"models":[{{"part":"a","submodel":{json}}}]}}"#);
    assert!(matches!(parse(&file), Err(CemError::TooDeep)));
}

#[test]
fn path_rules() {
    use PackPath::{Feature, Resource};
    assert_eq!(
        resolve_path("cem", "a", ".png"),
        Feature("cem/a.png".into())
    );
    assert_eq!(
        resolve_path("cem", "a.png", ".png"),
        Feature("cem/a.png".into())
    );
    assert_eq!(
        resolve_path("cem", "./b/a", ".png"),
        Feature("cem/b/a.png".into())
    );
    assert_eq!(
        resolve_path("cem/x", "~/cem/a", ".png"),
        Feature("cem/a.png".into())
    );
    assert_eq!(
        resolve_path("cem", "textures/entity/a", ".png"),
        Resource("textures/entity/a.png".into())
    );
    assert_eq!(
        resolve_path("cem", "minecraft:textures/entity/a", ".png"),
        Resource("textures/entity/a.png".into())
    );
    assert_eq!(resolve_path("", "a", ".jpm"), Feature("a.jpm".into()));
}

#[test]
fn the_entity_is_the_file_name() {
    let model = parse_at(
        "cem/zombie/zombie_pigman.jem",
        r#"{"models":[]}"#,
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(model.entity, "zombie_pigman");
}

#[test]
fn numeric_animation_values_are_read_as_text() {
    let model = parse(r#"{"models":[{"part":"a","animations":[{"this.rx":0.5}]}]}"#).unwrap();
    assert_eq!(model.entries[0].animations[0].expression, "0.5");
}

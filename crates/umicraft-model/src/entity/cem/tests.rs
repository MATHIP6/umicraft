// SPDX-License-Identifier: MIT OR Apache-2.0

use umicraft_packext::cem::parse_jem;

use super::*;
use crate::entity::{CreeperModel, HumanoidModel};

fn rig(json: &str) -> CemRig {
    CemRig::new(parse_jem("cem/creeper.jem", json.as_bytes(), &|_| None).expect("parses"))
}

/// A head: an 8-pixel cube at the head's rest pivot, as MCP lays it out
/// (`addBox(-4, -8, -4, 8, 8, 8)` at rotation point `(0, 6, 0)`).
const HEAD: &str = r#"{"textureSize":[64,32],"models":[
  {"part":"head","translate":[0,6,0],
   "boxes":[{"coordinates":[-4,-8,-4,8,8,8],"textureOffset":[0,0]}]}]}"#;

fn bounds(vertices: &[ModelVertex]) -> ([f32; 3], [f32; 3]) {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for v in vertices {
        for axis in 0..3 {
            lo[axis] = lo[axis].min(v.position[axis]);
            hi[axis] = hi[axis].max(v.position[axis]);
        }
    }
    (lo, hi)
}

fn close(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5)
}

fn apply(rig: &CemRig, vanilla: &mut CreeperModel, inputs: &EntityInputs) -> Vec<ModelVertex> {
    let mut state = rig.new_state(1);
    rig.apply(vanilla, &CreeperModel::new(), &mut state, inputs, 0.016)
}

#[test]
fn a_replacing_head_lands_where_the_vanilla_head_was() {
    let mut vanilla = CreeperModel::new();
    let vanilla_head = {
        let mut out = Vec::new();
        vanilla
            .head
            .emit(Mat4::from_scale(Vec3::splat(1.0 / 16.0)), &mut out);
        out
    };
    let custom = apply(&rig(HEAD), &mut vanilla, &EntityInputs::default());
    assert_eq!(custom.len(), 36, "one box, six faces");
    let (lo, hi) = bounds(&custom);
    let (vlo, vhi) = bounds(&vanilla_head);
    assert!(
        close(lo, vlo) && close(hi, vhi),
        "{lo:?} {hi:?} vs {vlo:?} {vhi:?}"
    );
    assert!(close(lo, [-0.25, 1.125, -0.25]) && close(hi, [0.25, 1.625, 0.25]));
    assert!(vanilla.head.boxes.is_empty(), "the vanilla head is gone");
    assert!(!vanilla.body.boxes.is_empty(), "the rest stays");
}

#[test]
fn a_custom_creeper_has_the_expected_box_count() {
    let legs: String = (1..=4)
        .map(|n| {
            let (x, z) = (if n % 2 == 1 { -2 } else { 2 }, if n <= 2 { 4 } else { -4 });
            format!(
                r#"{{"part":"leg{n}","translate":[{x},18,{z}],
                   "boxes":[{{"coordinates":[-2,0,-2,4,6,4],"textureOffset":[0,16]}}]}},"#
            )
        })
        .collect();
    let json = format!(
        r#"{{"textureSize":[64,32],"models":[{legs}
          {{"part":"body","translate":[0,6,0],
            "boxes":[{{"coordinates":[-4,0,-2,8,12,4],"textureOffset":[16,16]}}]}},
          {{"part":"head","translate":[0,6,0],
            "boxes":[{{"coordinates":[-4,-8,-4,8,8,8],"textureOffset":[0,0]}}]}}]}}"#
    );
    let mut vanilla = CreeperModel::new();
    let custom = apply(&rig(&json), &mut vanilla, &EntityInputs::default());
    assert_eq!(custom.len(), 6 * 36);
    // The custom legs sit at the vanilla legs' feet and hips.
    let (lo, hi) = bounds(&custom);
    assert!(lo[1].abs() < 1e-5, "feet on the ground: {lo:?}");
    assert!((hi[1] - 1.625).abs() < 1e-5, "top of the head: {hi:?}");
}

#[test]
fn an_attached_part_follows_the_vanilla_part() {
    let json = r#"{"models":[{"part":"head","attach":true,
        "boxes":[{"coordinates":[-1,-10,-1,2,2,2],"textureOffset":[0,0]}]}]}"#;
    let mut vanilla = CreeperModel::new();
    vanilla.head.rotation[1] = std::f32::consts::FRAC_PI_2; // turned a quarter
    let custom = apply(&rig(json), &mut vanilla, &EntityInputs::default());
    assert!(
        !vanilla.head.boxes.is_empty(),
        "attach keeps the vanilla boxes"
    );
    let (lo, hi) = bounds(&custom);
    // A 2x2x2 box 2 pixels above the head top, centred: still centred on x
    // and z whichever way the head turns, and above the head.
    assert!(close(lo, [-0.0625, 1.625, -0.0625]), "{lo:?}");
    assert!(close(hi, [0.0625, 1.75, 0.0625]), "{hi:?}");
}

#[test]
fn an_animation_overrides_the_vanilla_rotation() {
    let json = r#"{"models":[{"part":"head","translate":[0,6,0],
        "animations":[{"this.ry":"torad(90)"}],
        "boxes":[{"coordinates":[-4,-8,-4,8,8,16],"textureOffset":[0,0]}]}]}"#;
    let mut vanilla = CreeperModel::new();
    let custom = apply(&rig(json), &mut vanilla, &EntityInputs::default());
    let (lo, hi) = bounds(&custom);
    // The box is 8 wide in x and 16 deep in z before turning; a quarter turn
    // about y makes it 16 wide in x and 8 deep in z.
    assert!(((hi[0] - lo[0]) - 1.0).abs() < 1e-4, "{lo:?} {hi:?}");
    assert!(((hi[2] - lo[2]) - 0.5).abs() < 1e-4);
}

#[test]
fn walking_still_moves_a_replaced_leg() {
    let json = r#"{"models":[{"part":"leg1","translate":[-2,18,4],
        "boxes":[{"coordinates":[-2,0,-2,4,6,4],"textureOffset":[0,16]}]}]}"#;
    let rig = rig(json);
    let mut still = CreeperModel::new();
    let straight = apply(&rig, &mut still, &EntityInputs::default());
    let mut walking = CreeperModel::new();
    walking.set_angles(1.0, 1.0, 0.0, 0.0);
    let swung = apply(&rig, &mut walking, &EntityInputs::default());
    assert_ne!(
        bounds(&straight),
        bounds(&swung),
        "the vanilla swing carries over"
    );
}

#[test]
fn unknown_parts_are_skipped() {
    let json = r#"{"models":[{"part":"tail","boxes":[{"coordinates":[0,0,0,1,1,1],"textureOffset":[0,0]}]}]}"#;
    let mut vanilla = CreeperModel::new();
    assert!(apply(&rig(json), &mut vanilla, &EntityInputs::default()).is_empty());
}

#[test]
fn invisible_parts_are_not_drawn_and_do_not_shift_later_ones() {
    let json = r#"{"models":[{"part":"head","translate":[0,6,0],
        "animations":[{"this.visible":"false"},{"two.ty":"0"}],
        "boxes":[{"coordinates":[-1,-1,-1,2,2,2],"textureOffset":[0,0]}],
        "submodel":{"id":"hidden","boxes":[{"coordinates":[-1,-1,-1,2,2,2],"textureOffset":[0,0]}]}},
        {"part":"body","id":"two","translate":[0,6,0],
         "boxes":[{"coordinates":[-1,-1,-1,2,2,2],"textureOffset":[0,0]}]}]}"#;
    let mut vanilla = CreeperModel::new();
    let custom = apply(&rig(json), &mut vanilla, &EntityInputs::default());
    assert_eq!(custom.len(), 36, "only the body box");
}

#[test]
fn per_face_uvs_use_their_rectangles() {
    let json = r#"{"textureSize":[16,16],"models":[{"part":"head","translate":[0,6,0],
        "boxes":[{"coordinates":[0,0,0,2,2,2],"uvUp":[0,0,8,8],"uvNorth":[8,8,16,16]}]}]}"#;
    let mut vanilla = CreeperModel::new();
    let custom = apply(&rig(json), &mut vanilla, &EntityInputs::default());
    assert_eq!(custom.len(), 12, "two faces");
    let up = &custom[..6];
    let uv_max = up.iter().map(|v| v.uv[0]).fold(0.0_f32, f32::max);
    assert!((uv_max - 0.5).abs() < 1e-6);
    let north = &custom[6..];
    assert!(north.iter().all(|v| v.uv[0] >= 0.5 && v.uv[1] >= 0.5));
}

#[test]
fn a_zombie_arm_can_be_replaced() {
    let json = r#"{"models":[{"part":"right_arm","translate":[-5,2,0],
        "boxes":[{"coordinates":[-3,-2,-2,4,12,4],"textureOffset":[40,16]}]}]}"#;
    let rig = rig(json);
    let mut zombie = HumanoidModel::zombie();
    let mut state = rig.new_state(1);
    let custom = rig.apply(
        &mut zombie,
        &HumanoidModel::zombie(),
        &mut state,
        &EntityInputs::default(),
        0.0,
    );
    assert_eq!(custom.len(), 36);
    assert!(zombie.right_arm.boxes.is_empty());
}

#[test]
fn mirrored_texture_flips_the_box() {
    let plain = r#"{"models":[{"part":"head","translate":[0,6,0],
        "boxes":[{"coordinates":[-4,-8,-4,8,8,8],"textureOffset":[0,0]}]}]}"#;
    let mirrored = plain.replace(r#""part":"head""#, r#""part":"head","mirrorTexture":"u""#);
    let mut a = CreeperModel::new();
    let mut b = CreeperModel::new();
    let first = apply(&rig(plain), &mut a, &EntityInputs::default());
    let second = apply(&rig(&mirrored), &mut b, &EntityInputs::default());
    assert_eq!(first.len(), second.len());
    assert_ne!(first[0].uv, second[0].uv);
}

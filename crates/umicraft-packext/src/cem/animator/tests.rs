// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;
use crate::cem::parse_jem;

fn model(json: &str) -> CemModel {
    parse_jem("cem/test.jem", json.as_bytes(), &|_| None).expect("test model parses")
}

const TWO_PARTS: &str = r#"{"models":[
  {"part":"head","id":"h","translate":[0,6,0],
   "animations":[{"this.ry":"torad(head_yaw)"},{"this.rx":"torad(head_pitch)"}],
   "submodel":{"id":"nose","translate":[0,1,0]}},
  {"part":"leg1","translate":[2,18,4],
   "animations":[{"this.rx":"cos(limb_swing * 0.6662) * 1.4 * limb_speed"},
                 {"h:nose.tz":"leg1.rx"}]}
]}"#;

fn inputs() -> EntityInputs {
    EntityInputs {
        head_yaw: 90.0,
        head_pitch: 45.0,
        limb_swing: 0.0,
        limb_speed: 0.5,
        ..EntityInputs::default()
    }
}

#[test]
fn animations_override_rotations_in_file_order() {
    let m = model(TWO_PARTS);
    let animator = CemAnimator::new(&m);
    assert_eq!(animator.assignment_count(), 4);
    let mut state = animator.new_state(1);
    animator.evaluate(&mut state, &inputs(), &[], 0.016);
    let head = animator.pose(&state, 0);
    assert!((head.rotate[1] - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    assert!((head.rotate[0] - std::f32::consts::FRAC_PI_4).abs() < 1e-5);
    assert_eq!(
        head.translate,
        [0.0, 6.0, 0.0],
        "untouched values stay at rest"
    );
    let leg = animator.pose(&state, animator.root_of(1).unwrap());
    assert!((leg.rotate[0] - 0.7).abs() < 1e-5);
    // A later assignment sees the value an earlier one wrote, and a child is
    // addressed through its parent's id.
    let nose = animator.pose(&state, 1);
    assert!((nose.translate[2] - 0.7).abs() < 1e-5);
}

#[test]
fn the_vanilla_move_is_added_to_the_rest_pose_before_the_animations() {
    let m = model(
        r#"{"models":[{"part":"head","rotate":[0,10,0],"translate":[0,6,0],
        "animations":[{"this.rz":"this.ry * 2"}]}]}"#,
    );
    let animator = CemAnimator::new(&m);
    let mut state = animator.new_state(1);
    let delta = RootDelta {
        translate: [0.0, 1.0, 0.0],
        rotate: [0.5, 0.0, 0.0],
    };
    animator.evaluate(&mut state, &EntityInputs::default(), &[delta], 0.0);
    let pose = animator.pose(&state, 0);
    assert_eq!(pose.translate, [0.0, 7.0, 0.0]);
    assert!((pose.rotate[0] - 0.5).abs() < 1e-6);
    let ten = 10.0_f32.to_radians();
    assert!(
        (pose.rotate[2] - 2.0 * ten).abs() < 1e-6,
        "reads its own rest rotation"
    );
}

#[test]
fn a_bad_assignment_is_dropped_and_the_others_still_run() {
    let m = model(
        r#"{"models":[{"part":"head","animations":[
        {"this.rx":"nonexistent_variable * 2"},
        {"this.ry":"1 +"},
        {"this.nope":"1"},
        {"ghost.rx":"1"},
        {"limb_swing":"1"},
        {"this.visible":"1"},
        {"this.rz":"0.25"}]}]}"#,
    );
    let animator = CemAnimator::new(&m);
    assert_eq!(animator.assignment_count(), 1);
    let mut state = animator.new_state(1);
    animator.evaluate(&mut state, &EntityInputs::default(), &[], 0.0);
    assert_eq!(animator.pose(&state, 0).rotate, [0.0, 0.0, 0.25]);
}

#[test]
fn custom_variables_persist_between_frames() {
    let m = model(
        r#"{"models":[{"part":"head","animations":[
        {"var.count":"var.count + 1"},{"this.ty":"var.count"}]}]}"#,
    );
    let animator = CemAnimator::new(&m);
    let mut state = animator.new_state(1);
    for frame in 1..=3 {
        animator.evaluate(&mut state, &EntityInputs::default(), &[], 0.0);
        assert_eq!(animator.pose(&state, 0).translate[1], frame as f32);
    }
    let mut other = animator.new_state(2);
    animator.evaluate(&mut other, &EntityInputs::default(), &[], 0.0);
    assert_eq!(
        animator.pose(&other, 0).translate[1],
        1.0,
        "state is per entity"
    );
}

#[test]
fn visibility_and_boolean_variables() {
    let m = model(
        r#"{"models":[{"part":"head","animations":[
        {"varb.hide":"is_child"},
        {"this.visible":"!varb.hide"},
        {"this.visible_boxes":"limb_speed < 0.1"},
        {"this.sx":"if(is_riding, 2, 1)"}]}]}"#,
    );
    let animator = CemAnimator::new(&m);
    let mut state = animator.new_state(1);
    animator.evaluate(&mut state, &EntityInputs::default(), &[], 0.0);
    let pose = animator.pose(&state, 0);
    assert!(pose.visible && pose.visible_boxes);
    assert_eq!(pose.scale, [1.0; 3]);
    let baby = EntityInputs {
        is_child: true,
        is_riding: true,
        limb_speed: 1.0,
        ..EntityInputs::default()
    };
    animator.evaluate(&mut state, &baby, &[], 0.0);
    let pose = animator.pose(&state, 0);
    assert!(!pose.visible && !pose.visible_boxes);
    assert_eq!(pose.scale[0], 2.0);
}

#[test]
fn smooth_is_kept_per_state() {
    let m = model(
        r#"{"models":[{"part":"head","animations":[
        {"this.tx":"smooth(0, head_yaw, 1)"}]}]}"#,
    );
    let animator = CemAnimator::new(&m);
    let mut state = animator.new_state(1);
    let at = |yaw| EntityInputs {
        head_yaw: yaw,
        ..EntityInputs::default()
    };
    animator.evaluate(&mut state, &at(10.0), &[], 0.1);
    assert_eq!(animator.pose(&state, 0).translate[0], 10.0);
    animator.evaluate(&mut state, &at(0.0), &[], 0.1);
    let eased = animator.pose(&state, 0).translate[0];
    assert!(eased > 0.0 && eased < 10.0);
}

#[test]
fn part_names_ids_and_hierarchy_resolve() {
    let m = model(
        r#"{"models":[{"part":"body","id":"b","animations":[
        {"b.tx":"1"},{"body.ty":"2"},{"part.tz":"3"},{"b:inner.rx":"4"}],
        "submodel":{"id":"inner"}}]}"#,
    );
    let animator = CemAnimator::new(&m);
    assert_eq!(animator.assignment_count(), 4);
    let mut state = animator.new_state(1);
    animator.evaluate(&mut state, &EntityInputs::default(), &[], 0.0);
    assert_eq!(animator.pose(&state, 0).translate, [1.0, 2.0, 3.0]);
    assert_eq!(animator.pose(&state, 1).rotate[0], 4.0);
}

#[test]
fn evaluation_leaves_the_state_sizes_alone() {
    let m = model(TWO_PARTS);
    let animator = CemAnimator::new(&m);
    let mut state = animator.new_state(1);
    let (memory, smooth) = (state.memory.capacity(), state.smooth.capacity());
    for _ in 0..100 {
        animator.evaluate(&mut state, &inputs(), &[RootDelta::default(); 2], 0.016);
    }
    assert_eq!(
        (state.memory.capacity(), state.smooth.capacity()),
        (memory, smooth)
    );
}

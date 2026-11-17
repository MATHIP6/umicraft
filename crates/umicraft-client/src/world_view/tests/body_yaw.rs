// SPDX-License-Identifier: MIT OR Apache-2.0

//! The head and body yaw of the local player in the vanilla scenarios.

use super::*;

// =========================================================================
// VANILLA 1.8.9 HEAD ROTATION & BODY ALIGNMENT REPLICATION TESTS (PHASE 5)
// =========================================================================

fn local_player_record(eye: glam::DVec3, yaw: f32, pitch: f32) -> EntityRecord {
    let mut rec = EntityRecord::new(EntityId(1), EntityKind::Player, eye, yaw, pitch);
    rec.local = true;
    rec
}

#[test]
fn test_vanilla_scenario_1_player_turns_only_head_under_50_degrees() {
    // Vanilla EntityLivingBase.func_110146_f:
    // When twist*twist <= 2500 (angle < 50°), the body stays completely still.
    let mut harness = Harness::new();
    let tick = Instant::now();
    let eye = glam::DVec3::new(0.0, 64.0, 0.0);

    let mut f0 = frame(eye.as_vec3(), tick, 0);
    f0.perspective = Perspective::ThirdPersonBack;
    let rec = local_player_record(eye, 0.0, 0.0);
    let fd = rec.frame_data();
    f0.local_body = Arc::new(fd.clone());
    f0.entities = Arc::from([fd]);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f0))
        .expect("frame");
    harness.sync();

    // Turn head 30° to the right via raw mouse look
    harness
        .world_view
        .apply_look_delta(30.0_f32.to_radians(), 0.0);
    let mut out = Vec::new();
    harness
        .world_view
        .entities_into(tick, glam::DVec3::ZERO, &mut out);

    let local = out.iter().find(|e| e.local).expect("local player snapshot");
    assert!(
        (local.head_yaw - 30.0).abs() < 1e-3,
        "head must face 30°, got {}",
        local.head_yaw
    );
    assert!(
        (local.body_yaw - 0.0).abs() < 1e-3,
        "body must stay still at 0°, got {}",
        local.body_yaw
    );
    let net_head_yaw = wrap_degrees(local.head_yaw - local.body_yaw);
    assert!(
        (net_head_yaw - 30.0).abs() < 1e-3,
        "netHeadYaw must be exactly 30°, got {}",
        net_head_yaw
    );
}

#[test]
fn test_vanilla_scenario_2_player_turns_head_past_75_degrees_drags_body() {
    // Vanilla EntityLivingBase.func_110146_f:
    // f1 = wrapAngleTo180(rotationYaw - renderYawOffset) clamped to [-75, 75]
    // renderYawOffset = rotationYaw - f1; if f1*f1 > 2500 renderYawOffset += f1*0.2
    let mut harness = Harness::new();
    let tick = Instant::now();
    let eye = glam::DVec3::new(0.0, 64.0, 0.0);

    let mut f0 = frame(eye.as_vec3(), tick, 0);
    f0.perspective = Perspective::ThirdPersonBack;
    let rec = local_player_record(eye, 0.0, 0.0);
    let fd = rec.frame_data();
    f0.local_body = Arc::new(fd.clone());
    f0.entities = Arc::from([fd]);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f0))
        .expect("frame");
    harness.sync();

    // Turn head 90° right: twist clamps to 75°, diff*diff > 2500 applies 0.2*75 = 15°
    // Expected body_yaw = 90 - 75 + 15 = 30°
    harness
        .world_view
        .apply_look_delta(90.0_f32.to_radians(), 0.0);
    let mut out = Vec::new();
    harness
        .world_view
        .entities_into(tick, glam::DVec3::ZERO, &mut out);

    let local = out.iter().find(|e| e.local).expect("local player snapshot");
    assert!((local.head_yaw - 90.0).abs() < 1e-3, "head must face 90°");
    assert!(
        (local.body_yaw - 30.0).abs() < 1e-3,
        "body must be dragged to 30°, got {}",
        local.body_yaw
    );
    let net_head_yaw = wrap_degrees(local.head_yaw - local.body_yaw);
    assert!(
        (net_head_yaw - 60.0).abs() < 1e-3,
        "netHeadYaw must be 60° (within 75° bound)"
    );
}

#[test]
fn test_vanilla_scenario_3_rapid_mouse_flick_left_and_right() {
    // Rapid alternating mouse flicking: ensure netHeadYaw remains strictly in [-75, 75]
    let mut harness = Harness::new();
    let tick = Instant::now();
    let eye = glam::DVec3::new(0.0, 64.0, 0.0);

    let mut f0 = frame(eye.as_vec3(), tick, 0);
    f0.perspective = Perspective::ThirdPersonBack;
    let rec = local_player_record(eye, 0.0, 0.0);
    let fd = rec.frame_data();
    f0.local_body = Arc::new(fd.clone());
    f0.entities = Arc::from([fd]);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f0))
        .expect("frame");
    harness.sync();

    let deltas = [60.0_f32, -120.0, 150.0, -180.0, 70.0, -90.0];
    for d in deltas {
        harness.world_view.apply_look_delta(d.to_radians(), 0.0);
        let mut out = Vec::new();
        harness
            .world_view
            .entities_into(tick, glam::DVec3::ZERO, &mut out);
        let local = out.iter().find(|e| e.local).expect("local player");
        let net_head_yaw = wrap_degrees(local.head_yaw - local.body_yaw);
        assert!(
            net_head_yaw.abs() <= 75.001,
            "netHeadYaw {} exceeded 75° during flick delta {}",
            net_head_yaw,
            d
        );
    }
}

#[test]
fn test_vanilla_scenario_4_natural_lag_offset_beyond_50_degrees() {
    // Exact 50° threshold test (EntityLivingBase.java: f1 * f1 > 2500.0F)
    let mut harness = Harness::new();
    let tick = Instant::now();
    let eye = glam::DVec3::new(0.0, 64.0, 0.0);

    let mut f0 = frame(eye.as_vec3(), tick, 0);
    f0.perspective = Perspective::ThirdPersonBack;
    let rec = local_player_record(eye, 0.0, 0.0);
    let fd = rec.frame_data();
    f0.local_body = Arc::new(fd.clone());
    f0.entities = Arc::from([fd]);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f0))
        .expect("frame");
    harness.sync();

    // 1. At 45° (45^2 = 2025 <= 2500): body stays at 0°
    harness
        .world_view
        .apply_look_delta(45.0_f32.to_radians(), 0.0);
    let mut out = Vec::new();
    harness
        .world_view
        .entities_into(tick, glam::DVec3::ZERO, &mut out);
    let local45 = out.iter().find(|e| e.local).unwrap();
    assert!(
        (local45.body_yaw - 0.0).abs() < 1e-3,
        "body must stay 0° under 50°"
    );

    // 2. Further to 60° (60^2 = 3600 > 2500): body catches up by 60 * 0.2 = 12°
    harness
        .world_view
        .apply_look_delta(15.0_f32.to_radians(), 0.0);
    out.clear();
    harness
        .world_view
        .entities_into(tick, glam::DVec3::ZERO, &mut out);
    let local60 = out.iter().find(|e| e.local).unwrap();
    assert!(
        (local60.body_yaw - 12.0).abs() < 1e-3,
        "body must catch up to 12° at 60° look"
    );
    assert!((local60.head_yaw - 60.0).abs() < 1e-3);
}

#[test]
fn test_vanilla_scenario_5_subtick_smooth_rotation_interpolation() {
    let mut harness = Harness::new();
    let t0 = Instant::now();
    let eye = glam::DVec3::new(0.0, 64.0, 0.0);

    // Tick 0 with entity facing 0°
    let mut f0 = frame(eye.as_vec3(), t0, 0);
    f0.perspective = Perspective::ThirdPersonBack;
    let mut remote0 = EntityRecord::new(EntityId(2), EntityKind::Player, eye, 0.0, 0.0);
    remote0.local = false;
    f0.entities = Arc::from([remote0.frame_data()]);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f0))
        .expect("frame");
    harness.sync();

    // Tick 1 with entity turning to 40°
    let t1 = t0 + TICK;
    let mut f1 = frame(eye.as_vec3(), t1, 1);
    f1.perspective = Perspective::ThirdPersonBack;
    let mut remote1 = EntityRecord::new(EntityId(2), EntityKind::Player, eye, 40.0, 0.0);
    remote1.local = false;
    remote1.previous_yaw = 0.0;
    remote1.previous_head_yaw = 0.0;
    remote1.body_yaw.offset = 12.0;
    remote1.body_yaw.previous = 0.0;
    f1.entities = Arc::from([remote1.frame_data()]);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f1))
        .expect("frame");
    harness.sync();

    // Sample at alpha = 0.5 (halfway between ticks)
    let t_half = t1 + TICK / 2;
    let mut out = Vec::new();
    harness
        .world_view
        .entities_into(t_half, glam::DVec3::ZERO, &mut out);
    let remote = out.iter().find(|e| e.id == EntityId(2)).unwrap();
    // Head smoothly interpolates: (0° to 40° at alpha 0.5 is 20°)
    assert!(
        (remote.head_yaw - 20.0).abs() < 1e-3,
        "sub-tick head_yaw must smoothly interpolate, got {}",
        remote.head_yaw
    );
    // Body smoothly interpolates: (0° to 12° at alpha 0.5 is 6°)
    assert!(
        (remote.body_yaw - 6.0).abs() < 1e-3,
        "sub-tick body_yaw must smoothly interpolate, got {}",
        remote.body_yaw
    );
}

#[test]
fn test_vanilla_scenario_6_wrapping_across_359_to_0_degrees() {
    // Player faces 355° (or -5°) and turns to 10°
    let mut harness = Harness::new();
    let tick = Instant::now();
    let eye = glam::DVec3::new(0.0, 64.0, 0.0);

    let mut f0 = frame(eye.as_vec3(), tick, 0);
    f0.perspective = Perspective::ThirdPersonBack;
    let mut rec = local_player_record(eye, 355.0, 0.0);
    rec.body_yaw.reset_to(355.0);
    let fd = rec.frame_data();
    f0.local_body = Arc::new(fd.clone());
    f0.entities = Arc::from([fd]);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f0))
        .expect("frame");
    harness.sync();

    // Apply mouse delta of +15° (from 355° to 10°)
    harness
        .world_view
        .apply_look_delta(15.0_f32.to_radians(), 0.0);
    let mut out = Vec::new();
    harness
        .world_view
        .entities_into(tick, glam::DVec3::ZERO, &mut out);
    let local = out.iter().find(|e| e.local).unwrap();

    assert!(
        (wrap_degrees(local.head_yaw) - 10.0).abs() < 1e-3,
        "head must wrap to 10°"
    );
    let net_head_yaw = wrap_degrees(local.head_yaw - local.body_yaw);
    assert!(
        (net_head_yaw - 15.0).abs() < 1e-3,
        "netHeadYaw must be +15°, got {}",
        net_head_yaw
    );
}

#[test]
fn test_vanilla_scenario_7_f5_third_person_back_head_and_body() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    let eye = glam::DVec3::new(10.0, 64.0, 20.0);

    let mut f0 = frame(eye.as_vec3(), tick, 0);
    f0.perspective = Perspective::ThirdPersonBack;
    let rec = local_player_record(eye, 45.0, 10.0);
    let fd = rec.frame_data();
    f0.local_body = Arc::new(fd.clone());
    f0.entities = Arc::from([fd]);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f0))
        .expect("frame");
    harness.sync();

    let mut out = Vec::new();
    harness
        .world_view
        .entities_into(tick, glam::DVec3::ZERO, &mut out);
    let local = out.iter().find(|e| e.local).unwrap();
    assert!((local.head_yaw - 45.0).abs() < 1e-3);
    assert!((local.body_yaw - 45.0).abs() < 1e-3);
    assert!((local.pitch - 10.0).abs() < 1e-3);
}

#[test]
fn test_vanilla_scenario_8_f5_third_person_front_head_faces_camera() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    let eye = glam::DVec3::new(10.0, 64.0, 20.0);

    let mut f0 = frame(eye.as_vec3(), tick, 0);
    f0.perspective = Perspective::ThirdPersonFront;
    let rec = local_player_record(eye, 0.0, 0.0);
    let fd = rec.frame_data();
    f0.local_body = Arc::new(fd.clone());
    f0.entities = Arc::from([fd]);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f0))
        .expect("frame");
    harness.sync();

    // In front view, camera is looking at player's face from the front
    // Turning mouse 20° turns player look direction towards 20°
    harness
        .world_view
        .apply_look_delta(20.0_f32.to_radians(), 0.0);
    let mut out = Vec::new();
    harness
        .world_view
        .entities_into(tick, glam::DVec3::ZERO, &mut out);
    let local = out.iter().find(|e| e.local).unwrap();
    assert!(
        (local.head_yaw - 20.0).abs() < 1e-3,
        "head must face player_yaw 20°"
    );
    assert!(
        (local.body_yaw - 0.0).abs() < 1e-3,
        "body stays within 50° limit"
    );
    let net = wrap_degrees(local.head_yaw - local.body_yaw);
    assert!((net - 20.0).abs() < 1e-3);
}

#[test]
fn test_vanilla_scenario_9_movement_and_jump_body_alignment() {
    // When player moves along +X (Minecraft yaw -90°):
    // BodyYaw::update_player turns body 30% towards -90° while head looks at 0°
    let mut body = BodyYaw::new(0.0);
    body.update_player((0.2, 0.0), 0.0, false);
    // target = atan2(0.0, 0.2) - 90 = -90°
    // offset += wrap_degrees(-90 - 0) * 0.3 = -27°
    assert!(
        (body.offset - (-27.0)).abs() < 1e-3,
        "body aligns towards movement direction"
    );

    // When swinging arm (PvP attack during jump):
    // target snaps to yaw (0°), pulling body back towards aim
    body.update_player((0.2, 0.0), 0.0, true);
    // target = 0°, offset += wrap_degrees(0 - (-27)) * 0.3 = -27 + 8.1 = -18.9°
    assert!(
        (body.offset - (-18.9)).abs() < 1e-3,
        "swinging snaps target towards aim"
    );
}

#[test]
fn test_vanilla_scenario_10_server_setback_head_and_body_clean_reset() {
    let mut record = EntityRecord::new(
        EntityId(1),
        EntityKind::Player,
        glam::DVec3::new(10.0, 64.0, 10.0),
        45.0,
        10.0,
    );
    record.body_yaw.offset = 30.0;
    record.body_yaw.previous = 20.0;

    // Server setback arrives at (0, 64, 0), yaw 180°, pitch 0°
    record.teleport_to_exact(glam::DVec3::new(0.0, 64.0, 0.0), 180.0, 0.0);

    assert_eq!(record.yaw, 180.0);
    assert_eq!(record.head_yaw, 180.0);
    assert_eq!(record.previous_head_yaw, 180.0);
    assert_eq!(record.body_yaw.offset, 180.0);
    assert_eq!(record.body_yaw.previous, 180.0);

    let snap = record.frame_data().interpolate(0.5, glam::DVec3::ZERO);
    assert_eq!(snap.head_yaw, 180.0);
    assert_eq!(snap.body_yaw, 180.0);
    assert_eq!(
        wrap_degrees(snap.head_yaw - snap.body_yaw),
        0.0,
        "zero residual twist after setback"
    );
}

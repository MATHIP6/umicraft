// SPDX-License-Identifier: MIT OR Apache-2.0

//! The camera: the mouse look between ticks, the perspectives, the view
//! effects options and teleports.

use super::*;

#[test]
fn a_look_between_ticks_turns_the_camera_without_restarting_the_interpolation() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    let later = tick + TICK;
    for event in [
        RenderEvent::Frame(frame(glam::Vec3::ZERO, tick, 0)),
        RenderEvent::Frame(frame(glam::Vec3::new(2.0, 0.0, 0.0), later, 0)),
    ] {
        harness.frames.0.send(event).expect("frame queue");
    }
    let mut turned = frame(glam::Vec3::new(2.0, 0.0, 0.0), later, 0).state;
    turned.yaw = 1.0;
    // The mouse never queues an event: it overwrites the shared slot, so
    // a hundred looks leave the frame queue as empty as one.
    let slot = crate::game::LookSlot::default();
    harness
        .frames
        .0
        .send(RenderEvent::LookSlot(slot.clone()))
        .expect("frame queue");
    for yaw in 0..100 {
        let mut state = turned;
        state.yaw = yaw as f32 / 100.0;
        slot.store(crate::game::Look {
            view: state,
            body_pitch: 7.0,
        });
    }
    slot.store(crate::game::Look {
        view: turned,
        body_pitch: 12.0,
    });
    harness.sync();
    assert_eq!(harness.world_view.view.body_pitch, Some(12.0));
    assert!(slot.take().is_none(), "the look was taken");

    let halfway = later + TICK / 2;
    let mut camera = Camera::default();
    harness
        .world_view
        .update_camera(&mut camera, halfway, (true, true, true));
    assert!(
        (camera.position().x - 1.0).abs() < 1e-4,
        "{}",
        camera.position().x
    );
    assert!((camera.yaw - (1.0 + std::f32::consts::PI)).abs() < 1e-4);
    assert!((harness.world_view.view.partial_tick(halfway) - 0.5).abs() < 1e-4);
}

#[test]
fn apply_look_delta_updates_camera_immediately_without_waiting_for_game_thread() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    harness
        .frames
        .0
        .send(RenderEvent::Frame(frame(glam::Vec3::ZERO, tick, 0)))
        .expect("frame queue");
    harness.sync();

    let mut camera = Camera::default();
    harness
        .world_view
        .update_camera(&mut camera, tick, (false, false, false));
    let initial_yaw = camera.yaw;
    let initial_pitch = camera.pitch;

    // Apply raw mouse delta directly
    harness.world_view.apply_look_delta(0.25, -0.15);
    harness
        .world_view
        .update_camera(&mut camera, tick, (false, false, false));

    assert!((camera.yaw - (initial_yaw + 0.25)).abs() < 1e-4);
    assert!((camera.pitch - (initial_pitch + 0.15)).abs() < 1e-4);
}

#[test]
fn the_hand_sways_with_the_live_look_not_the_one_of_the_tick() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    let later = tick + TICK;
    for event in [
        RenderEvent::Frame(frame(glam::Vec3::ZERO, tick, 0)),
        RenderEvent::Frame(frame(glam::Vec3::new(2.0, 0.0, 0.0), later, 0)),
    ] {
        harness.frames.0.send(event).expect("frame queue");
    }
    harness.sync();
    let at_rest = harness.world_view.hand_pose(later).sway_yaw;
    assert!(at_rest.abs() < 1e-4, "{at_rest}");
    // The mouse turned 0.7 radians since the tick: 40 degrees against an
    // arm that has not moved, a tenth of it as sway.
    let mut turned = frame(glam::Vec3::new(2.0, 0.0, 0.0), later, 0).state;
    turned.yaw = 0.7;
    let slot = crate::game::LookSlot::default();
    harness
        .frames
        .0
        .send(RenderEvent::LookSlot(slot.clone()))
        .expect("frame queue");
    slot.store(crate::game::Look {
        view: turned,
        body_pitch: 0.0,
    });
    harness.sync();
    let swayed = harness.world_view.hand_pose(later).sway_yaw;
    let expected = 0.7_f32.to_degrees() * 0.1;
    assert!((swayed - expected).abs() < 1e-3, "{swayed} vs {expected}");
}

#[test]
fn a_look_from_a_tick_not_received_yet_does_not_move_the_camera() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    let later = tick + TICK;
    for event in [
        RenderEvent::Frame(frame(glam::Vec3::ZERO, tick, 0)),
        RenderEvent::Frame(frame(glam::Vec3::new(2.0, 0.0, 0.0), later, 0)),
    ] {
        harness.frames.0.send(event).expect("frame queue");
    }
    // The game thread already ticked once more (x = 4): its look reaches
    // the slot before its frame reaches the queue.
    let mut ahead = frame(glam::Vec3::new(4.0, 0.0, 0.0), later + TICK, 0).state;
    ahead.yaw = 0.5;
    let slot = crate::game::LookSlot::default();
    harness
        .frames
        .0
        .send(RenderEvent::LookSlot(slot.clone()))
        .expect("frame queue");
    slot.store(crate::game::Look {
        view: ahead,
        body_pitch: 0.0,
    });
    harness.sync();

    let mut camera = Camera::default();
    harness
        .world_view
        .update_camera(&mut camera, later + TICK / 2, (true, true, true));
    // Halfway between the eyes of the two ticks the view knows, not 3.
    assert!(
        (camera.position().x - 1.0).abs() < 1e-4,
        "{}",
        camera.position().x
    );
    assert!((camera.yaw - (0.5 + std::f32::consts::PI)).abs() < 1e-4);
}

#[test]
fn the_third_person_camera_orbits_the_interpolated_eye_with_the_newest_angles() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    let later = tick + TICK;
    // Tick 1: eye at the origin, camera 4 blocks behind it (south).
    let mut first = frame(glam::Vec3::ZERO, tick, 0);
    first.perspective = Perspective::ThirdPersonBack;
    first.state.position = glam::DVec3::new(0.0, 0.0, -4.0);
    // Tick 2: eye 2 blocks east, the player turned and the camera is now
    // 4 blocks east of the eye.
    let mut second = frame(glam::Vec3::new(2.0, 0.0, 0.0), later, 0);
    second.perspective = Perspective::ThirdPersonBack;
    second.state.position = glam::DVec3::new(6.0, 0.0, 0.0);
    second.state.orbit_start = glam::Vec3::new(4.0, 0.0, 0.0);
    for event in [RenderEvent::Frame(first), RenderEvent::Frame(second)] {
        harness.frames.0.send(event).expect("frame queue");
    }
    harness.sync();

    let mut camera = Camera::default();
    harness
        .world_view
        .update_camera(&mut camera, later + TICK / 2, (true, true, true));
    // `orientCamera`: eye at (1, 0, 0) plus the orbit of the newest angles.
    // Blending the two camera positions would give (3, 0, -2) and drag the
    // camera behind the turn.
    let expected = glam::Vec3::new(5.0, 0.0, 0.0);
    assert!(
        (camera.position() - expected).length() < 1e-4,
        "{}",
        camera.position()
    );
}

#[test]
fn the_camera_distance_follows_the_wall_while_the_eye_moves() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    let later = tick + TICK;
    let mut first = frame(glam::Vec3::ZERO, tick, 0);
    first.perspective = Perspective::ThirdPersonBack;
    // The eye walks 1 block towards a wall behind it: the camera was 4
    // blocks back from the old eye and is only 1 back from the new one.
    let mut second = frame(glam::Vec3::new(0.0, 0.0, 1.0), later, 0);
    second.perspective = Perspective::ThirdPersonBack;
    second.state.position = glam::DVec3::new(0.0, 0.0, 0.0);
    second.state.orbit_start = glam::Vec3::new(0.0, 0.0, -4.0);
    for event in [RenderEvent::Frame(first), RenderEvent::Frame(second)] {
        harness.frames.0.send(event).expect("frame queue");
    }
    harness.sync();

    // Each frame casts the rays from the eye of that frame: halfway the
    // eye is 0.5 along and the camera 2.5 behind it, not a step at the
    // tick that jumps from 4 to 1.
    let mut camera = Camera::default();
    harness
        .world_view
        .update_camera(&mut camera, later + TICK / 2, (true, true, true));
    assert!(
        (camera.position().z - (0.5 - 2.5)).abs() < 1e-4,
        "{}",
        camera.position().z
    );
}

#[test]
fn the_view_bobbing_option_gates_the_camera_sway() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    let mut walking = frame(glam::Vec3::ZERO, tick, 0);
    walking.state.effects.bob = umicraft_game::view::ViewBob::new(0.4, 0.1, 0.0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(walking))
        .expect("frame queue");
    harness.sync();

    let mut camera = Camera::default();
    harness
        .world_view
        .update_camera(&mut camera, tick, (true, true, true));
    assert_ne!(camera.view_bob(), glam::Mat4::IDENTITY, "bobbing on");
    harness
        .world_view
        .update_camera(&mut camera, tick, (false, true, true));
    assert_eq!(camera.view_bob(), glam::Mat4::IDENTITY, "bobbing off");
}

#[test]
fn the_no_hurt_cam_mod_leaves_the_roll_out_and_keeps_the_sway() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    let mut hurt = frame(glam::Vec3::ZERO, tick, 0);
    hurt.state.effects.hurt_roll_degrees = 9.0;
    hurt.state.effects.bob = umicraft_game::view::ViewBob::new(0.4, 0.1, 0.0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(hurt))
        .expect("frame queue");
    harness.sync();

    let mut camera = Camera::default();
    harness
        .world_view
        .update_camera(&mut camera, tick, (false, true, false));
    assert_eq!(camera.view_bob(), glam::Mat4::IDENTITY, "no roll, no sway");
    harness
        .world_view
        .update_camera(&mut camera, tick, (true, true, false));
    let swayed = camera.view_bob();
    harness
        .world_view
        .update_camera(&mut camera, tick, (true, true, true));
    assert_ne!(
        swayed,
        camera.view_bob(),
        "the roll comes back with the mod off"
    );
}

/// Regression: the roll of a hurt player went with the bobbing, so the camera
/// stopped shaking when "View Bobbing" was off; vanilla keeps it.
#[test]
fn the_hurt_roll_stays_when_view_bobbing_is_off() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    let mut hurt = frame(glam::Vec3::ZERO, tick, 0);
    hurt.state.effects.hurt_roll_degrees = 9.0;
    hurt.state.effects.bob = umicraft_game::view::ViewBob::new(0.4, 0.1, 0.0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(hurt))
        .expect("frame queue");
    harness.sync();

    let mut camera = Camera::default();
    harness
        .world_view
        .update_camera(&mut camera, tick, (false, true, true));
    assert_eq!(
        camera.view_bob(),
        glam::Mat4::from_rotation_z(9.0_f32.to_radians()),
        "off: the roll without the sway"
    );
    harness
        .world_view
        .update_camera(&mut camera, tick, (true, true, true));
    assert_ne!(
        camera.view_bob(),
        glam::Mat4::from_rotation_z(9.0_f32.to_radians()),
        "on: the sway comes with it"
    );
}

#[test]
fn the_body_is_only_reported_in_the_third_person() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    let mut first = frame(glam::Vec3::ZERO, tick, 0);
    first.state.eye = first.state.position;
    harness
        .frames
        .0
        .send(RenderEvent::Frame(first))
        .expect("frame queue");
    harness.sync();
    assert_eq!(
        harness.world_view.third_person_body(tick),
        None,
        "the camera is on the eye in the first person"
    );

    let mut third = frame(glam::Vec3::ZERO, tick + TICK, 1);
    third.perspective = Perspective::ThirdPersonBack;
    third.state.eye = third.state.position + glam::DVec3::new(0.0, 0.0, 4.0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(third))
        .expect("frame queue");
    harness.sync();
    let eye = harness
        .world_view
        .third_person_body(tick + TICK * 2)
        .expect("the camera orbits the player");
    assert!((eye.z - 4.0).abs() < 1e-6, "{eye}");
}

#[test]
fn the_dynamic_fov_option_gates_the_widening_of_the_view() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    let mut sprinting = frame(glam::Vec3::ZERO, tick, 0);
    sprinting.state.effects.fov_modifier = 1.3;
    harness
        .frames
        .0
        .send(RenderEvent::Frame(sprinting))
        .expect("frame queue");
    harness.sync();

    let mut dynamic = Camera::default();
    harness
        .world_view
        .update_camera(&mut dynamic, tick, (true, true, true));
    let mut fixed = Camera::default();
    harness
        .world_view
        .update_camera(&mut fixed, tick, (true, false, true));
    // The same camera with no widening at all.
    let mut plain = fixed;
    plain.set_view_effects(1.0, fixed.view_bob());

    let origin = glam::DVec3::ZERO;
    assert_ne!(
        dynamic.view_projection_from(origin),
        plain.view_projection_from(origin),
        "on, sprinting widens the view"
    );
    assert_eq!(
        fixed.view_projection_from(origin),
        plain.view_projection_from(origin),
        "off, the view stays as set"
    );
}

#[test]
fn the_camera_and_the_clock_interpolate_between_the_two_newest_frames() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    let start = glam::Vec3::new(0.0, 64.0, 0.0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(frame(start, tick, -6_000)))
        .expect("frame queue");
    harness.sync();
    // One frame is not enough to interpolate: the camera shows it and
    // the clock fraction is zero even when `now` is ahead of the tick.
    let mut camera = Camera::default();
    harness
        .world_view
        .update_camera(&mut camera, tick + TICK * 2, (true, true, true));
    assert_eq!(camera.position(), start);
    assert_eq!(
        harness
            .world_view
            .environment(tick + TICK * 2, 12)
            .partial_tick,
        0.0
    );

    let later = tick + TICK;
    let end = glam::Vec3::new(2.0, 64.0, 0.0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(frame(end, later, -5_900)))
        .expect("frame queue");
    harness.sync();
    assert_eq!(harness.world_view.frame_count(), 2);

    let halfway = later + TICK / 2;
    harness
        .world_view
        .update_camera(&mut camera, halfway, (true, true, true));
    assert!((camera.position().x - 1.0).abs() < 1e-3);
    let environment = harness.world_view.environment(halfway, 12);
    assert!((environment.partial_tick - 0.5).abs() < 1e-3);
    assert_eq!(environment.world_time, -5_900);
    assert_eq!(environment.dimension, -1);
    // The world type of the frame reaches the sky and the fog.
    assert!(environment.flat_world);
    assert_eq!(environment.view_distance, 12);

    // Before the newest tick and after a full tick the fraction is
    // clamped to the interpolation range.
    assert_eq!(
        harness
            .world_view
            .environment(later - TICK, 12)
            .partial_tick,
        0.0
    );
    assert_eq!(
        harness
            .world_view
            .environment(later + TICK * 3, 12)
            .partial_tick,
        1.0
    );
}

#[test]
fn teleport_event_snaps_camera_immediately_without_gliding() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    harness
        .frames
        .0
        .send(RenderEvent::Frame(frame(glam::Vec3::ZERO, tick, 0)))
        .expect("frame queue");
    harness.sync();

    let initial_sample = harness.world_view.view.sample(tick).expect("sample");
    assert_eq!(initial_sample.position, glam::DVec3::ZERO);

    // Server issues a setback or teleport 15 blocks away
    let target = glam::DVec3::new(15.0, 64.0, -10.0);
    let yaw = 1.23_f32;
    let pitch = -0.45_f32;
    harness
        .frames
        .0
        .send(RenderEvent::Teleport {
            position: target,
            yaw: crate::game::ViewAngle::To(yaw),
            pitch: crate::game::ViewAngle::To(pitch),
        })
        .expect("teleport event");
    harness.sync();

    // The camera should immediately sample the exact target position without gliding
    let snapped_sample = harness
        .world_view
        .view
        .sample(Instant::now())
        .expect("sample");
    assert_eq!(snapped_sample.position, target);
    assert!((snapped_sample.yaw - yaw).abs() < 1e-5);
    assert!((snapped_sample.pitch - pitch).abs() < 1e-5);
}

#[test]
fn local_mouse_look_prevents_rollback_from_stale_ticks_and_echoes() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    harness
        .frames
        .0
        .send(RenderEvent::Frame(frame(glam::Vec3::ZERO, tick, 0)))
        .expect("frame queue");
    harness.sync();

    // 1. User applies zero-latency mouse look
    let yaw_delta = 0.5_f32;
    let pitch_delta = -0.2_f32;
    harness.world_view.apply_look_delta(yaw_delta, pitch_delta);

    let current_yaw = harness.world_view.view.current.unwrap().yaw;
    let current_pitch = harness.world_view.view.current.unwrap().pitch;
    assert!((current_yaw - yaw_delta).abs() < 1e-5);
    assert!((current_pitch - pitch_delta).abs() < 1e-5);

    // 2. A tick arrives from the game thread with stale 0.0 yaw
    let later = tick + TICK;
    harness
        .frames
        .0
        .send(RenderEvent::Frame(frame(
            glam::Vec3::new(1.0, 0.0, 0.0),
            later,
            0,
        )))
        .expect("frame queue");
    harness.sync();

    // Camera must preserve the live mouse angles and NOT roll back to 0.0!
    let preserved_yaw = harness.world_view.view.current.unwrap().yaw;
    let preserved_pitch = harness.world_view.view.current.unwrap().pitch;
    assert!(
        (preserved_yaw - yaw_delta).abs() < 1e-5,
        "{preserved_yaw} vs {yaw_delta}"
    );
    assert!(
        (preserved_pitch - pitch_delta).abs() < 1e-5,
        "{preserved_pitch} vs {pitch_delta}"
    );

    // 3. A stale echo arrives via LookSlot
    let slot = crate::game::LookSlot::default();
    harness
        .frames
        .0
        .send(RenderEvent::LookSlot(slot.clone()))
        .expect("look slot");
    let mut stale_look = frame(glam::Vec3::ZERO, later, 0).state;
    stale_look.yaw = 0.05; // Delayed angle from earlier input
    slot.store(crate::game::Look {
        view: stale_look,
        body_pitch: 0.0,
    });
    harness.sync();

    // The camera must STILL keep the live mouse angles and NOT roll back to 0.05!
    let final_yaw = harness.world_view.view.current.unwrap().yaw;
    assert!(
        (final_yaw - yaw_delta).abs() < 1e-5,
        "{final_yaw} vs {yaw_delta}"
    );
}

#[test]
fn third_person_back_orbits_correctly_with_raw_mouse_look() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    let eye = glam::DVec3::new(10.0, 64.0, 20.0);
    let mut f1 = frame(eye.as_vec3(), tick, 0);
    f1.perspective = Perspective::ThirdPersonBack;
    f1.state.eye = eye;
    f1.state.position = eye + glam::DVec3::new(0.0, 0.0, -4.0);
    f1.state.orbit_start = glam::Vec3::new(0.0, 0.0, -4.0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f1))
        .expect("frame");
    harness.sync();

    let s = harness.world_view.view.sample(tick).expect("sample");
    assert!((s.position - (eye + glam::DVec3::new(0.0, 0.0, -4.0))).length() < 1e-4);

    // Turn mouse 90 degrees right
    harness
        .world_view
        .apply_look_delta(std::f32::consts::FRAC_PI_2, 0.0);
    let s2 = harness.world_view.view.sample(tick).expect("sample");
    // At yaw = PI/2, look_direction is (-1, 0, 0); in back view, camera is at eye - (-1, 0, 0)*4 = eye + (4, 0, 0)
    assert!(
        (s2.position - (eye + glam::DVec3::new(4.0, 0.0, 0.0))).length() < 1e-4,
        "got {:?}",
        s2.position
    );
    assert!((s2.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-4);

    // Player model must face player_yaw
    let body = harness.world_view.local_body().expect("local body");
    assert!((body.yaw - 90.0).abs() < 1e-3);
}

#[test]
fn third_person_front_faces_player_face_and_orbits_with_mouse() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    let eye = glam::DVec3::new(10.0, 64.0, 20.0);
    let mut f1 = frame(eye.as_vec3(), tick, 0);
    f1.perspective = Perspective::ThirdPersonFront;
    f1.state.eye = eye;
    f1.state.position = eye + glam::DVec3::new(0.0, 0.0, 4.0); // 4 blocks south (in front of south-facing player)
    f1.state.yaw = std::f32::consts::PI;
    f1.state.pitch = 0.0;
    f1.state.orbit_start = glam::Vec3::new(0.0, 0.0, 4.0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f1))
        .expect("frame");
    harness.sync();

    let s = harness.world_view.view.sample(tick).expect("sample");
    assert!((s.position - (eye + glam::DVec3::new(0.0, 0.0, 4.0))).length() < 1e-4);
    assert!((s.yaw - std::f32::consts::PI).abs() < 1e-4);

    // Player body must face 0.0 (towards camera, face visible!)
    let body = harness.world_view.local_body().expect("local body");
    assert!(body.yaw.abs() < 1e-3, "player yaw: {}", body.yaw);

    // Move mouse look
    harness.world_view.apply_look_delta(0.4, -0.2);
    let s2 = harness.world_view.view.sample(tick).expect("sample");
    assert!((s2.yaw - (0.4 + std::f32::consts::PI)).abs() < 1e-4);
    assert!((s2.pitch - 0.2).abs() < 1e-4);

    // Player body and head must follow true player angles (0.4, -0.2)
    let body2 = harness.world_view.local_body().expect("local body");
    assert!((body2.yaw - 0.4_f32.to_degrees()).abs() < 1e-3);
    assert!((body2.pitch - (-0.2_f32.to_degrees())).abs() < 1e-3);
}

#[test]
fn rapid_perspective_switching_f5_cuts_cleanly_without_gliding_or_desync() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    let eye = glam::DVec3::new(0.0, 64.0, 0.0);

    // 1. FirstPerson
    let mut f1 = frame(eye.as_vec3(), tick, 0);
    f1.perspective = Perspective::FirstPerson;
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f1))
        .expect("frame");
    harness.sync();
    let s1 = harness.world_view.view.sample(tick).expect("sample");
    assert_eq!(s1.position, eye);

    // 2. Switch to ThirdPersonBack
    let later = tick + TICK;
    let mut f2 = frame(eye.as_vec3(), later, 1);
    f2.perspective = Perspective::ThirdPersonBack;
    f2.state.position = eye + glam::DVec3::new(0.0, 0.0, -4.0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f2))
        .expect("frame");
    harness.sync();

    // Sample halfway: must NOT glide from (0, 64, 0) to (0, 64, -4); must cut instantly to -4!
    let s2 = harness
        .world_view
        .view
        .sample(later + TICK / 2)
        .expect("sample");
    assert!(
        (s2.position.z - (eye.z - 4.0)).abs() < 1e-4,
        "perspective switch must cut immediately: {}",
        s2.position
    );

    // 3. Switch to ThirdPersonFront
    let later2 = later + TICK;
    let mut f3 = frame(eye.as_vec3(), later2, 2);
    f3.perspective = Perspective::ThirdPersonFront;
    f3.state.position = eye + glam::DVec3::new(0.0, 0.0, 4.0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f3))
        .expect("frame");
    harness.sync();
    let s3 = harness
        .world_view
        .view
        .sample(later2 + TICK / 2)
        .expect("sample");
    assert!(
        (s3.position.z - (eye.z + 4.0)).abs() < 1e-4,
        "perspective switch must cut immediately: {}",
        s3.position
    );

    // 4. Switch back to FirstPerson
    let later3 = later2 + TICK;
    let mut f4 = frame(eye.as_vec3(), later3, 3);
    f4.perspective = Perspective::FirstPerson;
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f4))
        .expect("frame");
    harness.sync();
    let s4 = harness
        .world_view
        .view
        .sample(later3 + TICK / 2)
        .expect("sample");
    assert_eq!(s4.position, eye);
}

#[test]
fn server_setback_during_f5_teleports_orbit_and_camera_instantly() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    let eye = glam::DVec3::new(0.0, 64.0, 0.0);
    let mut f1 = frame(eye.as_vec3(), tick, 0);
    f1.perspective = Perspective::ThirdPersonBack;
    f1.state.position = eye + glam::DVec3::new(0.0, 0.0, -4.0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f1))
        .expect("frame");
    harness.sync();

    // Server setback / teleport to new location
    let target_eye = glam::DVec3::new(100.0, 80.0, -200.0);
    let yaw = 0.8_f32;
    let pitch = 0.2_f32;
    harness
        .frames
        .0
        .send(RenderEvent::Teleport {
            position: target_eye,
            yaw: crate::game::ViewAngle::To(yaw),
            pitch: crate::game::ViewAngle::To(pitch),
        })
        .expect("teleport");
    harness.sync();

    let s = harness
        .world_view
        .view
        .sample(Instant::now())
        .expect("sample");
    assert_eq!(s.eye, target_eye);
    assert!((s.yaw - yaw).abs() < 1e-4);
    assert!((s.pitch - pitch).abs() < 1e-4);

    let expected_dir = look_direction(yaw, pitch) * -1.0;
    let expected_pos = target_eye + expected_dir * 4.0;
    assert!(
        (s.position - expected_pos).length() < 1e-4,
        "got {:?}, expected {:?}",
        s.position,
        expected_pos
    );
}

#[test]
fn full_360_degree_mouse_look_rotation_in_both_f5_modes() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    let eye = glam::DVec3::new(5.0, 64.0, 5.0);

    for perspective in [Perspective::ThirdPersonBack, Perspective::ThirdPersonFront] {
        let mut f = frame(eye.as_vec3(), tick, 0);
        f.perspective = perspective;
        let dist_vec = if perspective == Perspective::ThirdPersonBack {
            glam::DVec3::new(0.0, 0.0, -4.0)
        } else {
            glam::DVec3::new(0.0, 0.0, 4.0)
        };
        f.state.position = eye + dist_vec;
        harness.frames.0.send(RenderEvent::Frame(f)).expect("frame");
        harness.sync();

        let step = std::f32::consts::TAU / 12.0;
        for _ in 0..12 {
            harness.world_view.apply_look_delta(step, 0.0);
            let s = harness.world_view.view.sample(tick).expect("sample");
            assert!(!s.yaw.is_nan());
            assert!(!s.pitch.is_nan());
            assert!(!s.position.x.is_nan());
            let dist = (s.position - eye).length();
            assert!((dist - 4.0).abs() < 1e-4, "orbit distance broken: {dist}");
        }
    }
}

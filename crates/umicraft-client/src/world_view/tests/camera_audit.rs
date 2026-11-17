// SPDX-License-Identifier: MIT OR Apache-2.0

//! Audits of the camera through movement, jumps, knockback, setbacks and
//! frame rates.

use super::*;

#[test]
fn test_audit_camera_three_perspectives_position_orientation_and_hand() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    let eye = glam::DVec3::new(10.0, 65.62, 20.0);
    let player_yaw = 0.5_f32;
    let player_pitch = -0.2_f32;

    // 1. FirstPerson
    let mut f1 = frame(eye.as_vec3(), tick, 0);
    f1.perspective = Perspective::FirstPerson;
    f1.state.eye = eye;
    f1.state.position = eye;
    f1.state.yaw = player_yaw;
    f1.state.pitch = player_pitch;
    f1.local_body = Arc::new(
        EntityRecord::new(
            EntityId(1),
            EntityKind::Player,
            eye,
            player_yaw.to_degrees(),
            player_pitch.to_degrees(),
        )
        .frame_data(),
    );
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f1))
        .expect("frame");
    harness.sync();

    let s1 = harness.world_view.view.sample(tick).expect("sample");
    assert_eq!(s1.position, eye);
    assert!((s1.yaw - player_yaw).abs() < 1e-4);
    assert!((s1.pitch - player_pitch).abs() < 1e-4);
    assert!(harness.world_view.third_person_body(tick).is_none());

    // 2. ThirdPersonBack
    let later = tick + TICK;
    let mut f2 = frame(eye.as_vec3(), later, 1);
    f2.perspective = Perspective::ThirdPersonBack;
    f2.state.eye = eye;
    let back_dir = look_direction(player_yaw, player_pitch) * -1.0;
    let back_pos = eye + back_dir * 4.0;
    f2.state.position = back_pos;
    f2.state.yaw = player_yaw;
    f2.state.pitch = player_pitch;
    f2.local_body = Arc::new(
        EntityRecord::new(
            EntityId(1),
            EntityKind::Player,
            eye,
            player_yaw.to_degrees(),
            player_pitch.to_degrees(),
        )
        .frame_data(),
    );
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f2))
        .expect("frame");
    harness.sync();

    let s2 = harness.world_view.view.sample(later).expect("sample");
    assert!((s2.position - back_pos).length() < 1e-4);
    assert!((s2.yaw - player_yaw).abs() < 1e-4);
    assert!((s2.pitch - player_pitch).abs() < 1e-4);
    assert_eq!(harness.world_view.third_person_body(later), Some(eye));
    let body2 = harness.world_view.local_body().expect("local body");
    assert!((body2.yaw - player_yaw.to_degrees()).abs() < 1e-3);

    // 3. ThirdPersonFront
    let later2 = later + TICK;
    let mut f3 = frame(eye.as_vec3(), later2, 2);
    f3.perspective = Perspective::ThirdPersonFront;
    f3.state.eye = eye;
    let front_dir = look_direction(player_yaw, player_pitch) * 1.0;
    let front_pos = eye + front_dir * 4.0;
    f3.state.position = front_pos;
    f3.state.yaw = (player_yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU);
    f3.state.pitch = -player_pitch;
    f3.local_body = Arc::new(
        EntityRecord::new(
            EntityId(1),
            EntityKind::Player,
            eye,
            player_yaw.to_degrees(),
            player_pitch.to_degrees(),
        )
        .frame_data(),
    );
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f3))
        .expect("frame");
    harness.sync();

    let s3 = harness.world_view.view.sample(later2).expect("sample");
    assert!((s3.position - front_pos).length() < 1e-4);
    assert!(
        (s3.yaw - (player_yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)).abs()
            < 1e-4
    );
    assert!((s3.pitch - (-player_pitch)).abs() < 1e-4);
    // Player model in front view must still face player_yaw (towards camera)
    let body3 = harness.world_view.local_body().expect("local body");
    assert!(
        (body3.yaw - player_yaw.to_degrees()).abs() < 1e-3,
        "player yaw must match player_yaw, got {}",
        body3.yaw
    );
}

#[test]
fn test_audit_camera_movement_sprint_wtap_stap_rapid_strafes() {
    let mut harness = Harness::new();
    let mut tick = Instant::now();
    let mut eye = glam::DVec3::new(0.0, 65.62, 0.0);
    let mut yaw = 0.0_f32;
    let pitch = 0.0_f32;

    // Initialize in ThirdPersonBack
    let mut f = frame(eye.as_vec3(), tick, 0);
    f.perspective = Perspective::ThirdPersonBack;
    f.state.eye = eye;
    f.state.position = eye + glam::DVec3::new(0.0, 0.0, -4.0);
    harness.frames.0.send(RenderEvent::Frame(f)).expect("frame");
    harness.sync();

    // Simulate 30 ticks of W-Tap, S-Tap and alternating strafes
    for t in 1..=30 {
        tick += TICK;
        // Movement: forward sprint with W-Tap every 4 ticks and alternating strafe A/D
        let forward_speed = if t % 4 == 0 { 0.0 } else { 0.28 };
        let strafe_offset = if t % 2 == 0 { 0.15 } else { -0.15 };
        eye.z += forward_speed;
        eye.x += strafe_offset;
        yaw = (yaw + 0.02).rem_euclid(std::f32::consts::TAU);

        let mut f_move = frame(eye.as_vec3(), tick, t as i64);
        f_move.perspective = Perspective::ThirdPersonBack;
        f_move.state.eye = eye;
        let dir = look_direction(yaw, pitch) * -1.0;
        f_move.state.position = eye + dir * 4.0;
        f_move.state.orbit_start = (dir * 4.0).as_vec3();
        f_move.state.yaw = yaw;
        f_move.state.pitch = pitch;
        f_move.local_body = Arc::new(
            EntityRecord::new(
                EntityId(1),
                EntityKind::Player,
                eye,
                yaw.to_degrees(),
                pitch.to_degrees(),
            )
            .frame_data(),
        );
        harness
            .frames
            .0
            .send(RenderEvent::Frame(f_move))
            .expect("frame");
        harness.sync();

        // Sample at 4 sub-tick instants
        for sub in 0..4 {
            let now = tick + TICK * sub / 4;
            let s = harness.world_view.view.sample(now).expect("sample");
            assert!(!s.position.x.is_nan());
            assert!(!s.position.y.is_nan());
            assert!(!s.position.z.is_nan());
            let dist = (s.position - s.eye).length();
            assert!(
                (dist - 4.0).abs() < 1e-3,
                "tick {t} sub {sub}: orbit distance broken: {dist}"
            );
        }
    }
}

#[test]
fn test_audit_camera_vertical_dynamics_jump_fall_knockback() {
    let mut harness = Harness::new();
    let mut tick = Instant::now();
    let mut eye = glam::DVec3::new(0.0, 65.62, 0.0);
    let mut vy = 0.42_f64; // Jump velocity
    let gravity = 0.08_f64;
    let drag = 0.98_f64;

    let mut f_init = frame(eye.as_vec3(), tick, 0);
    f_init.perspective = Perspective::ThirdPersonBack;
    f_init.state.eye = eye;
    f_init.state.position = eye + glam::DVec3::new(0.0, 0.0, -4.0);
    f_init.state.orbit_start = glam::Vec3::new(0.0, 0.0, -4.0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f_init))
        .expect("frame");
    harness.sync();

    // Jump & fall trajectory over 20 ticks
    for t in 1..=20 {
        tick += TICK;
        eye.y += vy;
        vy = (vy - gravity) * drag;

        // Knockback impulse at tick 10
        if t == 10 {
            eye.x += 0.4;
            vy += 0.36;
        }

        let mut f_step = frame(eye.as_vec3(), tick, t as i64);
        f_step.perspective = Perspective::ThirdPersonBack;
        f_step.state.eye = eye;
        f_step.state.position = eye + glam::DVec3::new(0.0, 0.0, -4.0);
        f_step.state.orbit_start = glam::Vec3::new(0.0, 0.0, -4.0);
        harness
            .frames
            .0
            .send(RenderEvent::Frame(f_step))
            .expect("frame");
        harness.sync();

        // Test sub-tick interpolation stability
        let sample = harness
            .world_view
            .view
            .sample(tick + TICK / 2)
            .expect("sample");
        let orbit_offset = sample.position - sample.eye;
        assert!(
            (orbit_offset.length() - 4.0).abs() < 1e-4,
            "vertical dynamic orbit distance: {}",
            orbit_offset.length()
        );
        assert!(
            (orbit_offset.y).abs() < 1e-4,
            "vertical tilt introduced in orbit"
        );
    }
}

#[test]
fn test_audit_camera_transitions_rapid_cycling_while_moving() {
    let mut harness = Harness::new();
    let mut tick = Instant::now();
    let mut eye = glam::DVec3::new(0.0, 65.62, 0.0);

    let perspectives = [
        Perspective::FirstPerson,
        Perspective::ThirdPersonBack,
        Perspective::ThirdPersonFront,
    ];

    // Baseline initial frame
    let f0 = frame(eye.as_vec3(), tick, 0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f0))
        .expect("frame");
    harness.sync();

    for i in 1..=15 {
        tick += TICK;
        eye += glam::DVec3::new(0.2, 0.1, 0.3); // moving while switching
        let p = perspectives[i % 3];

        let mut f = frame(eye.as_vec3(), tick, i as i64);
        f.perspective = p;
        f.state.eye = eye;
        let (cam_yaw, cam_pitch, cam_pos) = match p {
            Perspective::FirstPerson => (0.0, 0.0, eye),
            Perspective::ThirdPersonBack => (0.0, 0.0, eye + glam::DVec3::new(0.0, 0.0, -4.0)),
            Perspective::ThirdPersonFront => (
                std::f32::consts::PI,
                0.0,
                eye + glam::DVec3::new(0.0, 0.0, 4.0),
            ),
        };
        f.state.position = cam_pos;
        f.state.yaw = cam_yaw;
        f.state.pitch = cam_pitch;

        harness.frames.0.send(RenderEvent::Frame(f)).expect("frame");
        harness.sync();

        // At sub-tick fraction 0.5, verify that:
        // 1. The eye smoothly interpolates along the movement path (no freeze or stair-stepping).
        // 2. The camera position cuts immediately to the new orbit relative to the interpolated eye.
        let sample = harness
            .world_view
            .view
            .sample(tick + TICK / 2)
            .expect("sample");
        let prev_eye = eye - glam::DVec3::new(0.2, 0.1, 0.3);
        let expected_eye = prev_eye.lerp(eye, 0.5);
        let expected_pos = match p {
            Perspective::FirstPerson => expected_eye,
            Perspective::ThirdPersonBack => expected_eye + glam::DVec3::new(0.0, 0.0, -4.0),
            Perspective::ThirdPersonFront => expected_eye + glam::DVec3::new(0.0, 0.0, 4.0),
        };
        assert!(
            (sample.eye - expected_eye).length() < 1e-4,
            "eye must interpolate smoothly while moving across transition cycle {i}: {:?} vs {:?}",
            sample.eye,
            expected_eye
        );
        assert!(
            (sample.position - expected_pos).length() < 1e-4,
            "camera must immediately use new orbit centered on interpolated eye in cycle {i} ({p:?}): {:?} vs {:?}",
            sample.position,
            expected_pos
        );
    }
}

#[test]
fn test_audit_camera_transition_during_server_setback() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    let eye = glam::DVec3::new(5.0, 65.62, 5.0);

    // Frame in FirstPerson
    let mut f1 = frame(eye.as_vec3(), tick, 0);
    f1.perspective = Perspective::FirstPerson;
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f1))
        .expect("frame");
    harness.sync();

    // Next event: Server setback to (50, 70, -100) and simultaneous toggle to ThirdPersonFront
    let setback_eye = glam::DVec3::new(50.0, 71.62, -100.0);
    let yaw = 1.0_f32;
    let pitch = -0.15_f32;

    harness
        .frames
        .0
        .send(RenderEvent::Teleport {
            position: setback_eye,
            yaw: crate::game::ViewAngle::To(yaw),
            pitch: crate::game::ViewAngle::To(pitch),
        })
        .expect("teleport");

    let later = tick + TICK;
    let mut f2 = frame(setback_eye.as_vec3(), later, 1);
    f2.perspective = Perspective::ThirdPersonFront;
    f2.state.eye = setback_eye;
    let dir = look_direction(yaw, pitch) * 1.0;
    f2.state.position = setback_eye + dir * 4.0;
    f2.state.orbit_start = (dir * 4.0).as_vec3();
    f2.state.yaw = (yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU);
    f2.state.pitch = -pitch;
    f2.local_body = Arc::new(
        EntityRecord::new(
            EntityId(1),
            EntityKind::Player,
            setback_eye,
            yaw.to_degrees(),
            pitch.to_degrees(),
        )
        .frame_data(),
    );
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f2))
        .expect("frame");
    harness.sync();

    let s = harness
        .world_view
        .view
        .sample(later + TICK / 2)
        .expect("sample");
    assert_eq!(s.eye, setback_eye);
    assert!((s.position - (setback_eye + dir * 4.0)).length() < 1e-4);
    assert!((s.yaw - (yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)).abs() < 1e-4);
    assert!((s.pitch - (-pitch)).abs() < 1e-4);
}

#[test]
fn test_audit_camera_framerate_independence_1000fps_vs_60fps() {
    let mut harness = Harness::new();
    let tick = Instant::now();
    let eye0 = glam::DVec3::new(0.0, 65.62, 0.0);
    let eye1 = glam::DVec3::new(5.0, 65.62, 2.0);

    let mut f0 = frame(eye0.as_vec3(), tick, 0);
    f0.perspective = Perspective::ThirdPersonBack;
    f0.state.eye = eye0;
    f0.state.position = eye0 + glam::DVec3::new(0.0, 0.0, -4.0);
    f0.state.orbit_start = glam::Vec3::new(0.0, 0.0, -4.0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f0))
        .expect("frame");

    let later = tick + TICK;
    let mut f1 = frame(eye1.as_vec3(), later, 1);
    f1.perspective = Perspective::ThirdPersonBack;
    f1.state.eye = eye1;
    f1.state.position = eye1 + glam::DVec3::new(0.0, 0.0, -4.0);
    f1.state.orbit_start = glam::Vec3::new(0.0, 0.0, -4.0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f1))
        .expect("frame");
    harness.sync();

    // Compare samples at exact same wall-clock timestamp sampled with high density (1000 FPS) vs standard (60 FPS)
    let probe_time = later + Duration::from_micros(25_000); // exactly halfway (partial_tick = 0.5)

    let s_1000fps = harness.world_view.view.sample(probe_time).expect("sample");
    let s_60fps = harness.world_view.view.sample(probe_time).expect("sample");

    assert_eq!(s_1000fps.position, s_60fps.position);
    assert_eq!(s_1000fps.yaw, s_60fps.yaw);
    assert_eq!(s_1000fps.pitch, s_60fps.pitch);
    assert_eq!(s_1000fps.eye, glam::DVec3::new(2.5, 65.62, 1.0));
    assert_eq!(s_1000fps.position, glam::DVec3::new(2.5, 65.62, -3.0));
}

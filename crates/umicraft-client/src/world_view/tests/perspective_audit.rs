// SPDX-License-Identifier: MIT OR Apache-2.0

//! Audits of the perspective key (F5) pressed quickly while the player
//! moves, jumps, looks around or is set back.

use super::*;

#[test]
fn test_audit_f5_rapid_toggle_stationary() {
    let mut harness = Harness::new();
    let mut tick = Instant::now();
    let eye = glam::DVec3::new(10.0, 64.0, 10.0);

    let perspectives = [
        Perspective::FirstPerson,
        Perspective::ThirdPersonBack,
        Perspective::ThirdPersonFront,
    ];

    // Send baseline frame
    let f0 = frame(eye.as_vec3(), tick, 0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f0))
        .expect("frame");
    harness.sync();

    for i in 1..=10 {
        tick += TICK;
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

        for alpha_step in [0.0, 0.5, 1.0] {
            let sample_time = tick + Duration::from_secs_f32(0.05 * alpha_step);
            let sample = harness.world_view.view.sample(sample_time).expect("sample");
            assert_eq!(sample.eye, eye, "stationary eye must remain unchanged");
            let expected_dist = if p == Perspective::FirstPerson {
                0.0
            } else {
                4.0
            };
            assert!(
                ((sample.position - sample.eye).length() - expected_dist).abs() < 1e-4,
                "orbit distance must be exact"
            );
        }
    }
}

#[test]
fn test_audit_f5_rapid_toggle_moving() {
    let mut harness = Harness::new();
    let mut tick = Instant::now();
    let mut eye = glam::DVec3::new(0.0, 64.0, 0.0);
    let move_delta = glam::DVec3::new(0.3, 0.0, 0.4);

    let perspectives = [
        Perspective::FirstPerson,
        Perspective::ThirdPersonBack,
        Perspective::ThirdPersonFront,
    ];

    let f0 = frame(eye.as_vec3(), tick, 0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f0))
        .expect("frame");
    harness.sync();

    for i in 1..=10 {
        let prev_eye = eye;
        eye += move_delta;
        tick += TICK;
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

        let sample = harness
            .world_view
            .view
            .sample(tick + TICK / 2)
            .expect("sample");
        let expected_eye = prev_eye.lerp(eye, 0.5);
        assert!(
            (sample.eye - expected_eye).length() < 1e-4,
            "eye must interpolate continuously without freeze: {:?} vs {:?}",
            sample.eye,
            expected_eye
        );
        let expected_pos = expected_eye
            + match p {
                Perspective::FirstPerson => glam::DVec3::ZERO,
                Perspective::ThirdPersonBack => glam::DVec3::new(0.0, 0.0, -4.0),
                Perspective::ThirdPersonFront => glam::DVec3::new(0.0, 0.0, 4.0),
            };
        assert!(
            (sample.position - expected_pos).length() < 1e-4,
            "camera must be placed at new orbit offset from interpolated eye"
        );
    }
}

#[test]
fn test_audit_f5_rapid_toggle_during_jump() {
    let mut harness = Harness::new();
    let mut tick = Instant::now();
    let jump_heights = [
        64.0, 64.42, 64.75, 65.00, 65.17, 65.25, 65.24, 65.13, 64.93, 64.63,
    ];

    let perspectives = [
        Perspective::FirstPerson,
        Perspective::ThirdPersonBack,
        Perspective::ThirdPersonFront,
    ];

    let mut eye = glam::DVec3::new(0.0, jump_heights[0], 0.0);
    let f0 = frame(eye.as_vec3(), tick, 0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f0))
        .expect("frame");
    harness.sync();

    for i in 1..jump_heights.len() {
        let prev_eye = eye;
        eye = glam::DVec3::new(0.0, jump_heights[i], 0.0);
        tick += TICK;
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

        for fraction in [0.25, 0.5, 0.75] {
            let sample_time = tick + Duration::from_secs_f32(0.05 * fraction);
            let sample = harness.world_view.view.sample(sample_time).expect("sample");
            let expected_y = prev_eye.y + (eye.y - prev_eye.y) * f64::from(fraction);
            assert!(
                (sample.eye.y - expected_y).abs() < 1e-4,
                "jump height must interpolate smoothly: got {}, expected {}",
                sample.eye.y,
                expected_y
            );
            let expected_cam_y = expected_y; // pitch = 0, orbit is horizontal
            assert!(
                (sample.position.y - expected_cam_y).abs() < 1e-4,
                "camera height must match jump height without sudden leap"
            );
        }
    }
}

#[test]
fn test_audit_f5_spam_movement_and_jump() {
    let mut harness = Harness::new();
    let mut tick = Instant::now();
    let jump_heights = [
        64.0, 64.42, 64.75, 65.00, 65.17, 65.25, 65.24, 65.13, 64.93, 64.63, 64.25, 64.0,
    ];
    let mut eye = glam::DVec3::new(0.0, jump_heights[0], 0.0);

    let perspectives = [
        Perspective::FirstPerson,
        Perspective::ThirdPersonBack,
        Perspective::ThirdPersonFront,
    ];

    let f0 = frame(eye.as_vec3(), tick, 0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f0))
        .expect("frame");
    harness.sync();

    for i in 1..jump_heights.len() {
        let prev_eye = eye;
        eye = glam::DVec3::new(i as f64 * 0.28, jump_heights[i], 0.0);
        tick += TICK;
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

        let sample = harness
            .world_view
            .view
            .sample(tick + TICK / 2)
            .expect("sample");
        let expected_eye = prev_eye.lerp(eye, 0.5);
        assert!(
            (sample.eye - expected_eye).length() < 1e-4,
            "combined 3D movement + jump must interpolate smoothly"
        );
        let expected_dist = if p == Perspective::FirstPerson {
            0.0
        } else {
            4.0
        };
        assert!(
            ((sample.position - sample.eye).length() - expected_dist).abs() < 1e-4,
            "orbit distance must remain constant during jump + movement"
        );
    }
}

#[test]
fn test_audit_f5_spam_movement_and_mouse_look() {
    let mut harness = Harness::new();
    let mut tick = Instant::now();
    let mut eye = glam::DVec3::new(0.0, 64.0, 0.0);

    let perspectives = [
        Perspective::FirstPerson,
        Perspective::ThirdPersonBack,
        Perspective::ThirdPersonFront,
    ];

    let f0 = frame(eye.as_vec3(), tick, 0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f0))
        .expect("frame");
    harness.sync();

    for i in 1..=10 {
        eye += glam::DVec3::new(0.25, 0.0, 0.1);
        tick += TICK;
        let p = perspectives[i % 3];

        let mut f = frame(eye.as_vec3(), tick, i as i64);
        f.perspective = p;
        f.state.eye = eye;
        harness.frames.0.send(RenderEvent::Frame(f)).expect("frame");
        harness.sync();

        // Direct raw mouse look delta applied on the render thread
        harness.world_view.apply_look_delta(0.1, -0.05);

        let sample = harness
            .world_view
            .view
            .sample(tick + TICK / 2)
            .expect("sample");
        let expected_dist = if p == Perspective::FirstPerson {
            0.0
        } else {
            4.0
        };
        assert!(
            ((sample.position - sample.eye).length() - expected_dist).abs() < 1e-4,
            "raw mouse look must maintain exact orbit distance across perspective toggles"
        );
    }
}

#[test]
fn test_audit_f5_spam_jump_and_mouse_look() {
    let mut harness = Harness::new();
    let mut tick = Instant::now();
    let jump_heights = [64.0, 64.42, 64.75, 65.00, 65.17, 65.25];
    let mut eye = glam::DVec3::new(0.0, jump_heights[0], 0.0);

    let perspectives = [
        Perspective::FirstPerson,
        Perspective::ThirdPersonBack,
        Perspective::ThirdPersonFront,
    ];

    let f0 = frame(eye.as_vec3(), tick, 0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f0))
        .expect("frame");
    harness.sync();

    for i in 1..jump_heights.len() {
        eye = glam::DVec3::new(0.0, jump_heights[i], 0.0);
        tick += TICK;
        let p = perspectives[i % 3];

        let mut f = frame(eye.as_vec3(), tick, i as i64);
        f.perspective = p;
        f.state.eye = eye;
        harness.frames.0.send(RenderEvent::Frame(f)).expect("frame");
        harness.sync();

        // Continuous 360-degree rotation step
        harness.world_view.apply_look_delta(0.5, 0.1);

        let sample = harness
            .world_view
            .view
            .sample(tick + TICK / 2)
            .expect("sample");
        let expected_dist = if p == Perspective::FirstPerson {
            0.0
        } else {
            4.0
        };
        assert!(
            ((sample.position - sample.eye).length() - expected_dist).abs() < 1e-4,
            "orbit radius must never collapse or diverge during 360-degree look + jump"
        );
    }
}

#[test]
fn test_audit_f5_spam_during_knockback() {
    let mut harness = Harness::new();
    let mut tick = Instant::now();
    let mut eye = glam::DVec3::new(0.0, 64.0, 0.0);
    let mut velocity = glam::DVec3::new(1.2, 0.35, 1.2);

    let perspectives = [
        Perspective::FirstPerson,
        Perspective::ThirdPersonBack,
        Perspective::ThirdPersonFront,
    ];

    let f0 = frame(eye.as_vec3(), tick, 0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f0))
        .expect("frame");
    harness.sync();

    for i in 1..=6 {
        let prev_eye = eye;
        eye += velocity;
        velocity *= 0.6; // knockback decay
        tick += TICK;
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

        let sample = harness
            .world_view
            .view
            .sample(tick + TICK / 2)
            .expect("sample");
        let expected_eye = prev_eye.lerp(eye, 0.5);
        assert!(
            (sample.eye - expected_eye).length() < 1e-4,
            "knockback impulse must interpolate smoothly across F5 toggles"
        );
        let expected_dist = if p == Perspective::FirstPerson {
            0.0
        } else {
            4.0
        };
        assert!(
            ((sample.position - sample.eye).length() - expected_dist).abs() < 1e-4,
            "orbit distance must remain strictly anchored to knocked back eye"
        );
    }
}

#[test]
fn test_audit_f5_spam_during_server_setback() {
    let mut harness = Harness::new();
    let mut tick = Instant::now();
    let mut eye = glam::DVec3::new(0.0, 64.0, 0.0);

    let f0 = frame(eye.as_vec3(), tick, 0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f0))
        .expect("frame");
    harness.sync();

    // 1. Move in ThirdPersonBack
    eye += glam::DVec3::new(1.0, 0.0, 0.0);
    tick += TICK;
    let mut f1 = frame(eye.as_vec3(), tick, 1);
    f1.perspective = Perspective::ThirdPersonBack;
    f1.state.eye = eye;
    f1.state.position = eye + glam::DVec3::new(0.0, 0.0, -4.0);
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f1))
        .expect("frame");
    harness.sync();

    // 2. Server setback / rubberband teleport occurs
    let setback_pos = glam::DVec3::new(0.0, 64.0, 0.0);
    harness.world_view.view.teleport(setback_pos, 0.0, 0.0);

    // Immediate sample after teleport must be snapped at setback_pos with NO glide
    let s_snap = harness.world_view.view.sample(tick).expect("sample");
    assert_eq!(
        s_snap.eye, setback_pos,
        "eye must snap to setback position instantly"
    );
    assert_eq!(
        s_snap.position,
        setback_pos + glam::DVec3::new(0.0, 0.0, -4.0),
        "orbit must snap to setback position instantly"
    );

    // 3. Next tick with another F5 toggle to ThirdPersonFront
    let next_eye = setback_pos + glam::DVec3::new(0.1, 0.0, 0.0);
    tick += TICK;
    let mut f2 = frame(next_eye.as_vec3(), tick, 2);
    f2.perspective = Perspective::ThirdPersonFront;
    f2.state.eye = next_eye;
    f2.state.position = next_eye + glam::DVec3::new(0.0, 0.0, 4.0);
    f2.state.yaw = std::f32::consts::PI;
    harness
        .frames
        .0
        .send(RenderEvent::Frame(f2))
        .expect("frame");
    harness.sync();

    let s_after = harness
        .world_view
        .view
        .sample(tick + TICK / 2)
        .expect("sample");
    let expected_eye = setback_pos.lerp(next_eye, 0.5);
    assert!(
        (s_after.eye - expected_eye).length() < 1e-4,
        "interpolation must cleanly resume from setback origin"
    );
    assert!(
        ((s_after.position - s_after.eye).length() - 4.0).abs() < 1e-4,
        "front view orbit must be exact after setback"
    );
}

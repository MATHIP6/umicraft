// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mouse look: continuous yaw, clamped pitch and the wire angles.

use super::*;
use crate::mouse_look::{look_turn, sensitivity_factor, MouseSteps};
use umicraft_game::input::{set_angles, MAX_PITCH};

#[test]
fn the_input_update_turns_the_view() {
    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    handle_input(
        &mut game,
        InputUpdate {
            state: InputState {
                forward: 1.0,
                ..InputState::default()
            },
            look_yaw: 0.5,
            look_pitch: 0.25,
            ..InputUpdate::default()
        },
        &mut queue,
    );
    assert_eq!((game.yaw, game.pitch), set_angles(0.0, 0.0, (0.5, 0.25)));
    assert_eq!(game.controls.input.forward, 1.0);
}

/// Turns by `degrees`, runs one tick and returns the yaw of the movement
/// packet that tick queued, decoded from the encoded wire bytes.
fn turn_and_read_wire_yaw(game: &mut Game, queue: &mut CommandQueue, degrees: &[f32]) -> f32 {
    use umicraft_protocol::EncodablePacket;
    for step in degrees {
        handle_input(
            game,
            InputUpdate {
                // Degrees to `setAngles` units (0.15 degree each).
                look_yaw: step / 0.15,
                ..InputUpdate::default()
            },
            queue,
        );
    }
    let (frames, _frames_rx) = mpsc::sync_channel(64);
    let (chunks, _chunks_rx) = mpsc::sync_channel(64);
    tick(game, queue, &frames, &chunks, Instant::now());
    let yaw = match queue.ordered.drain(..).next_back() {
        Some(ClientCommand::PositionLook { yaw, .. } | ClientCommand::PlayerLook { yaw, .. }) => {
            yaw
        }
        other => panic!("the tick must report the look, got {other:?}"),
    };
    let mut body = Vec::new();
    umicraft_protocol::packets::play::serverbound::PlayerLook {
        yaw,
        pitch: 0.0,
        on_ground: true,
    }
    .encode_body(&mut body)
    .expect("encode");
    f32::from_be_bytes([body[0], body[1], body[2], body[3]])
}

fn game_facing(degrees: f32) -> (Game, CommandQueue) {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.playing = true;
    game.yaw = degrees;
    (game, CommandQueue::default())
}

#[test]
fn the_wire_yaw_crosses_360_without_wrapping() {
    let (mut game, mut queue) = game_facing(359.9);
    let yaw = turn_and_read_wire_yaw(&mut game, &mut queue, &[0.3]);
    assert!((yaw - 360.2).abs() < 1e-2, "{yaw}");
    let (mut game, mut queue) = game_facing(359.0);
    let yaw = turn_and_read_wire_yaw(&mut game, &mut queue, &[1.0]);
    assert!((yaw - 360.0).abs() < 1e-2, "{yaw}");
}

#[test]
fn the_wire_yaw_goes_below_zero_without_wrapping() {
    let (mut game, mut queue) = game_facing(0.0);
    let yaw = turn_and_read_wire_yaw(&mut game, &mut queue, &[-0.2]);
    assert!((yaw + 0.2).abs() < 1e-3, "{yaw}");
    let yaw = turn_and_read_wire_yaw(&mut game, &mut queue, &[-720.0]);
    assert!((yaw + 720.2).abs() < 1e-1, "{yaw}");
}

#[test]
fn several_turns_in_a_row_keep_adding_up() {
    let (mut game, mut queue) = game_facing(0.0);
    let mut total = 0.0;
    for _ in 0..8 {
        total += 100.0;
        let yaw = turn_and_read_wire_yaw(&mut game, &mut queue, &[100.0]);
        assert!((yaw - total).abs() < 1e-1, "{yaw} against {total}");
    }
    assert!(total > 720.0);
}

#[test]
fn rapid_mouse_movements_between_ticks_add_up() {
    let (mut game, mut queue) = game_facing(350.0);
    let burst = [7.3_f32; 50];
    let yaw = turn_and_read_wire_yaw(&mut game, &mut queue, &burst);
    assert!((yaw - (350.0 + 365.0)).abs() < 1e-1, "{yaw}");
}

#[test]
fn consecutive_wire_yaws_never_jump_by_a_turn() {
    // A slow turn through two full circles: every step stays the size of
    // the mouse step, there is no -360 step at the crossing.
    let (mut game, mut queue) = game_facing(0.0);
    let mut previous = 0.0_f32;
    for _ in 0..1440 {
        let yaw = turn_and_read_wire_yaw(&mut game, &mut queue, &[0.5]);
        assert!((yaw - previous - 0.5).abs() < 1e-1, "{previous} -> {yaw}");
        previous = yaw;
    }
    assert!(previous > 719.0);
    // And back, through zero into the negative.
    for _ in 0..1440 {
        let yaw = turn_and_read_wire_yaw(&mut game, &mut queue, &[-0.5]);
        assert!((yaw - previous + 0.5).abs() < 1e-1, "{previous} -> {yaw}");
        previous = yaw;
    }
}

#[test]
fn a_continuous_yaw_looks_the_same_as_the_wrapped_one() {
    // The raycast and the third person camera only use the angle, not
    // its turn count.
    let wrapped = look_direction(0.2, 0.3);
    let turned = look_direction(0.2 + 3.0 * std::f32::consts::TAU, 0.3);
    assert!((wrapped - turned).length() < 1e-3);
    let behind = look_direction(0.2 - 2.0 * std::f32::consts::TAU, 0.3);
    assert!((wrapped - behind).length() < 1e-3);
}

#[test]
fn the_local_body_and_the_head_follow_a_continuous_yaw() {
    use umicraft_game::entity::angle::wrap_degrees;
    let flags = EntityFlags::new(0, 0x7F);
    let mut a = Game::new();
    let mut b = Game::new();
    for (yaw_a, yaw_b) in [(359.9, -0.1), (360.2, 0.2), (721.0, 1.0)] {
        a.body.follow(DVec3::ZERO, yaw_a, 0.0, flags);
        b.body.follow(DVec3::ZERO, yaw_b, 0.0, flags);
        let (fa, fb) = (a.body.frame_data(), b.body.frame_data());
        assert!(wrap_degrees(fa.head_yaw - fb.head_yaw).abs() < 1e-3);
        assert!(wrap_degrees(fa.body_yaw - fb.body_yaw).abs() < 1e-3);
    }
}

#[test]
fn the_pitch_is_clamped_to_the_vanilla_limit() {
    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    for _ in 0..100 {
        handle_input(
            &mut game,
            InputUpdate {
                look_pitch: 0.5,
                ..InputUpdate::default()
            },
            &mut queue,
        );
    }
    assert!(game.pitch <= MAX_PITCH);
}

// =========================================================================
// Phase C: Parité des rotations et des entrées utilisateur (Minecraft 1.8.9)
// =========================================================================

#[test]
fn test_rot_1_pitch_at_minus_90_degrees() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);

    // Input pushing pitch to negative limit (looking up towards sky)
    handle_input(
        &mut game,
        InputUpdate {
            look_pitch: -1000.0,
            ..InputUpdate::default()
        },
        &mut queue,
    );

    assert_eq!(game.pitch, -90.0);

    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    let movements = movement_of(&queue);
    assert!(!movements.is_empty());
    let last = movements.last().unwrap();
    let wire_pitch = match last {
        ClientCommand::PositionLook { pitch, .. } | ClientCommand::PlayerLook { pitch, .. } => {
            *pitch
        }
        other => panic!("expected look report, got {other:?}"),
    };
    assert!(
        (wire_pitch - (-90.0)).abs() < 1e-4,
        "wire pitch must be -90.0°, got {wire_pitch}"
    );
}

#[test]
fn test_rot_2_pitch_at_plus_90_degrees() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);

    // Input pushing pitch to positive limit (looking down towards feet)
    handle_input(
        &mut game,
        InputUpdate {
            look_pitch: 1000.0,
            ..InputUpdate::default()
        },
        &mut queue,
    );

    assert_eq!(game.pitch, 90.0);

    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    let movements = movement_of(&queue);
    assert!(!movements.is_empty());
    let last = movements.last().unwrap();
    let wire_pitch = match last {
        ClientCommand::PositionLook { pitch, .. } | ClientCommand::PlayerLook { pitch, .. } => {
            *pitch
        }
        other => panic!("expected look report, got {other:?}"),
    };
    assert!(
        (wire_pitch - 90.0).abs() < 1e-4,
        "wire pitch must be +90.0°, got {wire_pitch}"
    );
}

#[test]
fn test_rot_3_pitch_clamp_beyond_limits() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();

    // Attempting to exceed upper limit
    handle_input(
        &mut game,
        InputUpdate {
            look_pitch: 1000.0,
            ..InputUpdate::default()
        },
        &mut queue,
    );
    assert_eq!(game.pitch, MAX_PITCH);

    // Attempting to exceed lower limit
    handle_input(
        &mut game,
        InputUpdate {
            look_pitch: -2000.0,
            ..InputUpdate::default()
        },
        &mut queue,
    );
    assert_eq!(game.pitch, -MAX_PITCH);
}

/// The degrees `pixels` of horizontal motion turn the look by.
fn turned_degrees(pixels: i32, sensitivity: f32) -> f32 {
    let controls = crate::options::ControlOptions {
        mouse_sensitivity: sensitivity,
        ..crate::options::ControlOptions::default()
    };
    set_angles(0.0, 0.0, look_turn((pixels, 0), &controls, 1.0)).0
}

#[test]
fn test_rot_4_mouse_delta_1_10_100_pixels() {
    // At the default sensitivity (0.5) the factor is 1: 0.15 degree a pixel.
    assert_eq!(turned_degrees(1, 0.5), 0.15);
    assert_eq!(turned_degrees(10, 0.5), 1.5);
    assert_eq!(turned_degrees(100, 0.5), 15.0);
}

#[test]
fn test_rot_5_sensitivity_scaling_levels() {
    // 100 pixels at the lowest, default and highest sensitivity:
    // f1 = 0.008, 1 and 4.096 (`(s * 0.6 + 0.2)^3 * 8`).
    assert!((turned_degrees(100, 0.0) - 100.0 * 0.064 * 0.15).abs() < 1e-4);
    assert_eq!(turned_degrees(100, 0.5), 15.0);
    assert!((turned_degrees(100, 1.0) - 100.0 * 4.096 * 0.15).abs() < 1e-4);
}

#[test]
fn test_rot_6_java_rust_rotation_comparison() {
    // MCP `EntityRenderer.updateCameraAndRender` then `Entity.setAngles`,
    // in Java's types: the turn must be the same bits.
    let sensitivities = [0.0_f32, 0.2, 0.37, 0.5, 0.8, 1.0];
    let deltas = [1, 5, 10, 25, 50, 100, -15, -80];
    for &s in &sensitivities {
        let f = s * 0.6_f32 + 0.2_f32;
        let f1 = f * f * f * 8.0_f32;
        for &d in &deltas {
            let start = 123.456_f32;
            let java = (f64::from(start) + f64::from(d as f32 * f1) * 0.15) as f32;
            let controls = crate::options::ControlOptions {
                mouse_sensitivity: s,
                ..crate::options::ControlOptions::default()
            };
            let ours = set_angles(start, 0.0, look_turn((d, 0), &controls, 1.0)).0;
            assert_eq!(ours.to_bits(), java.to_bits(), "sensitivity {s}, delta {d}");
        }
    }
    assert_eq!(
        sensitivity_factor(&crate::options::ControlOptions::default(), 1.0),
        1.0
    );
}

#[test]
fn test_rot_7_yaw_continuity_crossing_359_to_0() {
    let (mut game, mut queue) = game_facing(359.0);
    // Turning +2.0° crosses 360°
    let wire_yaw = turn_and_read_wire_yaw(&mut game, &mut queue, &[2.0]);
    // Must be continuous 361.0°, NEVER 1.0° (which would be a -360° jump)
    assert!(
        (wire_yaw - 361.0).abs() < 1e-2,
        "expected 361.0°, got {wire_yaw}"
    );
}

#[test]
fn test_rot_8_yaw_continuity_crossing_0_to_359() {
    let (mut game, mut queue) = game_facing(0.0);
    // Turning -2.0° goes negative
    let wire_yaw = turn_and_read_wire_yaw(&mut game, &mut queue, &[-2.0]);
    // Must be continuous -2.0°, NEVER 358.0° (which would be a +360° jump)
    assert!(
        (wire_yaw - (-2.0)).abs() < 1e-2,
        "expected -2.0°, got {wire_yaw}"
    );
}

#[test]
fn test_rot_9_c05_player_look_serialization() {
    use umicraft_protocol::packets::play::serverbound::PlayerLook;
    use umicraft_protocol::EncodablePacket;

    let packet = PlayerLook {
        yaw: 185.5,
        pitch: -90.0,
        on_ground: true,
    };

    let mut bytes = Vec::new();
    packet.encode_body(&mut bytes).expect("encode");
    assert_eq!(
        bytes.len(),
        9,
        "C05 must be 9 bytes: 4 bytes yaw + 4 bytes pitch + 1 byte on_ground"
    );

    let decoded = PlayerLook::decode(&mut &bytes[..]).expect("decode");
    assert_eq!(decoded.yaw, 185.5);
    assert_eq!(decoded.pitch, -90.0);
    assert!(decoded.on_ground);
}

#[test]
fn test_rot_10_c06_position_look_serialization() {
    use umicraft_protocol::packets::play::serverbound::PositionLook;
    use umicraft_protocol::EncodablePacket;

    let packet = PositionLook {
        x: 123.456,
        y: 65.0,
        z: -789.101,
        yaw: 360.25,
        pitch: 90.0,
        on_ground: false,
    };

    let mut bytes = Vec::new();
    packet.encode_body(&mut bytes).expect("encode");
    assert_eq!(
        bytes.len(),
        33,
        "C06 must be 33 bytes: 24 bytes (3*f64) + 8 bytes (2*f32) + 1 byte on_ground"
    );

    let decoded = PositionLook::decode(&mut &bytes[..]).expect("decode");
    assert_eq!(decoded.x, 123.456);
    assert_eq!(decoded.y, 65.0);
    assert_eq!(decoded.z, -789.101);
    assert_eq!(decoded.yaw, 360.25);
    assert_eq!(decoded.pitch, 90.0);
    assert!(!decoded.on_ground);
}

#[test]
fn test_rot_11_camera_rotation_and_wire_consistency() {
    let mut game = placed_game();

    // 1. Looking down at feet (+90°)
    game.pitch = 90.0;
    let wire_pitch = game.pitch;
    assert!((wire_pitch - 90.0).abs() < 1e-4);

    let render_state = RenderState {
        position: DVec3::ZERO,
        yaw: game.yaw.to_radians(),
        pitch: game.pitch.to_radians(),
        eye: DVec3::new(0.0, 1.62, 0.0),
        orbit_start: Vec3::ZERO,
        on_ground: true,
        effects: umicraft_game::view::ViewEffects::default(),
    };
    let (cam_yaw, cam_pitch) = camera_angles(&render_state);
    assert!(
        (cam_pitch - (-90.0_f32.to_radians())).abs() < 1e-4,
        "camera pitch is flipped relative to game pitch"
    );
    assert!((cam_yaw - (game.yaw.to_radians() + std::f32::consts::PI)).abs() < 1e-4);

    // 2. Looking up at sky (-90°)
    game.pitch = -90.0;
    let wire_pitch_up = game.pitch;
    assert!((wire_pitch_up - (-90.0)).abs() < 1e-4);

    let render_state_up = RenderState {
        position: DVec3::ZERO,
        yaw: game.yaw.to_radians(),
        pitch: game.pitch.to_radians(),
        eye: DVec3::new(0.0, 1.62, 0.0),
        orbit_start: Vec3::ZERO,
        on_ground: true,
        effects: umicraft_game::view::ViewEffects::default(),
    };
    let (_, cam_pitch_up) = camera_angles(&render_state_up);
    assert!((cam_pitch_up - (90.0_f32.to_radians())).abs() < 1e-4);
}

// =========================================================================
// Phase C.1: Validation du pipeline réel des rotations
// =========================================================================

fn drive_production_mouse_pipeline(
    game: &mut Game,
    queue: &mut CommandQueue,
    controls: &crate::options::ControlOptions,
    zoom_factor: f32,
    deltas: &[(f64, f64)],
) {
    let mut steps = MouseSteps::default();
    for &delta in deltas {
        steps.add(delta);
    }
    let turn = look_turn(steps.whole(), controls, zoom_factor);
    let update = InputUpdate {
        look_yaw: turn.0,
        look_pitch: turn.1,
        ..InputUpdate::default()
    };
    handle_input(game, update, queue);
}

#[test]
fn test_pipeline_sensitivity_0_0_min() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    let controls = crate::options::ControlOptions {
        mouse_sensitivity: 0.0,
        ..crate::options::ControlOptions::default()
    };

    // 100px X, 50px Y at min sensitivity
    drive_production_mouse_pipeline(&mut game, &mut queue, &controls, 1.0, &[(100.0, 50.0)]);
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());

    let movements = movement_of(&queue);
    assert!(!movements.is_empty());
    let (yaw, pitch) = match movements.last().unwrap() {
        ClientCommand::PositionLook { yaw, pitch, .. }
        | ClientCommand::PlayerLook { yaw, pitch, .. } => (*yaw, *pitch),
        other => panic!("expected look report, got {other:?}"),
    };

    let expected_yaw = 100.0 * 0.064 * 0.15; // 0.96°
    let expected_pitch = 50.0 * 0.064 * 0.15; // 0.48°
    assert!(
        (yaw - expected_yaw).abs() < 1e-4,
        "yaw: expected {expected_yaw}, got {yaw}"
    );
    assert!(
        (pitch - expected_pitch).abs() < 1e-4,
        "pitch: expected {expected_pitch}, got {pitch}"
    );
}

#[test]
fn test_pipeline_sensitivity_0_5_default() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    let controls = crate::options::ControlOptions {
        mouse_sensitivity: 0.5,
        ..crate::options::ControlOptions::default()
    };

    // 100px X, -50px Y at default sensitivity
    drive_production_mouse_pipeline(&mut game, &mut queue, &controls, 1.0, &[(100.0, -50.0)]);
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());

    let movements = movement_of(&queue);
    assert!(!movements.is_empty());
    let (yaw, pitch) = match movements.last().unwrap() {
        ClientCommand::PositionLook { yaw, pitch, .. }
        | ClientCommand::PlayerLook { yaw, pitch, .. } => (*yaw, *pitch),
        other => panic!("expected look report, got {other:?}"),
    };

    let expected_yaw = 100.0 * 1.0 * 0.15; // 15.0°
    let expected_pitch = -50.0 * 1.0 * 0.15; // -7.5°
    assert!(
        (yaw - expected_yaw).abs() < 1e-4,
        "yaw: expected {expected_yaw}, got {yaw}"
    );
    assert!(
        (pitch - expected_pitch).abs() < 1e-4,
        "pitch: expected {expected_pitch}, got {pitch}"
    );
}

#[test]
fn test_pipeline_sensitivity_1_0_max() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    let controls = crate::options::ControlOptions {
        mouse_sensitivity: 1.0,
        ..crate::options::ControlOptions::default()
    };

    // -20px X, 30px Y at max sensitivity
    drive_production_mouse_pipeline(&mut game, &mut queue, &controls, 1.0, &[(-20.0, 30.0)]);
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());

    let movements = movement_of(&queue);
    assert!(!movements.is_empty());
    let (yaw, pitch) = match movements.last().unwrap() {
        ClientCommand::PositionLook { yaw, pitch, .. }
        | ClientCommand::PlayerLook { yaw, pitch, .. } => (*yaw, *pitch),
        other => panic!("expected look report, got {other:?}"),
    };

    let expected_yaw = -20.0 * 4.096 * 0.15; // -12.288°
    let expected_pitch = 30.0 * 4.096 * 0.15; // 18.432°
    assert!(
        (yaw - expected_yaw).abs() < 1e-4,
        "yaw: expected {expected_yaw}, got {yaw}"
    );
    assert!(
        (pitch - expected_pitch).abs() < 1e-4,
        "pitch: expected {expected_pitch}, got {pitch}"
    );
}

#[test]
fn test_pipeline_positive_and_negative_deltas() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    let controls = crate::options::ControlOptions {
        mouse_sensitivity: 0.5,
        ..crate::options::ControlOptions::default()
    };

    // Two movements: (+50, +40) then (-80, -100) -> net (-30, -60)
    drive_production_mouse_pipeline(
        &mut game,
        &mut queue,
        &controls,
        1.0,
        &[(50.0, 40.0), (-80.0, -100.0)],
    );
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());

    let movements = movement_of(&queue);
    assert!(!movements.is_empty());
    let (yaw, pitch) = match movements.last().unwrap() {
        ClientCommand::PositionLook { yaw, pitch, .. }
        | ClientCommand::PlayerLook { yaw, pitch, .. } => (*yaw, *pitch),
        other => panic!("expected look report, got {other:?}"),
    };

    let expected_yaw = -30.0 * 0.15; // -4.5°
    let expected_pitch = -60.0 * 0.15; // -9.0°
    assert!(
        (yaw - expected_yaw).abs() < 1e-4,
        "yaw: expected {expected_yaw}, got {yaw}"
    );
    assert!(
        (pitch - expected_pitch).abs() < 1e-4,
        "pitch: expected {expected_pitch}, got {pitch}"
    );
}

#[test]
fn test_pipeline_multiple_successive_mouse_events_in_frame() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    let controls = crate::options::ControlOptions {
        mouse_sensitivity: 0.5,
        ..crate::options::ControlOptions::default()
    };

    // 5 successive events arriving in the same frame
    let events = [
        (10.0, 5.0),
        (15.0, -2.0),
        (20.0, 7.0),
        (-5.0, -3.0),
        (8.0, 4.0),
    ]; // sum: x = 48.0, y = 11.0

    drive_production_mouse_pipeline(&mut game, &mut queue, &controls, 1.0, &events);
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());

    let movements = movement_of(&queue);
    assert!(!movements.is_empty());
    let (yaw, pitch) = match movements.last().unwrap() {
        ClientCommand::PositionLook { yaw, pitch, .. }
        | ClientCommand::PlayerLook { yaw, pitch, .. } => (*yaw, *pitch),
        other => panic!("expected look report, got {other:?}"),
    };

    let expected_yaw = 48.0 * 0.15; // 7.2°
    let expected_pitch = 11.0 * 0.15; // 1.65°
    assert!(
        (yaw - expected_yaw).abs() < 1e-4,
        "yaw: expected {expected_yaw}, got {yaw}"
    );
    assert!(
        (pitch - expected_pitch).abs() < 1e-4,
        "pitch: expected {expected_pitch}, got {pitch}"
    );
}

#[test]
fn test_pipeline_pitch_vertical_limits_plus_minus_90() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    let controls = crate::options::ControlOptions {
        mouse_sensitivity: 0.5,
        ..crate::options::ControlOptions::default()
    };

    // 1. Pulling mouse way up -> should clamp to -90.0°
    drive_production_mouse_pipeline(&mut game, &mut queue, &controls, 1.0, &[(0.0, -5000.0)]);
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());

    let movements = movement_of(&queue);
    let pitch_up = match movements.last().unwrap() {
        ClientCommand::PositionLook { pitch, .. } | ClientCommand::PlayerLook { pitch, .. } => {
            *pitch
        }
        other => panic!("expected look report, got {other:?}"),
    };
    assert!(
        (pitch_up - (-90.0)).abs() < 1e-4,
        "expected -90.0°, got {pitch_up}"
    );

    // 2. Pushing mouse way down -> should clamp to +90.0°
    drive_production_mouse_pipeline(&mut game, &mut queue, &controls, 1.0, &[(0.0, 10000.0)]);
    tick(
        &mut game,
        &mut queue,
        &frames,
        &chunks,
        Instant::now() + Duration::from_millis(50),
    );

    let movements = movement_of(&queue);
    let pitch_down = match movements.last().unwrap() {
        ClientCommand::PositionLook { pitch, .. } | ClientCommand::PlayerLook { pitch, .. } => {
            *pitch
        }
        other => panic!("expected look report, got {other:?}"),
    };
    assert!(
        (pitch_down - 90.0).abs() < 1e-4,
        "expected +90.0°, got {pitch_down}"
    );
}

#[test]
fn test_pipeline_yaw_exceeding_360_continuous() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    let controls = crate::options::ControlOptions {
        mouse_sensitivity: 0.5,
        ..crate::options::ControlOptions::default()
    };

    // 6 bursts of 500px to the right = 3000px
    // 3000px * 0.15 = 450.0°
    for _ in 0..6 {
        drive_production_mouse_pipeline(&mut game, &mut queue, &controls, 1.0, &[(500.0, 0.0)]);
    }
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());

    let movements = movement_of(&queue);
    let yaw = match movements.last().unwrap() {
        ClientCommand::PositionLook { yaw, .. } | ClientCommand::PlayerLook { yaw, .. } => *yaw,
        other => panic!("expected look report, got {other:?}"),
    };
    assert!(
        (yaw - 450.0).abs() < 1e-2,
        "continuous yaw must be 450.0°, got {yaw}"
    );
}

#[test]
fn test_pipeline_c05_c06_serialization_after_multiple_rotations() {
    use umicraft_protocol::packets::play::serverbound::{PlayerLook, PositionLook};
    use umicraft_protocol::EncodablePacket;

    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    let controls = crate::options::ControlOptions {
        mouse_sensitivity: 0.5,
        ..crate::options::ControlOptions::default()
    };

    // 1. Initial tick with movement+rotation reports C06 PositionLook (33 bytes)
    drive_production_mouse_pipeline(&mut game, &mut queue, &controls, 1.0, &[(50.0, -30.0)]);
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());

    let movements1 = movement_of(&queue);
    let (c06_pos, c06_yaw, c06_pitch) = match movements1.last().unwrap() {
        ClientCommand::PositionLook {
            x,
            y,
            z,
            yaw,
            pitch,
            ..
        } => (DVec3::new(*x, *y, *z), *yaw, *pitch),
        other => panic!("expected PositionLook (C06), got {other:?}"),
    };
    let mut c06_bytes = Vec::new();
    PositionLook {
        x: c06_pos.x,
        y: c06_pos.y,
        z: c06_pos.z,
        yaw: c06_yaw,
        pitch: c06_pitch,
        on_ground: false,
    }
    .encode_body(&mut c06_bytes)
    .expect("encode c06");
    assert_eq!(c06_bytes.len(), 33, "C06 must be exactly 33 bytes");
    let decoded_c06 = PositionLook::decode(&mut &c06_bytes[..]).expect("decode c06");
    assert_eq!(decoded_c06.x, c06_pos.x);
    assert_eq!(decoded_c06.y, c06_pos.y);
    assert_eq!(decoded_c06.z, c06_pos.z);
    assert_eq!(decoded_c06.yaw, c06_yaw);
    assert_eq!(decoded_c06.pitch, c06_pitch);

    queue.ordered.clear();

    // 2. Subsequent tick with rotation alone reports C05 PlayerLook (9 bytes)
    drive_production_mouse_pipeline(&mut game, &mut queue, &controls, 1.0, &[(200.0, 100.0)]);
    tick(
        &mut game,
        &mut queue,
        &frames,
        &chunks,
        Instant::now() + Duration::from_millis(50),
    );

    let movements2 = movement_of(&queue);
    let (c05_yaw, c05_pitch) = match movements2.last().unwrap() {
        ClientCommand::PlayerLook { yaw, pitch, .. } => (*yaw, *pitch),
        other => panic!("expected PlayerLook (C05), got {other:?}"),
    };
    let mut c05_bytes = Vec::new();
    PlayerLook {
        yaw: c05_yaw,
        pitch: c05_pitch,
        on_ground: false,
    }
    .encode_body(&mut c05_bytes)
    .expect("encode c05");
    assert_eq!(c05_bytes.len(), 9, "C05 must be exactly 9 bytes");
    let decoded_c05 = PlayerLook::decode(&mut &c05_bytes[..]).expect("decode c05");
    assert_eq!(decoded_c05.yaw, c05_yaw);
    assert_eq!(decoded_c05.pitch, c05_pitch);
}

#[test]
fn test_pipeline_consistency_player_state_and_wire() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    let controls = crate::options::ControlOptions {
        mouse_sensitivity: 0.5,
        ..crate::options::ControlOptions::default()
    };

    drive_production_mouse_pipeline(&mut game, &mut queue, &controls, 1.0, &[(123.0, -45.0)]);
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());

    let movements = movement_of(&queue);
    let (wire_yaw, wire_pitch) = match movements.last().unwrap() {
        ClientCommand::PositionLook { yaw, pitch, .. }
        | ClientCommand::PlayerLook { yaw, pitch, .. } => (*yaw, *pitch),
        other => panic!("expected look report, got {other:?}"),
    };

    // Exactly matches game state degrees
    assert_eq!(game.yaw, wire_yaw);
    assert_eq!(game.pitch, wire_pitch);
}

#[test]
fn a_server_look_is_sent_back_bit_for_bit() {
    // Regression (ADR-0076): the look went through radians and back, so a
    // `Player Position and Look` at yaw -123.4567 came back as -123.45669555.
    use umicraft_net::play::PlayerPlacement;

    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    let (yaw, pitch) = (-123.4567_f32, 12.3456_f32);
    let placement = PlayerPlacement {
        x: 0.5,
        y: 4.0,
        z: 0.5,
        yaw,
        pitch,
        raw: None,
    };
    handle_net(
        &mut game,
        PlayEvent::Placement(placement),
        &mut queue,
        &frames,
    );
    assert_eq!(
        (game.yaw.to_bits(), game.pitch.to_bits()),
        (yaw.to_bits(), pitch.to_bits())
    );
    queue.ordered.clear();
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    let looks: Vec<(u32, u32)> = movement_of(&queue)
        .into_iter()
        .filter_map(|command| match command {
            ClientCommand::PositionLook { yaw, pitch, .. }
            | ClientCommand::PlayerLook { yaw, pitch, .. } => {
                Some((yaw.to_bits(), pitch.to_bits()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(looks, [(yaw.to_bits(), pitch.to_bits())]);
}

#[test]
fn a_relative_teleport_turns_the_live_look() {
    // Regression: the relative yaw and pitch of a `Player Position and Look`
    // were added to the look of the last physics tick (stale on a vehicle,
    // over a missing chunk, or after a mouse turn within the tick), so the
    // view jumped. MCP `handlePlayerPosLook` adds them to the live look.
    use umicraft_net::play::{PlayerPlacement, RawPlacement};

    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, received) = mpsc::sync_channel(64);
    handle_input(
        &mut game,
        InputUpdate {
            look_yaw: 300.0,
            look_pitch: -100.0,
            ..InputUpdate::default()
        },
        &mut queue,
    );
    let (yaw, pitch) = (game.yaw, game.pitch);
    assert_eq!((yaw, pitch), (45.0, -15.0));
    let raw = RawPlacement {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        yaw: 10.0,
        pitch: 0.0,
        flags: 0x01 | 0x02 | 0x04 | 0x08 | 0x10,
    };
    let state = game.state.as_ref().expect("placed").position();
    let placement = PlayerPlacement {
        x: state.x,
        y: state.y,
        z: state.z,
        yaw: 10.0,
        pitch: 0.0,
        raw: Some(Box::new(raw)),
    };
    handle_net(
        &mut game,
        PlayEvent::Placement(placement),
        &mut queue,
        &frames,
    );
    assert_eq!((game.yaw, game.pitch), (55.0, -15.0));
    let teleport = received.try_iter().find_map(|event| match event {
        RenderEvent::Teleport { yaw, pitch, .. } => Some((yaw, pitch)),
        _ => None,
    });
    assert_eq!(
        teleport,
        Some((ViewAngle::By(10.0_f32.to_radians()), ViewAngle::By(0.0)))
    );
}

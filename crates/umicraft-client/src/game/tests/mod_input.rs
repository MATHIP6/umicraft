// SPDX-License-Identifier: MIT OR Apache-2.0

//! The hand and crosshair facts of the `INPUT` mod event (API 1.5).

use super::*;
use crate::game::mod_input::hand_facts;
use umicraft_modhost::{PointedFacts, UseKind};

#[test]
fn the_crosshair_on_the_floor_is_a_block_with_its_wire_state_and_face() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.pitch = 57.3;
    let PointedFacts::Block {
        position,
        face,
        state,
        distance,
        ..
    } = hand_facts(&game).pointed
    else {
        panic!("the floor is in reach");
    };
    assert_eq!(position[1], 63);
    assert_eq!(face, 1, "the top face is 1 on the wire");
    assert_eq!(state, 2 << 4, "grass, metadata 0");
    assert!(distance > 0.0 && distance < 5.0);
}

#[test]
fn an_entity_ahead_is_pointed_by_its_server_id() {
    let mut game = game_on_floor_with(BlockState::AIR);
    spawn_ahead(&mut game, false);
    let PointedFacts::Entity { id, distance, .. } = hand_facts(&game).pointed else {
        panic!("the zombie is under the crosshair");
    };
    assert_eq!(id, 7);
    assert!(distance > 1.0 && distance < 3.0);
}

#[test]
fn looking_up_misses() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.pitch = -90.0;
    assert_eq!(hand_facts(&game).pointed, PointedFacts::Miss);
}

#[test]
fn the_held_item_tells_its_use_and_whether_it_is_a_block() {
    let mut game = game_on_floor_with(BlockState::AIR);
    assert_eq!(hand_facts(&game).held, None);
    game.windows.apply_set_slot(0, 36, wire_item(276, 0));
    let sword = hand_facts(&game);
    assert_eq!(sword.held.map(|held| held.id), Some(276));
    assert_eq!(sword.held_use, UseKind::Block);
    assert!(!sword.held_is_block);
    game.windows.apply_set_slot(0, 36, wire_item(1, 0));
    let stone = hand_facts(&game);
    assert_eq!(stone.held_use, UseKind::None);
    assert!(stone.held_is_block);
}

#[test]
fn a_click_swings_the_arm_in_the_facts() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    game.pitch = 57.3;
    game.controls.attack_presses = 1;
    run_attack(&mut game, &mut queue);
    // MCP `updateArmSwingProgress`: the swing starts at -1, so the first tick
    // after it still reads 0.
    tick_hand(&mut game);
    assert_eq!(hand_facts(&game).swing_progress, 0.0);
    tick_hand(&mut game);
    assert!(hand_facts(&game).swing_progress > 0.0);
}

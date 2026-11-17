// SPDX-License-Identifier: MIT OR Apache-2.0

//! Digging, cracks and the selection outline.

use super::*;

#[test]
fn a_held_survival_click_digs_the_block_over_several_ticks_and_cracks_it() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    game.entity_id = 1;
    game.pitch = 1.5;
    game.input.attack = true;
    game.attack_pressed = true;
    let block = BlockPos::new(0, 63, 0);
    let mut statuses = Vec::new();
    let mut cracked = false;
    // The player never landed in this fixture, so the dig is five times
    // slower (airborne): 0.6 hardness takes about 90 ticks.
    for _ in 0..300 {
        run_attack(&mut game, &mut queue);
        while let Some(command) = queue.ordered.pop_front() {
            if let ClientCommand::PlayerDigging { status, .. } = command {
                statuses.push(status);
            }
        }
        if game.world.as_ref().expect("world").block(block) == BlockState::AIR {
            break;
        }
        // Once the progress passes 10 % the block shows cracks.
        if !game.cracks().is_empty() {
            cracked = true;
        }
    }
    assert!(cracked, "the block never showed a crack while it was dug");
    assert!(game.cracks().is_empty(), "the cracks go with the block");
    assert_eq!(
        statuses.first(),
        Some(&umicraft_protocol::packets::play::serverbound::DiggingStatus::StartDestroyBlock)
    );
    assert_eq!(
        statuses.last(),
        Some(&umicraft_protocol::packets::play::serverbound::DiggingStatus::StopDestroyBlock)
    );
    assert!(game.world.as_ref().expect("world").block(block).is_air());
}

#[test]
fn a_creative_break_removes_the_block_at_once_and_sends_only_the_start() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    game.gamemode = GameMode::Creative;
    game.pitch = 1.5;
    game.attack_pressed = true;
    game.input.attack = true;
    run_attack(&mut game, &mut queue);
    let world = game.world.as_ref().expect("world");
    assert_eq!(world.block(BlockPos::new(0, 63, 0)), BlockState::AIR);
    assert_eq!(digging_commands(&queue), 1);
    assert_eq!(game.interaction.digging_block(), None);
}

#[test]
fn a_spectator_cannot_break_blocks() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    game.gamemode = GameMode::Spectator;
    game.pitch = 1.5;
    game.attack_pressed = true;
    game.input.attack = true;
    run_attack(&mut game, &mut queue);
    assert_eq!(digging_commands(&queue), 0);
}

#[test]
fn the_cracks_follow_every_breaker_and_vanish_with_the_block() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let anim = |breaker: i32, stage: u8, game: &mut Game| {
        game.server.apply_block_break_anim(&BlockBreakAnim {
            entity_id: breaker,
            x: 0,
            y: 63,
            z: 0,
            stage,
        });
    };
    assert!(game.cracks().is_empty());
    anim(7, 4, &mut game);
    let cracks = game.cracks();
    assert_eq!(cracks.len(), 1);
    assert_eq!(cracks[0].stage, 4);
    assert_eq!(cracks[0].bounds.min, [0.0, 63.0, 0.0]);
    assert_eq!(cracks[0].bounds.max, [1.0, 64.0, 1.0]);

    // A second player digging elsewhere is drawn as well.
    game.server.apply_block_break_anim(&BlockBreakAnim {
        entity_id: 8,
        x: 1,
        y: 63,
        z: 0,
        stage: 2,
    });
    assert_eq!(game.cracks().len(), 2);

    anim(7, 255, &mut game);
    assert_eq!(game.cracks().len(), 1, "stage > 9 clears the breaker");
}

#[test]
fn looking_at_the_floor_selects_the_floor_block() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.pitch = std::f32::consts::FRAC_PI_2;
    let boxes = game.selection_outline();
    assert_eq!(
        boxes,
        vec![OutlineBox {
            min: [0.0, 63.0, 0.0],
            max: [1.0, 64.0, 1.0],
        }]
    );
}

#[test]
fn a_selection_is_offset_by_the_block_position_and_follows_its_shape() {
    // A stone slab (id 44) at (0, 64, 0) seen from beside it: it is
    // half a block high, so the outline must not be the full block.
    let mut game = game_on_floor_with(BlockState::new(44, 0));
    game.state = Some(GameState::new(DVec3::new(0.5, 64.0, 3.5)));
    game.pitch = 0.45;
    game.yaw = std::f32::consts::PI;
    let boxes = game.selection_outline();
    assert_eq!(boxes.len(), 1);
    assert_eq!(boxes[0].min, [0.0, 64.0, 0.0]);
    assert_eq!(boxes[0].max, [1.0, 64.5, 1.0]);
}

#[test]
fn looking_at_the_sky_or_being_a_spectator_selects_nothing() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.pitch = -std::f32::consts::FRAC_PI_2;
    assert!(game.selection_outline().is_empty());
    game.pitch = std::f32::consts::FRAC_PI_2;
    game.gamemode = GameMode::Spectator;
    assert!(game.selection_outline().is_empty());
}

#[test]
fn an_adventure_player_gets_no_selection_outline() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.pitch = std::f32::consts::FRAC_PI_2;
    assert!(!game.selection_outline().is_empty());
    game.gamemode = GameMode::Adventure;
    assert!(game.selection_outline().is_empty());
}

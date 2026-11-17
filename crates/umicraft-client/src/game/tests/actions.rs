// SPDX-License-Identifier: MIT OR Apache-2.0

//! Clicks on entities and blocks, and the use of the held item.

use super::*;

#[test]
fn a_click_swings_the_hand_and_a_placement_swings_it_too() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    game.pitch = 1.5;
    game.input.attack = true;
    game.attack_pressed = true;
    run_attack(&mut game, &mut queue);
    tick_hand(&mut game);
    assert!(game.hand.pose().swing > 0.0 || game.hand.swing_progress(1.0) >= 0.0);
    assert!(queue
        .ordered
        .iter()
        .any(|command| matches!(command, ClientCommand::Swing)));

    // Placing with an item swings; with an empty hand it does not.
    let mut empty = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    // Aim ahead of the feet: the block below the player cannot be built on.
    empty.pitch = 1.0;
    click_use(&mut empty, &mut queue);
    assert!(!queue
        .ordered
        .iter()
        .any(|command| matches!(command, ClientCommand::Swing)));
    empty.windows.apply_set_slot(
        0,
        36,
        WireItemStack {
            item_id: 1,
            stack_size: 8,
            damage: 0,
            nbt: None,
        },
    );
    click_use(&mut empty, &mut queue);
    assert!(queue
        .ordered
        .iter()
        .any(|command| matches!(command, ClientCommand::Swing)));
}

fn click_attack(game: &mut Game, queue: &mut CommandQueue) {
    game.input.attack = true;
    game.attack_pressed = true;
    run_attack(game, queue);
}

#[test]
fn a_left_click_on_an_entity_swings_then_attacks_it() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    spawn_ahead(&mut game, false);
    click_attack(&mut game, &mut queue);
    let sent = commands(&queue);
    assert!(matches!(sent[0], ClientCommand::Swing));
    assert!(matches!(
        sent[1],
        ClientCommand::UseEntity {
            target: 7,
            action: UseEntityAction::Attack
        }
    ));
    assert_eq!(sent.len(), 2, "no dig packet while an entity is targeted");
}

#[test]
fn an_entity_behind_a_block_is_not_attacked() {
    let mut game = game_on_floor_with(BlockState::new(1, 0));
    let mut queue = CommandQueue::default();
    spawn_ahead(&mut game, false);
    game.pitch = 0.0;
    // A wall one block ahead of the eyes.
    if let Some(world) = game.world.as_mut() {
        world.set_block(BlockPos::new(0, 65, 1), BlockState::new(1, 0));
    }
    click_attack(&mut game, &mut queue);
    assert!(!commands(&queue)
        .iter()
        .any(|command| matches!(command, ClientCommand::UseEntity { .. })));
}

#[test]
fn a_knockback_hit_on_a_player_slows_the_attacker_but_a_mob_does_not() {
    for (player, factor) in [(true, 0.6), (false, 1.0)] {
        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        spawn_ahead(&mut game, player);
        game.windows.apply_set_slot(
            0,
            36,
            WireItemStack {
                nbt: Some(enchanted_nbt(19, 1)),
                ..wire_item(267, 0)
            },
        );
        if let Some(state) = game.state.as_mut() {
            state.set_velocity(DVec3::new(1.0, 0.0, 1.0));
        }
        click_attack(&mut game, &mut queue);
        let velocity = game.state.as_ref().unwrap().player().velocity;
        assert!((velocity.x - factor).abs() < 1e-9, "player: {player}");
    }
}

#[test]
fn a_right_click_on_a_mob_interacts_before_using_the_item() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    spawn_ahead(&mut game, false);
    game.windows.apply_set_slot(0, 36, wire_item(267, 0));
    click_use(&mut game, &mut queue);
    let sent = commands(&queue);
    assert!(matches!(
        sent[0],
        ClientCommand::UseEntity {
            action: UseEntityAction::InteractAt { .. },
            ..
        }
    ));
    assert!(matches!(
        sent[1],
        ClientCommand::UseEntity {
            action: UseEntityAction::Interact,
            ..
        }
    ));
    assert!(matches!(sent[2], ClientCommand::UseItem { .. }));
}

/// A hungry player who holds bread starts eating on the click, walks at
/// a fifth of the speed while the button is held, and releases it.
#[test]
fn holding_the_use_button_eats_slowly_and_releasing_tells_the_server() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.food = 10.0;
    game.windows.apply_set_slot(0, 36, wire_item(297, 0));
    game.pitch = -0.5;
    let mut queue = CommandQueue::default();
    game.input.use_item = true;
    click_use(&mut game, &mut queue);
    assert!(game.item_use.is_using());
    assert!(matches!(
        commands(&queue)[0],
        ClientCommand::UseItem { item } if item.item_id == 297
    ));

    game.input.use_item = false;
    run_use(&mut game, &mut queue);
    assert!(!game.item_use.is_using());
    assert!(commands(&queue).iter().any(|command| matches!(
        command,
        ClientCommand::PlayerDigging {
            status: DiggingStatus::ReleaseUseItem,
            ..
        }
    )));
}

#[test]
fn the_published_hand_carries_the_item_use_until_it_ends() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.food = 10.0;
    game.windows.apply_set_slot(0, 36, wire_item(297, 0));
    game.pitch = -0.5;
    game.input.use_item = true;
    let mut queue = CommandQueue::default();
    click_use(&mut game, &mut queue);
    let (frames, received) = mpsc::sync_channel(32);
    let frame = |game: &Game| {
        publish_frame(game, &frames, Instant::now());
        match received.try_recv() {
            Ok(RenderEvent::Frame(frame)) => frame,
            _ => panic!("a frame was expected"),
        }
    };
    let using = frame(&game).state.effects.hand.item_use.expect("a use");
    assert_eq!(using.action, UseAction::Eat);
    assert_eq!(using.count, 32.0);
    game.item_use.finish();
    assert_eq!(frame(&game).state.effects.hand.item_use, None);
}

#[test]
fn the_local_player_eating_throws_crumbs_on_the_bite_ticks() {
    // Regression: the local player's own meal showed no crumbs.
    let mut game = game_on_floor_with(BlockState::AIR);
    game.food = 10.0;
    game.windows.apply_set_slot(0, 36, wire_item(297, 0));
    game.input.use_item = true;
    let mut queue = CommandQueue::default();
    click_use(&mut game, &mut queue);
    let mut bites = Vec::new();
    for _ in 0..40 {
        let Some(view) = game.item_use.view() else {
            break;
        };
        let before = game.particles.len();
        spawn_local_crumbs(&mut game);
        if game.particles.len() > before {
            bites.push(view.count as u32);
        }
        run_use(&mut game, &mut queue);
    }
    assert_eq!(bites, [24, 20, 16, 12, 8, 4]);
}

#[test]
fn the_local_player_eating_bites_out_loud() {
    // Regression: the local player's own meal made no sound.
    let mut game = game_on_floor_with(BlockState::AIR);
    game.food = 10.0;
    game.windows.apply_set_slot(0, 36, wire_item(297, 0));
    game.input.use_item = true;
    let mut queue = CommandQueue::default();
    click_use(&mut game, &mut queue);
    let mut bites = 0;
    for _ in 0..40 {
        if game.item_use.view().is_none() {
            break;
        }
        spawn_local_crumbs(&mut game);
        let cues = game.state.as_mut().unwrap().drain_sound_cues();
        bites += cues.iter().filter(|c| c.name == "random.eat").count();
        run_use(&mut game, &mut queue);
    }
    assert_eq!(bites, 6);
}

#[test]
fn the_server_finishing_the_meal_ends_the_use() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.food = 10.0;
    game.entity_id = 7;
    game.windows.apply_set_slot(0, 36, wire_item(297, 0));
    game.pitch = -0.5;
    game.input.use_item = true;
    let mut queue = CommandQueue::default();
    click_use(&mut game, &mut queue);
    let (frames, _received) = mpsc::sync_channel(8);
    let status = PlayEvent::EntityStatus(
        umicraft_protocol::packets::play::clientbound::EntityStatus {
            entity_id: 7,
            status: 9,
        },
    );
    handle_net(&mut game, status, &mut queue, &frames);
    assert!(!game.item_use.is_using());
}

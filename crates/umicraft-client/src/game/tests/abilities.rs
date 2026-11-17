// SPDX-License-Identifier: MIT OR Apache-2.0

//! The game mode, the abilities and the movement stats of the local player.

use super::*;

#[test]
fn the_elder_guardian_curse_haunts_the_player() {
    // Regression: game state 10 did nothing, so no elder guardian showed.
    use umicraft_game::particles::Behaviour;
    use umicraft_protocol::packets::play::clientbound::ChangeGameState;

    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    let change = |reason, value| PlayEvent::ChangeGameState(ChangeGameState { reason, value });
    // Rain is not a curse.
    handle_net(&mut game, change(1, 0.0), &mut queue, &frames);
    assert!(game.particles.is_empty());
    handle_net(&mut game, change(10, 0.0), &mut queue, &frames);
    let apparition = game
        .particles
        .particles(umicraft_game::particles::Layer::Lit, false)
        .iter()
        .chain(
            game.particles
                .particles(umicraft_game::particles::Layer::Lit, true),
        )
        .find(|particle| particle.behaviour == Behaviour::MobAppearance)
        .expect("the apparition");
    assert_eq!(apparition.position, DVec3::new(0.5, 64.0, 0.5));
    let cues = game.state.as_mut().expect("placed").drain_sound_cues();
    assert_eq!(cues.len(), 1);
    assert_eq!(cues[0].name, "mob.guardian.curse");
}

#[test]
fn a_game_state_change_switches_the_gamemode() {
    use umicraft_protocol::packets::play::clientbound::ChangeGameState;

    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    let change = |reason, value| PlayEvent::ChangeGameState(ChangeGameState { reason, value });
    handle_net(&mut game, change(3, 3.0), &mut queue, &frames);
    assert_eq!(game.gamemode, GameMode::Spectator);
    assert_eq!(game.status_data.gamemode, 3);
    let player = game.state.as_ref().expect("placed").player();
    assert!(player.no_clip && player.allow_flying);

    handle_net(&mut game, change(3, 0.0), &mut queue, &frames);
    assert_eq!(game.gamemode, GameMode::Survival);
    assert!(!game.state.as_ref().expect("placed").player().no_clip);

    // Rain is not a gamemode.
    handle_net(&mut game, change(1, 3.0), &mut queue, &frames);
    assert_eq!(game.gamemode, GameMode::Survival);
}

#[test]
fn the_server_abilities_let_a_creative_player_fly() {
    use umicraft_protocol::packets::play::clientbound::PlayerAbilities;

    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    let abilities = PlayEvent::PlayerAbilities(PlayerAbilities {
        flags: 0b1101,
        fly_speed: 0.1,
        walk_speed: 0.1,
    });
    handle_net(&mut game, abilities, &mut queue, &frames);
    let player = game.state.as_ref().expect("placed").player();
    assert!(player.allow_flying);
    assert!(!player.flying);
    assert_eq!(player.fly_speed, 0.1);
}

#[test]
fn a_change_of_the_flying_state_is_reported_to_the_server() {
    use umicraft_protocol::packets::play::clientbound::PlayerAbilities;

    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    game.server.apply_abilities(&PlayerAbilities {
        flags: 0b1100,
        fly_speed: 0.05,
        walk_speed: 0.1,
    });
    report_flight(&game, false, &mut queue);
    assert!(queue.ordered.is_empty(), "nothing changed");

    game.state
        .as_mut()
        .expect("placed")
        .set_abilities(true, true, 0.05);
    report_flight(&game, false, &mut queue);
    assert!(matches!(
        queue.ordered.front(),
        Some(ClientCommand::PlayerAbilities { flying: true, .. })
    ));
}

#[test]
fn the_server_state_drives_the_movement_stats() {
    use umicraft_protocol::packets::play::clientbound::EntityEffect;

    let mut game = game_on_floor_with(BlockState::AIR);
    game.vitals.food = 6.0;
    game.server.on_join(1);
    game.server.apply_entity_effect(&EntityEffect {
        entity_id: 1,
        effect_id: 1,
        amplifier: 1,
        duration: 600,
        hide_particles: 0,
    });
    game.server.apply_entity_effect(&EntityEffect {
        entity_id: 1,
        effect_id: 8,
        amplifier: 0,
        duration: 600,
        hide_particles: 0,
    });
    apply_movement_stats(&mut game);
    let player = game.state.as_ref().expect("placed").player();
    assert!((player.movement_speed - 0.14).abs() < 1e-6);
    assert_eq!(player.jump_boost, 1);
    assert!(!player.well_fed, "6 food points are not enough to sprint");
}

#[test]
fn a_player_who_uses_an_item_walks_at_a_fifth_of_the_speed() {
    let walk = |using: bool| {
        let mut game = game_on_floor_with(BlockState::AIR);
        // The simulation waits for the nine columns around the player.
        let world = game.world.as_mut().expect("a world");
        let payload = vec![0u8; umicraft_world::chunk::payload_len(0b1000, true, true)];
        for (x, z) in [
            (-1, -1),
            (-1, 0),
            (-1, 1),
            (0, -1),
            (0, 1),
            (1, -1),
            (1, 0),
            (1, 1),
        ] {
            world
                .apply_chunk(
                    ChunkPos::new(x, z),
                    umicraft_world::ChunkPayload {
                        primary_bit_mask: 0b1000,
                        ground_up: true,
                        data: &payload,
                    },
                )
                .expect("an empty column");
        }
        game.playing = true;
        game.vitals.food = 10.0;
        game.controls.input.forward = 1.0;
        game.controls.input.use_item = using;
        game.windows.apply_set_slot(0, 36, wire_item(267, 0));
        game.pitch = -0.5;
        game.yaw = 0.0;
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(8);
        let (chunks, _chunk_received) = mpsc::sync_channel(8);
        for _ in 0..10 {
            tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
        }
        let position = game.state.as_ref().expect("placed").position();
        position.z - 0.5
    };
    let free = walk(false);
    let slowed = walk(true);
    assert!(slowed > 0.0, "the player still moves");
    assert!((slowed / free - 0.2).abs() < 0.03, "{slowed} vs {free}");
}

/// Depth Strider on the worn boots reaches the physics.
#[test]
fn the_depth_strider_of_the_boots_reaches_the_player() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut boots = wire_item(313, 0);
    // `ench` list of one compound: `id` = 8 (Depth Strider), `lvl` = 3.
    boots.nbt = Some(enchanted_nbt(8, 3));
    game.windows.apply_set_slot(0, 8, boots);
    apply_movement_stats(&mut game);
    assert_eq!(
        game.state.as_ref().expect("placed").player().depth_strider,
        3
    );
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The entities the server spawns and updates, their name tags and statuses.

use super::*;

fn spawn_object_event(entity_id: i32, object_type: u8, data: i32) -> PlayEvent {
    use umicraft_protocol::packets::play::clientbound::SpawnObject;

    PlayEvent::SpawnObject(SpawnObject {
        entity_id,
        object_type,
        x: 0,
        y: 0,
        z: 0,
        pitch: 0,
        yaw: 0,
        data,
        velocity_x: 0,
        velocity_y: 0,
        velocity_z: 0,
    })
}

fn stack_watcher(
    index: u8,
    stack: &WireItemStack,
) -> umicraft_protocol::packets::play::metadata::WatchedObject {
    let mut value = Vec::new();
    stack.encode(&mut value).expect("encodes");
    umicraft_protocol::packets::play::metadata::WatchedObject {
        type_id: 5,
        index,
        value,
    }
}

fn metadata_event(
    entity_id: i32,
    objects: Vec<umicraft_protocol::packets::play::metadata::WatchedObject>,
) -> PlayEvent {
    use umicraft_protocol::packets::play::clientbound::EntityMetadata;

    PlayEvent::EntityMetadata(EntityMetadata {
        entity_id,
        metadata: Metadata { objects },
    })
}

#[test]
fn the_stack_of_a_dropped_item_comes_from_watcher_ten() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    handle_net(&mut game, spawn_object_event(7, 2, 1), &mut queue, &frames);
    let event = metadata_event(7, vec![stack_watcher(10, &wire_item(264, 0))]);
    handle_net(&mut game, event, &mut queue, &frames);
    let snapshot = &game.entities.snapshots(0.0)[0];
    let item = snapshot.item.as_ref().expect("stack");
    assert_eq!((item.item_id, item.count), (264, 1));
}

#[test]
fn a_bobber_of_the_local_player_casts_their_rod_until_it_is_destroyed() {
    use umicraft_protocol::packets::play::clientbound::DestroyEntities;

    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    let own = game.entity_id;
    // Someone else's bobber does not cast the rod of the local player.
    handle_net(
        &mut game,
        spawn_object_event(40, 90, own + 1),
        &mut queue,
        &frames,
    );
    assert!(!player_view(&game).expect("view").0.effects.hand.fishing);
    handle_net(
        &mut game,
        spawn_object_event(41, 90, own),
        &mut queue,
        &frames,
    );
    assert_eq!(game.body.bobber, Some(EntityId(41)));
    assert!(player_view(&game).expect("view").0.effects.hand.fishing);
    assert!(game.body.frame_data().fishing, "the third person rod too");
    let destroy = PlayEvent::DestroyEntities(DestroyEntities {
        entity_ids: vec![41],
    });
    handle_net(&mut game, destroy, &mut queue, &frames);
    assert_eq!(game.body.bobber, None);
}

#[test]
fn a_collected_item_leaves_the_world() {
    use umicraft_protocol::packets::play::clientbound::CollectItem;

    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    handle_net(&mut game, spawn_object_event(7, 2, 1), &mut queue, &frames);
    assert_eq!(game.entities.snapshots(0.0).len(), 1);
    let event = PlayEvent::CollectItem(CollectItem {
        collected_entity_id: 7,
        collector_entity_id: 1,
    });
    handle_net(&mut game, event, &mut queue, &frames);
    assert_eq!(game.entities.snapshots(0.0).len(), 1, "it flies first");
    for _ in 0..3 {
        game.entities.tick();
    }
    assert!(game.entities.snapshots(0.0).is_empty());
}

fn collect_event(collected: i32) -> PlayEvent {
    PlayEvent::CollectItem(umicraft_protocol::packets::play::clientbound::CollectItem {
        collected_entity_id: collected,
        collector_entity_id: 1,
    })
}

#[test]
fn collecting_an_item_pops_and_an_unknown_entity_is_silent() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    handle_net(&mut game, spawn_object_event(7, 2, 1), &mut queue, &frames);
    handle_net(&mut game, collect_event(99), &mut queue, &frames);
    let state = game.state.as_mut().unwrap();
    assert!(state.drain_sound_cues().is_empty(), "unknown entity");
    handle_net(&mut game, collect_event(7), &mut queue, &frames);
    let cues = game.state.as_mut().unwrap().drain_sound_cues();
    assert_eq!(cues.len(), 1);
    assert_eq!(cues[0].name, "random.pop");
    assert_eq!(cues[0].volume, 0.2);
}

#[test]
fn collecting_an_experience_orb_plays_the_orb_sound() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    game.entities.spawn_xp_orb(EntityId(8), DVec3::ZERO, 1);
    handle_net(&mut game, collect_event(8), &mut queue, &frames);
    let cues = game.state.as_mut().unwrap().drain_sound_cues();
    assert_eq!(cues.len(), 1);
    assert_eq!(cues[0].name, "random.orb");
}

#[test]
fn an_item_frame_takes_its_stack_rotation_and_direction() {
    use umicraft_protocol::packets::play::metadata::WatchedObject;

    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    handle_net(&mut game, spawn_object_event(8, 71, 2), &mut queue, &frames);
    let rotation = WatchedObject {
        type_id: 0,
        index: 9,
        value: vec![0x0B],
    };
    let event = metadata_event(8, vec![stack_watcher(8, &wire_item(276, 4)), rotation]);
    handle_net(&mut game, event, &mut queue, &frames);
    let snapshot = &game.entities.snapshots(0.0)[0];
    assert_eq!(snapshot.frame_direction, 2);
    assert_eq!(snapshot.frame_rotation, 3);
    let item = snapshot.item.as_ref().expect("stack");
    assert_eq!((item.item_id, item.damage), (276, 4));
}

#[test]
fn dropped_item_watchers_do_not_leak_onto_other_kinds() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    handle_net(&mut game, spawn_object_event(9, 60, 0), &mut queue, &frames);
    let event = metadata_event(9, vec![stack_watcher(10, &wire_item(264, 0))]);
    handle_net(&mut game, event, &mut queue, &frames);
    assert_eq!(game.entities.snapshots(0.0)[0].item, None);
}

#[test]
fn the_equipment_of_another_entity_sets_its_held_stack() {
    use umicraft_protocol::packets::play::clientbound::EntityEquipment;

    let mut game = game_on_floor_with(BlockState::AIR);
    game.entity_id = 1;
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    game.entities.spawn_player(
        EntityId(4),
        [0; 16],
        DVec3::ZERO,
        0.0,
        0.0,
        None,
        EntityFlags::default(),
    );
    let equip = |item| {
        PlayEvent::EntityEquipment(EntityEquipment {
            entity_id: 4,
            slot: 0,
            item,
        })
    };
    handle_net(&mut game, equip(wire_item(276, 9)), &mut queue, &frames);
    let held = game.entities.snapshots(0.0)[0].held.clone().expect("held");
    assert_eq!((held.item_id, held.damage), (276, 9));

    handle_net(&mut game, equip(WireItemStack::EMPTY), &mut queue, &frames);
    assert_eq!(game.entities.snapshots(0.0)[0].held, None);
}

#[test]
fn a_tab_list_entry_gives_the_spawned_player_its_name() {
    use umicraft_protocol::packets::play::clientbound::player_list_item::AddPlayerEntry;

    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    let uuid = [7u8; 16];
    game.entities.spawn_player(
        EntityId(4),
        uuid,
        DVec3::ZERO,
        0.0,
        0.0,
        None,
        EntityFlags::default(),
    );
    let entry = AddPlayerEntry {
        uuid: u128::from_be_bytes(uuid),
        name: "Notch".to_owned(),
        properties: Vec::new(),
        game_mode: 0,
        ping: 0,
        display_name: None,
    };
    let event = PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![entry]));
    handle_net(&mut game, event, &mut queue, &frames);
    let profile = game.entities.snapshots(0.0)[0].profile.clone();
    assert_eq!(
        profile.map(|p| p.name.to_string()),
        Some("Notch".to_owned())
    );
    assert_eq!(
        game.server.player_list().len(),
        1,
        "the tab list still updates"
    );
}

#[test]
fn a_team_mate_is_a_ghost_only_when_the_team_sees_invisible_mates() {
    use umicraft_protocol::packets::play::clientbound::player_list_item::AddPlayerEntry;
    use umicraft_protocol::packets::play::clientbound::teams::{TeamInfo, Teams};

    let mut game = game_on_floor_with(BlockState::AIR);
    game.own_name = "me".to_owned();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    let uuid = [9u8; 16];
    game.entities.spawn_player(
        EntityId(4),
        uuid,
        DVec3::ZERO,
        0.0,
        0.0,
        None,
        EntityFlags::default(),
    );
    let entry = AddPlayerEntry {
        uuid: u128::from_be_bytes(uuid),
        name: "Bob".to_owned(),
        properties: Vec::new(),
        game_mode: 0,
        ping: 0,
        display_name: None,
    };
    let listed = PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![entry]));
    handle_net(&mut game, listed, &mut queue, &frames);
    let ghost = |game: &Game| game.entities.snapshots(0.0)[0].ghost_visible;
    assert!(!ghost(&game), "no team");

    let team = |flags: i8, players: &[&str]| {
        PlayEvent::Teams(Box::new(Teams::Create {
            name: "red".to_owned(),
            info: TeamInfo {
                display_name: "Red".to_owned(),
                prefix: String::new(),
                suffix: String::new(),
                friendly_flags: flags,
                name_tag_visibility: "always".to_owned(),
                color: 12,
            },
            players: players.iter().map(|name| (*name).to_owned()).collect(),
        }))
    };
    // Bob alone in the team: the local player is not a mate.
    handle_net(&mut game, team(2, &["Bob"]), &mut queue, &frames);
    assert!(!ghost(&game));
    let remove = || {
        PlayEvent::Teams(Box::new(Teams::Remove {
            name: "red".to_owned(),
        }))
    };
    handle_net(&mut game, remove(), &mut queue, &frames);
    // Both in the team, but it does not see invisible mates.
    handle_net(&mut game, team(0, &["Bob", "me"]), &mut queue, &frames);
    assert!(!ghost(&game));
    handle_net(&mut game, remove(), &mut queue, &frames);
    handle_net(&mut game, team(2, &["Bob", "me"]), &mut queue, &frames);
    assert!(ghost(&game));
}

#[test]
fn the_objective_below_the_name_adds_a_score_line_to_the_tag() {
    use umicraft_protocol::packets::play::clientbound::player_list_item::AddPlayerEntry;
    use umicraft_protocol::packets::play::clientbound::{
        DisplayScoreboard, ObjectiveRenderType, ScoreboardObjective, UpdateScore,
    };

    let mut game = game_on_floor_with(BlockState::AIR);
    game.own_name = "me".to_owned();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    let uuid = [9u8; 16];
    game.entities.spawn_player(
        EntityId(4),
        uuid,
        DVec3::ZERO,
        0.0,
        0.0,
        None,
        EntityFlags::default(),
    );
    let entry = AddPlayerEntry {
        uuid: u128::from_be_bytes(uuid),
        name: "Bob".to_owned(),
        properties: Vec::new(),
        game_mode: 0,
        ping: 0,
        display_name: None,
    };
    let listed = PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![entry]));
    handle_net(&mut game, listed, &mut queue, &frames);
    let tag = |game: &Game| game.entities.snapshots(0.0)[0].name_tag.clone();
    assert_eq!(tag(&game).as_deref(), Some("Bob"));

    let objective = PlayEvent::ScoreboardObjective(ScoreboardObjective::Create {
        name: "kills".to_owned(),
        value: "Kills".to_owned(),
        render_type: ObjectiveRenderType::Integer,
    });
    handle_net(&mut game, objective, &mut queue, &frames);
    let display = PlayEvent::DisplayScoreboard(DisplayScoreboard {
        position: 2,
        score_name: "kills".to_owned(),
    });
    handle_net(&mut game, display, &mut queue, &frames);
    // No score yet: vanilla shows 0.
    assert_eq!(tag(&game).as_deref(), Some("Bob\n0 Kills"));
    let score = PlayEvent::UpdateScore(UpdateScore::Change {
        name: "Bob".to_owned(),
        objective: "kills".to_owned(),
        value: 12,
    });
    handle_net(&mut game, score, &mut queue, &frames);
    assert_eq!(tag(&game).as_deref(), Some("Bob\n12 Kills"));
}

#[test]
fn a_team_decides_the_name_tag_of_a_player() {
    use umicraft_protocol::packets::play::clientbound::player_list_item::AddPlayerEntry;
    use umicraft_protocol::packets::play::clientbound::teams::{TeamInfo, Teams};

    let mut game = game_on_floor_with(BlockState::AIR);
    game.own_name = "me".to_owned();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    let uuid = [9u8; 16];
    game.entities.spawn_player(
        EntityId(4),
        uuid,
        DVec3::ZERO,
        0.0,
        0.0,
        None,
        EntityFlags::default(),
    );
    let entry = AddPlayerEntry {
        uuid: u128::from_be_bytes(uuid),
        name: "Bob".to_owned(),
        properties: Vec::new(),
        game_mode: 0,
        ping: 0,
        display_name: None,
    };
    let listed = PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![entry]));
    handle_net(&mut game, listed, &mut queue, &frames);
    let tag = |game: &Game| game.entities.snapshots(0.0)[0].name_tag.clone();
    assert_eq!(tag(&game).as_deref(), Some("Bob"));

    let team = |visibility: &str| {
        PlayEvent::Teams(Box::new(Teams::Create {
            name: "red".to_owned(),
            info: TeamInfo {
                display_name: "Red".to_owned(),
                prefix: "§c".to_owned(),
                suffix: String::new(),
                friendly_flags: 0,
                name_tag_visibility: visibility.to_owned(),
                color: 12,
            },
            players: vec!["Bob".to_owned()],
        }))
    };
    handle_net(&mut game, team("always"), &mut queue, &frames);
    assert_eq!(tag(&game).as_deref(), Some("§cBob"));
    handle_net(
        &mut game,
        PlayEvent::Teams(Box::new(Teams::Remove {
            name: "red".to_owned(),
        })),
        &mut queue,
        &frames,
    );
    assert_eq!(tag(&game).as_deref(), Some("Bob"));
    handle_net(&mut game, team("never"), &mut queue, &frames);
    assert_eq!(tag(&game), None);
}

#[test]
fn a_hurt_status_of_the_local_player_rolls_the_camera() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.entity_id = 7;
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    follow_body(&mut game);
    handle_net(&mut game, health_update(10.0), &mut queue, &frames);
    follow_body(&mut game);
    handle_net(&mut game, hurt_status(7, 2), &mut queue, &frames);
    assert_eq!(game.body.animation.hurt_time(), 10);
    assert!(game.body.animation.is_hurt_resistant());
    // Someone else's hurt does not shake this camera.
    let mut other = game_on_floor_with(BlockState::AIR);
    other.entity_id = 7;
    handle_net(&mut other, hurt_status(8, 2), &mut queue, &frames);
    assert_eq!(other.body.animation.hurt_time(), 0);
}

#[test]
fn a_hurt_status_of_the_local_player_plays_the_hurt_sound_once() {
    // Regression: no damage sound was played when the player was hit.
    let mut game = game_on_floor_with(BlockState::AIR);
    game.entity_id = 7;
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    let feet = game.state.as_ref().unwrap().position();
    handle_net(&mut game, hurt_status(7, 2), &mut queue, &frames);
    let cues = game.state.as_mut().unwrap().drain_sound_cues();
    let hurt: Vec<_> = cues
        .iter()
        .filter(|c| c.name == "game.player.hurt")
        .collect();
    assert_eq!(hurt.len(), 1);
    assert_eq!(hurt[0].pos, feet.to_array());
    handle_net(&mut game, hurt_status(7, 3), &mut queue, &frames);
    let cues = game.state.as_mut().unwrap().drain_sound_cues();
    assert!(cues.iter().any(|c| c.name == "game.player.die"));
}

#[test]
fn a_hurt_status_of_another_entity_is_silent_because_the_server_sends_the_sound() {
    // `RenderGlobal.playSound` is empty in 1.8: playing it here too would
    // double the Sound Effect packet of the same hit.
    let mut game = game_on_floor_with(BlockState::AIR);
    game.entity_id = 7;
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    handle_net(&mut game, hurt_status(8, 2), &mut queue, &frames);
    let cues = game.state.as_mut().unwrap().drain_sound_cues();
    assert!(cues.iter().all(|c| c.name != "game.player.hurt"));
}

#[test]
fn a_critical_animation_throws_stars_for_three_ticks() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    game.entities.spawn_mob(
        EntityId(30),
        54,
        DVec3::new(2.5, 64.0, 0.5),
        0.0,
        0.0,
        0.0,
        Vec3::ZERO,
        EntityFlags::default(),
    );
    let animation = |code| {
        PlayEvent::Animation(umicraft_protocol::packets::play::clientbound::Animation {
            entity_id: 30,
            animation: code,
        })
    };
    // A swing throws nothing.
    handle_net(&mut game, animation(0), &mut queue, &frames);
    assert!(game.particles.is_empty());
    handle_net(&mut game, animation(4), &mut queue, &frames);
    let first = game.particles.len();
    assert!(first > 0, "the first burst comes at once");
    assert_eq!(game.hit_emitters.len(), 1);
    combat::tick_emitters(&mut game);
    combat::tick_emitters(&mut game);
    assert!(game.particles.len() > first);
    assert_eq!(game.hit_emitters.len(), 1);
    combat::tick_emitters(&mut game);
    assert!(game.hit_emitters.is_empty(), "three bursts, then done");
}

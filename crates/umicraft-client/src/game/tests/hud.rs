// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the HUD shows: the F3 facts, the tab list, the social events and the statistics.

use super::*;

#[test]
fn the_f3_facts_hold_the_feet_the_biome_and_the_chunk_cache() {
    let game = placed_game();
    let facts = debug_world(&game);
    let feet = game.state.as_ref().expect("state").position();
    assert_eq!(facts.feet, [feet.x, feet.y, feet.z]);
    assert_eq!(
        facts.chunk_cache.0,
        game.world.as_ref().expect("world").chunk_count()
    );
    let place = facts.location.expect("the player column is loaded");
    assert_eq!(place.day, 0);
    assert!(place.sky_light <= 15);
}

#[test]
fn the_hud_carries_the_ping_of_the_local_tab_list_entry() {
    use umicraft_protocol::packets::play::clientbound::player_list_item::AddPlayerEntry;

    let mut game = game_on_floor_with(BlockState::AIR);
    game.own_name = "Me".to_owned();
    assert_eq!(game.hud().ping_ms, None, "not listed yet");
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    let entry = |uuid: u8, name: &str, ping: i32| AddPlayerEntry {
        uuid: u128::from_be_bytes([uuid; 16]),
        name: name.to_owned(),
        properties: Vec::new(),
        game_mode: 0,
        ping,
        display_name: None,
    };
    let event = PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![
        entry(1, "Other", 250),
        entry(2, "Me", 42),
    ]));
    handle_net(&mut game, event, &mut queue, &frames);
    assert_eq!(game.hud().ping_ms, Some(42), "the own entry, not another");
}

#[test]
fn the_social_events_fill_the_hud() {
    use umicraft_protocol::packets::play::clientbound::{
        AddPlayerEntry, DisplayScoreboard, ObjectiveRenderType, PlayerListHeaderFooter,
        PlayerListItem, ScoreboardObjective, Title, UpdateScore,
    };

    let mut game = Game::new();
    game.own_name = "Steve".to_string();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    let mut send = |game: &mut Game, event| handle_net(game, event, &mut queue, &frames);

    send(
        &mut game,
        PlayEvent::ScoreboardObjective(ScoreboardObjective::Create {
            name: "kills".to_string(),
            value: "Kills".to_string(),
            render_type: ObjectiveRenderType::Integer,
        }),
    );
    send(
        &mut game,
        PlayEvent::DisplayScoreboard(DisplayScoreboard {
            position: 1,
            score_name: "kills".to_string(),
        }),
    );
    send(
        &mut game,
        PlayEvent::UpdateScore(UpdateScore::Change {
            name: "Alex".to_string(),
            objective: "kills".to_string(),
            value: 4,
        }),
    );
    send(
        &mut game,
        PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![AddPlayerEntry {
            uuid: u128::from_be_bytes([1; 16]),
            name: "Steve".to_string(),
            properties: vec![],
            game_mode: 0,
            ping: 37,
            display_name: None,
        }])),
    );
    send(
        &mut game,
        PlayEvent::PlayerListHeaderFooter(PlayerListHeaderFooter {
            header: r#"{"text":"Welcome"}"#.to_string(),
            footer: r#"{"text":""}"#.to_string(),
        }),
    );
    send(
        &mut game,
        PlayEvent::Title(Title::Title {
            json: r#"{"text":"Round 1"}"#.to_string(),
        }),
    );
    send(
        &mut game,
        PlayEvent::ActionBar(r#"{"text":"Go!"}"#.to_string()),
    );

    // The title fades in from nothing, so it needs a few ticks to show.
    for _ in 0..5 {
        game.title.tick();
    }
    let hud = game.hud();
    let sidebar = hud.sidebar.expect("the sidebar objective is displayed");
    assert_eq!(plain_text(&sidebar.title), "Kills");
    assert_eq!(sidebar.lines.len(), 1);
    assert_eq!(plain_text(&sidebar.lines[0].name), "Alex");
    assert_eq!(sidebar.lines[0].score, 4);

    let tab_list = hud.tab_list.expect("a player is listed");
    assert_eq!(
        tab_list.header.as_deref().map(plain_text).as_deref(),
        Some("Welcome")
    );
    assert_eq!(tab_list.footer, None, "an empty footer is absent");
    assert_eq!(tab_list.entries.len(), 1);
    assert_eq!(plain_text(&tab_list.entries[0].name), "Steve");
    assert_eq!(tab_list.entries[0].ping, 37);

    let title = hud.title.expect("a title is displayed");
    assert_eq!(plain_text(&title.title), "Round 1");
    assert!(title.alpha > 0.0);
    let action_bar = hud.action_bar.expect("an action bar is displayed");
    assert_eq!(plain_text(&action_bar.text), "Go!");
    assert!(action_bar.alpha > 0.0);
}

#[test]
fn a_dimension_change_clears_the_scoreboard_but_keeps_the_tab_list() {
    use umicraft_protocol::packets::play::clientbound::{
        AddPlayerEntry, DisplayScoreboard, ObjectiveRenderType, PlayerListItem, Respawn,
        ScoreboardObjective,
    };

    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    handle_net(
        &mut game,
        PlayEvent::ScoreboardObjective(ScoreboardObjective::Create {
            name: "o".to_string(),
            value: "O".to_string(),
            render_type: ObjectiveRenderType::Integer,
        }),
        &mut queue,
        &frames,
    );
    handle_net(
        &mut game,
        PlayEvent::DisplayScoreboard(DisplayScoreboard {
            position: 1,
            score_name: "o".to_string(),
        }),
        &mut queue,
        &frames,
    );
    handle_net(
        &mut game,
        PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![AddPlayerEntry {
            uuid: u128::from_be_bytes([2; 16]),
            name: "Alex".to_string(),
            properties: vec![],
            game_mode: 0,
            ping: 5,
            display_name: None,
        }])),
        &mut queue,
        &frames,
    );
    assert!(game.hud().sidebar.is_some());

    handle_net(
        &mut game,
        PlayEvent::Respawn(Respawn {
            dimension: -1,
            difficulty: 0,
            game_mode: 0,
            world_type: "default".to_string(),
        }),
        &mut queue,
        &frames,
    );
    let hud = game.hud();
    assert!(hud.sidebar.is_none(), "a new world has a new scoreboard");
    assert!(
        hud.tab_list.is_some(),
        "the tab list belongs to the connection"
    );
}

#[test]
fn boss_mob_spawn_and_metadata_updates_hud_boss_bar() {
    use umicraft_protocol::packets::play::clientbound::{
        DestroyEntities, EntityMetadata, SpawnMob,
    };
    use umicraft_protocol::packets::play::metadata::{Metadata, WatchedObject};

    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);

    // Spawn Ender Dragon (mob type 63) with full health (200.0)
    let mut metadata = Metadata::default();
    metadata.objects.push(WatchedObject {
        type_id: 3,
        index: 6,
        value: 200.0_f32.to_be_bytes().to_vec(),
    });
    handle_net(
        &mut game,
        PlayEvent::SpawnMob(SpawnMob {
            entity_id: 500,
            mob_type: 63,
            x: 0,
            y: 64 * 32,
            z: 0,
            yaw: 0,
            pitch: 0,
            head_yaw: 0,
            velocity_x: 0,
            velocity_y: 0,
            velocity_z: 0,
            metadata,
        }),
        &mut queue,
        &frames,
    );

    social_view::tick_boss_bar(&mut game.boss_bar, &game.entities);
    let hud = game.hud();
    let boss = hud.boss.expect("boss bar present after dragon spawn");
    assert_eq!(plain_text(&boss.name), "Ender Dragon");
    assert_eq!(boss.health, 1.0);

    // Update health via EntityMetadata to 100.0 (half health)
    let mut update_meta = Metadata::default();
    update_meta.objects.push(WatchedObject {
        type_id: 3,
        index: 6,
        value: 100.0_f32.to_be_bytes().to_vec(),
    });
    handle_net(
        &mut game,
        PlayEvent::EntityMetadata(EntityMetadata {
            entity_id: 500,
            metadata: update_meta,
        }),
        &mut queue,
        &frames,
    );

    social_view::tick_boss_bar(&mut game.boss_bar, &game.entities);
    let hud = game.hud();
    let boss = hud.boss.expect("boss bar present after damage");
    assert_eq!(boss.health, 0.5);

    // Destroy dragon
    handle_net(
        &mut game,
        PlayEvent::DestroyEntities(DestroyEntities {
            entity_ids: vec![500],
        }),
        &mut queue,
        &frames,
    );
    // `BossStatus.statusBarTime` is 100 when the dragon was last seen and
    // the tick of that same frame already took one off: 99 remain.
    for _ in 0..98 {
        social_view::tick_boss_bar(&mut game.boss_bar, &game.entities);
    }
    assert!(game.hud().boss.is_some(), "the bar outlives the dragon");
    social_view::tick_boss_bar(&mut game.boss_bar, &game.entities);
    assert!(
        game.hud().boss.is_none(),
        "boss bar disappears 100 ticks after the dragon"
    );
}

#[test]
fn a_statistics_packet_is_forwarded_by_name() {
    use umicraft_protocol::packets::play::clientbound::Statistics;

    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, received) = mpsc::sync_channel(16);
    let packet = Statistics {
        entries: vec![("stat.jump".to_string(), 7), ("stat.deaths".to_string(), 1)],
    };
    handle_net(
        &mut game,
        PlayEvent::Statistics(packet),
        &mut queue,
        &frames,
    );
    let stats = received.try_iter().find_map(|event| match event {
        RenderEvent::Stats(stats) => Some(stats),
        _ => None,
    });
    assert_eq!(
        stats,
        Some(vec![
            ("stat.deaths".to_string(), 1),
            ("stat.jump".to_string(), 7)
        ])
    );
}

#[test]
fn opening_the_statistics_asks_the_server() {
    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    handle_event(&mut game, GameEvent::RequestStats, &mut queue, &frames);
    assert!(matches!(
        queue.ordered.front(),
        Some(ClientCommand::RequestStats)
    ));
}

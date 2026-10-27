// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the [`ServerState`] routing: local-only packets, session and
//! respawn resets, and the simple stored values. The submodules test their
//! own logic.

use umicraft_protocol::packets::play::clientbound::entity_properties::{
    AttributeModifier, EntityProperty,
};
use umicraft_protocol::packets::play::clientbound::player_list_item::AddPlayerEntry;
use umicraft_protocol::packets::play::clientbound::scoreboard_objective::ObjectiveRenderType;
use umicraft_protocol::packets::play::clientbound::title::Title;
use umicraft_protocol::packets::play::clientbound::world_border::WorldBorder;
use umicraft_protocol::types::write_string;

use super::*;

const LOCAL: i32 = 5;

fn joined() -> ServerState {
    let mut state = ServerState::new();
    state.on_join(LOCAL);
    state
}

fn effect(entity_id: i32) -> EntityEffect {
    EntityEffect {
        entity_id,
        effect_id: 1,
        amplifier: 0,
        duration: 3,
        hide_particles: 0,
    }
}

fn speed_packet(entity_id: i32) -> EntityProperties {
    EntityProperties {
        entity_id,
        properties: vec![EntityProperty {
            key: "generic.movementSpeed".to_owned(),
            value: 0.1,
            modifiers: vec![AttributeModifier {
                uuid: 1,
                amount: 1.0,
                operation: 2,
            }],
        }],
    }
}

#[test]
fn effects_and_attributes_only_apply_to_the_local_player() {
    let mut state = joined();
    state.apply_entity_effect(&effect(LOCAL + 1));
    state.apply_entity_properties(&speed_packet(LOCAL + 1));
    assert!(state.effects().is_empty());
    assert_eq!(state.attribute_value("generic.movementSpeed"), None);

    state.apply_entity_effect(&effect(LOCAL));
    state.apply_entity_properties(&speed_packet(LOCAL));
    assert_eq!(state.effects().len(), 1);
    assert_eq!(state.attribute_value("generic.movementSpeed"), Some(0.2));

    state.apply_remove_entity_effect(&RemoveEntityEffect {
        entity_id: LOCAL + 1,
        effect_id: 1,
    });
    assert_eq!(state.effects().len(), 1);
    state.apply_remove_entity_effect(&RemoveEntityEffect {
        entity_id: LOCAL,
        effect_id: 1,
    });
    assert!(state.effects().is_empty());
}

#[test]
fn before_a_join_nothing_is_local() {
    let mut state = ServerState::new();
    state.apply_entity_effect(&effect(0));
    assert!(state.effects().is_empty());
    assert_eq!(state.local_entity_id(), None);
}

#[test]
fn tick_advances_effects_title_and_border() {
    let mut state = joined();
    state.apply_entity_effect(&effect(LOCAL));
    state.apply_title(&Title::Title {
        json: "t".to_owned(),
    });
    state.apply_world_border(&WorldBorder::LerpSize {
        old_diameter: 0.0,
        new_diameter: 100.0,
        speed: 100,
    });
    state.tick();
    assert_eq!(state.effects().get(1).unwrap().duration, 2);
    assert_eq!(state.title().timer(), 99);
    assert_eq!(state.border().diameter(), 50.0);
}

#[test]
fn on_join_clears_everything_and_sets_the_local_id() {
    let mut state = joined();
    state.apply_difficulty(&ServerDifficulty { difficulty: 3 });
    state.apply_statistics(&Statistics {
        entries: vec![("a".to_owned(), 1)],
    });
    state.on_join(9);
    assert_eq!(state.local_entity_id(), Some(9));
    assert_eq!(state.difficulty(), None);
    assert!(state.statistics().is_empty());
}

fn populated() -> ServerState {
    let mut state = joined();
    state.apply_entity_effect(&effect(LOCAL));
    state.apply_entity_properties(&speed_packet(LOCAL));
    state.apply_camera(&Camera { entity_id: 77 });
    state.apply_combat_event(&CombatEvent::EntityDied {
        player_id: LOCAL,
        entity_id: -1,
        message: "dead".to_owned(),
    });
    state.apply_sign_editor_open(&SignEditorOpen { x: 1, y: 2, z: 3 });
    state.apply_sign(&UpdateSign {
        x: 1,
        y: 2,
        z: 3,
        lines: Default::default(),
    });
    state.apply_block_break_anim(&BlockBreakAnim {
        entity_id: 1,
        x: 0,
        y: 0,
        z: 0,
        stage: 3,
    });
    state.apply_map(&Maps {
        map_id: 1,
        scale: 0,
        icons: vec![],
        columns: None,
    });
    state.apply_game_state(&ChangeGameState {
        reason: 1,
        value: 0.0,
    });
    state.apply_world_border(&WorldBorder::SetSize { diameter: 10.0 });
    state.apply_objective(&ScoreboardObjective::Create {
        name: "o".to_owned(),
        value: "O".to_owned(),
        render_type: ObjectiveRenderType::Integer,
    });
    state.apply_player_list(&PlayerListItem::AddPlayer(vec![AddPlayerEntry {
        uuid: 1,
        name: "A".to_owned(),
        properties: vec![],
        game_mode: 0,
        ping: 0,
        display_name: None,
    }]));
    state.apply_statistics(&Statistics {
        entries: vec![("s".to_owned(), 2)],
    });
    state
}

#[test]
fn a_respawn_in_the_same_dimension_resets_only_the_player_state() {
    let mut state = populated();
    state.on_respawn(false);
    assert!(state.effects().is_empty());
    assert!(state.attributes().is_empty());
    assert_eq!(state.camera_target(), None);
    assert_eq!(state.death_message(), None);
    assert_eq!(state.pending_sign_editor(), None);
    assert!(state.block_entities().sign((1, 2, 3)).is_some());
    assert!(state.block_entities().break_progress(1).is_some());
    assert!(state.maps().get(1).is_some());
    assert!(state.weather().raining());
    assert_eq!(state.border().diameter(), 10.0);
}

#[test]
fn a_dimension_change_also_resets_the_world_but_keeps_the_scoreboard() {
    let mut state = populated();
    state.on_respawn(true);
    assert!(state.block_entities().sign((1, 2, 3)).is_none());
    assert!(state.block_entities().break_progress(1).is_none());
    assert!(state.maps().is_empty());
    assert!(!state.weather().raining());
    assert_eq!(state.border().diameter(), 6.0e7);
    assert!(state.scoreboard().objective("o").is_some());
    assert_eq!(state.player_list().len(), 1);
    assert_eq!(state.statistic("s"), Some(2));
}

#[test]
fn simple_values_are_stored() {
    let mut state = joined();
    state.apply_abilities(&PlayerAbilities {
        flags: 0b0110,
        fly_speed: 0.05,
        walk_speed: 0.1,
    });
    assert!(state.abilities().unwrap().flying());
    state.apply_difficulty(&ServerDifficulty { difficulty: 2 });
    assert_eq!(state.difficulty(), Some(2));
    state.apply_resource_pack_offer(&ResourcePackSend {
        url: "http://x".to_owned(),
        hash: "abc".to_owned(),
    });
    let offer = state.resource_pack_offer().unwrap();
    assert_eq!(
        (offer.url.as_str(), offer.hash.as_str()),
        ("http://x", "abc")
    );
    state.apply_header_footer(&PlayerListHeaderFooter {
        header: "h".to_owned(),
        footer: "f".to_owned(),
    });
    assert_eq!(state.player_list().header(), "h");
}

#[test]
fn statistics_merge_by_name() {
    let mut state = joined();
    state.apply_statistics(&Statistics {
        entries: vec![("a".to_owned(), 1), ("b".to_owned(), 2)],
    });
    state.apply_statistics(&Statistics {
        entries: vec![("a".to_owned(), 5)],
    });
    assert_eq!(state.statistic("a"), Some(5));
    assert_eq!(state.statistic("b"), Some(2));
    assert_eq!(state.statistic("c"), None);
}

#[test]
fn the_camera_on_the_local_player_means_normal_view() {
    let mut state = joined();
    state.apply_camera(&Camera { entity_id: 12 });
    assert_eq!(state.camera_target(), Some(12));
    state.apply_camera(&Camera { entity_id: LOCAL });
    assert_eq!(state.camera_target(), None);
}

#[test]
fn only_the_local_death_stores_a_message() {
    let mut state = joined();
    let died = |player_id| CombatEvent::EntityDied {
        player_id,
        entity_id: 0,
        message: "msg".to_owned(),
    };
    state.apply_combat_event(&died(LOCAL + 1));
    state.apply_combat_event(&CombatEvent::EnterCombat);
    assert_eq!(state.death_message(), None);
    state.apply_combat_event(&died(LOCAL));
    assert_eq!(state.death_message(), Some("msg"));
}

fn brand_message(channel: &str, data: Vec<u8>) -> PluginMessage {
    PluginMessage {
        channel: channel.to_owned(),
        data,
    }
}

#[test]
fn the_brand_is_read_from_mc_brand_only() {
    let mut state = joined();
    let mut payload = Vec::new();
    write_string(&mut payload, "vanilla", 32767).unwrap();
    state.apply_plugin_message(&brand_message("other", payload.clone()));
    assert_eq!(state.server_brand(), None);
    state.apply_plugin_message(&brand_message("MC|Brand", payload));
    assert_eq!(state.server_brand(), Some("vanilla"));
}

#[test]
fn a_malformed_brand_is_ignored() {
    let mut state = joined();
    state.apply_plugin_message(&brand_message("MC|Brand", vec![]));
    state.apply_plugin_message(&brand_message("MC|Brand", vec![200, 1]));
    state.apply_plugin_message(&brand_message("MC|Brand", vec![2, 0xFF, 0xFE]));
    assert_eq!(state.server_brand(), None);
}

#[test]
fn the_sign_editor_request_is_taken_once() {
    let mut state = joined();
    state.apply_sign_editor_open(&SignEditorOpen { x: 4, y: 5, z: 6 });
    assert_eq!(state.pending_sign_editor(), Some((4, 5, 6)));
    assert_eq!(state.take_sign_editor(), Some((4, 5, 6)));
    assert_eq!(state.take_sign_editor(), None);
}

#[test]
fn the_remaining_routes_reach_their_stores() {
    let mut state = joined();
    state.apply_team(&Teams::Remove {
        name: "ghost".to_owned(),
    });
    state.apply_score(&UpdateScore::Remove {
        name: "x".to_owned(),
        objective: "ghost".to_owned(),
    });
    state.apply_display(&DisplayScoreboard {
        position: 99,
        score_name: String::new(),
    });
    state.apply_block_entity(&UpdateBlockEntity {
        x: 0,
        y: 0,
        z: 0,
        action: 1,
        nbt: None,
    });
    state.block_entities_mut().remove((0, 0, 0));
    assert!(state.block_entities().block_entity((0, 0, 0)).is_none());
    assert_eq!(state.scoreboard().teams().count(), 0);
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The chat: formatting, history, completions and visibility.

use super::*;

#[test]
fn a_formatted_chat_message_keeps_its_styles_and_arrival_time() {
    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    let before = Instant::now();
    handle_net(
        &mut game,
        PlayEvent::Chat(
            r#"{"text":"Hi ","color":"red","extra":[{"text":"there","bold":true}]}"#.to_string(),
        ),
        &mut queue,
        &frames,
    );
    let hud = game.hud();
    assert_eq!(hud.chat.len(), 1);
    assert_eq!(hud.chat[0].plain(), "Hi there");
    assert_eq!(hud.chat[0].runs[0].color, [0xFF, 0x55, 0x55]);
    assert!(!hud.chat[0].runs[0].bold);
    assert!(hud.chat[0].runs[1].bold);
    assert!(hud.chat[0].received >= before);
}

#[test]
fn an_empty_chat_component_is_a_blank_line_not_raw_json() {
    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    handle_net(
        &mut game,
        PlayEvent::Chat(r#"{"text":""}"#.to_string()),
        &mut queue,
        &frames,
    );
    assert_eq!(game.hud().chat[0].plain(), "");
}

#[test]
fn the_chat_keeps_the_last_one_hundred_messages() {
    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    for number in 0..150 {
        handle_net(
            &mut game,
            PlayEvent::Chat(format!(r#"{{"text":"message {number}"}}"#)),
            &mut queue,
            &frames,
        );
    }
    let chat = game.hud().chat;
    assert_eq!(chat.len(), CHAT_HISTORY);
    assert_eq!(chat[0].plain(), "message 50");
    assert_eq!(chat[99].plain(), "message 149");
}

#[test]
fn the_typed_chat_limit_counts_characters_not_bytes() {
    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    // 100 two-byte characters: 200 bytes, but within the 100-character limit.
    let message = "\u{e9}".repeat(100);
    handle_event(
        &mut game,
        GameEvent::Chat(message.clone()),
        &mut queue,
        &frames,
    );
    assert_eq!(queue.ordered.len(), 1, "a 100-character message is sent");
    handle_event(
        &mut game,
        GameEvent::Chat("\u{e9}".repeat(101)),
        &mut queue,
        &frames,
    );
    assert_eq!(queue.ordered.len(), 1, "101 characters is too long");
}

#[test]
fn a_tab_request_is_queued_and_the_answer_is_published_with_a_new_serial() {
    use umicraft_protocol::packets::play::clientbound::TabComplete;

    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    handle_event(
        &mut game,
        GameEvent::TabComplete("/he".to_string()),
        &mut queue,
        &frames,
    );
    assert!(matches!(
        queue.ordered.front(),
        Some(ClientCommand::TabComplete { message }) if message == "/he"
    ));

    assert_eq!(game.hud().completions_serial, 0);
    handle_net(
        &mut game,
        PlayEvent::TabComplete(TabComplete {
            matches: vec!["/help".to_string()],
        }),
        &mut queue,
        &frames,
    );
    let hud = game.hud();
    assert_eq!(hud.completions, ["/help"]);
    assert_eq!(hud.completions_serial, 1);
}

#[test]
fn the_chat_visibility_filters_incoming_chat_and_system_messages() {
    let mut game = Game::new();
    let say = |game: &mut Game, event: PlayEvent| {
        let (frames, _received) = mpsc::sync_channel(16);
        handle_net(game, event, &mut CommandQueue::default(), &frames);
    };
    // Commands only: the system message stays, the player message goes.
    game.chat_mode = 1;
    say(
        &mut game,
        PlayEvent::Chat(r#"{"text":"player"}"#.to_string()),
    );
    say(
        &mut game,
        PlayEvent::SystemChat(r#"{"text":"system"}"#.to_string()),
    );
    // Hidden: nothing is kept.
    game.chat_mode = 2;
    say(
        &mut game,
        PlayEvent::SystemChat(r#"{"text":"late"}"#.to_string()),
    );
    let plain: Vec<_> = game.hud().chat.iter().map(ChatLineView::plain).collect();
    assert_eq!(plain, ["system"]);
}

#[test]
fn a_client_settings_event_updates_the_chat_visibility_filter() {
    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    handle_event(
        &mut game,
        GameEvent::ClientSettings {
            view_distance: 8,
            chat_mode: 2,
            chat_colors: true,
            skin_parts: 0x7F,
        },
        &mut queue,
        &frames,
    );
    assert_eq!(game.chat_mode, 2);
}

#[test]
fn a_local_chat_line_is_never_parsed_as_json() {
    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    for text in ["null", "123", "Steve, Alex"] {
        handle_event(
            &mut game,
            GameEvent::LocalChat(text.to_string()),
            &mut queue,
            &frames,
        );
    }
    let plain: Vec<_> = game.hud().chat.iter().map(ChatLineView::plain).collect();
    assert_eq!(plain, ["null", "123", "Steve, Alex"]);
}

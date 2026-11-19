// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mod events of the game thread (ADR-0086, F3.3): the two tick events and
//! the two chat events.
//!
//! Every emission first asks [`EventBus::has_subscribers`], because building an
//! event copies its strings: with no mod loaded a tick or a chat line costs one
//! map lookup and no allocation. The bus belongs to the game thread, so there is
//! no lock; the intercept handlers of the mods run right here under their time
//! budget (see `umicraft-modhost`).

use std::borrow::Cow;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

use umicraft_core::events::EventBus;
use umicraft_game::chat::{commands, Position};
use umicraft_modapi_sys::{
    UMI_CHAT_POSITION_ACTION_BAR, UMI_CHAT_POSITION_CHAT, UMI_CHAT_POSITION_SYSTEM,
};
use umicraft_modhost::{ChatReceived, ChatSend, PostTick, PreTick};

use super::commands::CommandQueue;
use super::messages::{ChunkEvent, RenderEvent};
use super::state::Game;
use super::tick::tick;

/// Runs one game tick between its [`PreTick`] and [`PostTick`] events.
///
/// The number is the index of the tick, from zero; both events of one tick carry
/// the same value so a mod can pair them.
pub(super) fn run_tick(
    game: &mut Game,
    queue: &mut CommandQueue,
    frames: &SyncSender<RenderEvent>,
    chunks: &SyncSender<ChunkEvent>,
    tick_time: Instant,
) {
    let number = game.status_data.ticks;
    if game.bus.has_subscribers::<PreTick>() {
        game.bus.dispatch(&mut PreTick::new(number));
    }
    tick(game, queue, frames, chunks, tick_time);
    if game.bus.has_subscribers::<PostTick>() {
        game.bus.dispatch(&mut PostTick::new(number));
    }
}

/// The wire value of a chat position, as in the type byte of `S02PacketChat`.
const fn position_byte(position: Position) -> u8 {
    match position {
        Position::Chat => UMI_CHAT_POSITION_CHAT,
        Position::System => UMI_CHAT_POSITION_SYSTEM,
        Position::ActionBar => UMI_CHAT_POSITION_ACTION_BAR,
    }
}

/// Lets the mods see a received chat JSON before it is shown.
///
/// Returns `None` when a mod cancelled the line, otherwise the JSON to parse:
/// the original (borrowed, no copy) or the one a mod replaced.
pub(super) fn incoming_chat<'a>(
    bus: &mut EventBus,
    json: &'a str,
    position: Position,
) -> Option<Cow<'a, str>> {
    if !bus.has_subscribers::<ChatReceived>() {
        return Some(Cow::Borrowed(json));
    }
    let mut event = ChatReceived::new(json, position_byte(position));
    if bus.dispatch(&mut event) {
        return None;
    }
    Some(Cow::Owned(event.json().to_owned()))
}

/// Lets the mods see a line the player typed before it is queued as a chat
/// packet, and returns what to send, if anything.
///
/// The length check runs after the mods, so a mod cannot send a line the server
/// would reject.
pub(super) fn outgoing_chat(bus: &mut EventBus, message: String) -> Option<String> {
    if !bus.has_subscribers::<ChatSend>() {
        return sendable(message);
    }
    let mut event = ChatSend::new(&message);
    if bus.dispatch(&mut event) {
        return None;
    }
    sendable(event.text().to_owned())
}

/// The line if the server accepts it: not empty and within the limit, which
/// counts characters, not bytes.
fn sendable(message: String) -> Option<String> {
    (!message.is_empty() && message.chars().count() <= commands::MAX_LEN).then_some(message)
}

#[cfg(test)]
mod tests {
    use umicraft_core::module::ModuleId;

    use super::*;

    fn bus_with<E: 'static>(
        handler: impl FnMut(&mut E, &mut umicraft_core::events::EventCx) + Send + 'static,
    ) -> EventBus {
        let mut bus = EventBus::default();
        bus.on(ModuleId::new("test"), 0, handler);
        bus
    }

    #[test]
    fn without_a_mod_chat_passes_unchanged() {
        let mut bus = EventBus::default();
        let json = r#"{"text":"hi"}"#;
        assert!(matches!(
            incoming_chat(&mut bus, json, Position::Chat),
            Some(Cow::Borrowed(same)) if same == json
        ));
        assert_eq!(
            outgoing_chat(&mut bus, "hello".into()).as_deref(),
            Some("hello")
        );
    }

    #[test]
    fn a_cancelled_received_line_is_hidden() {
        let mut bus = bus_with(|_: &mut ChatReceived, cx| cx.cancel());
        assert!(incoming_chat(&mut bus, "{}", Position::System).is_none());
    }

    #[test]
    fn a_replaced_received_line_is_the_one_shown_and_carries_the_position() {
        let mut bus = bus_with(|event: &mut ChatReceived, _| {
            assert_eq!(event.position(), 2);
            event.set_json(r#"{"text":"changed"}"#);
        });
        let shown = incoming_chat(&mut bus, "{}", Position::ActionBar);
        assert_eq!(shown.as_deref(), Some(r#"{"text":"changed"}"#));
    }

    #[test]
    fn a_cancelled_outgoing_line_is_not_sent() {
        let mut bus = bus_with(|_: &mut ChatSend, cx| cx.cancel());
        assert_eq!(outgoing_chat(&mut bus, "hello".into()), None);
    }

    #[test]
    fn a_replaced_outgoing_line_is_sent_in_its_place() {
        let mut bus = bus_with(|event: &mut ChatSend, _| event.set_text("replaced"));
        assert_eq!(
            outgoing_chat(&mut bus, "hello".into()).as_deref(),
            Some("replaced")
        );
    }

    #[test]
    fn a_mod_cannot_send_an_overlong_line() {
        let long = "x".repeat(commands::MAX_LEN + 1);
        let mut bus = bus_with(move |event: &mut ChatSend, _| event.set_text(&long));
        assert_eq!(outgoing_chat(&mut bus, "hello".into()), None);
    }

    #[test]
    fn an_empty_line_is_never_sent() {
        assert_eq!(sendable(String::new()), None);
        assert_eq!(
            sendable("é".repeat(commands::MAX_LEN)).map(|m| m.len()),
            Some(2 * commands::MAX_LEN)
        );
    }

    #[test]
    fn each_tick_is_wrapped_by_a_pre_and_a_post_event_with_its_number() {
        use std::sync::{mpsc, Arc, Mutex};
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut bus = EventBus::default();
        let (pre, post) = (Arc::clone(&seen), Arc::clone(&seen));
        bus.on(ModuleId::new("t"), 0, move |e: &mut PreTick, _| {
            pre.lock().expect("lock").push(("pre", e.tick()));
        });
        bus.on(ModuleId::new("t"), 0, move |e: &mut PostTick, _| {
            post.lock().expect("lock").push(("post", e.tick()));
        });
        let mut game = Game::new();
        game.bus = bus;
        let mut queue = CommandQueue::default();
        let (frames, _frames_rx) = mpsc::sync_channel(64);
        let (chunks, _chunks_rx) = mpsc::sync_channel(64);
        for _ in 0..2 {
            run_tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
        }
        assert_eq!(
            *seen.lock().expect("lock"),
            [("pre", 0), ("post", 0), ("pre", 1), ("post", 1)]
        );
    }
}

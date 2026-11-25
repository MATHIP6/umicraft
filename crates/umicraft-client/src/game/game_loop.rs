// SPDX-License-Identifier: MIT OR Apache-2.0

//! The loop of the game thread: inputs first, then the network events of
//! the frame slot, then the due ticks (ADR-0005, phase E.9).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::time::{Duration, Instant};

use umicraft_assets::profile_textures::AccountSkinFetcher;
use umicraft_game::chat::commands;

use super::actions::handle_input;
use super::chat::{local_chat_line, push_chat};
use super::commands::CommandQueue;
use super::container_events::apply_container_action;
use super::creative_events::apply_creative_event;
use super::inventory_events::{apply_inventory_action, report_inventory_opened};
use super::messages::{GameEvent, RenderEvent};
use super::mod_events::{run_tick, typed_chat, GameMods};
use super::net::handle_net;
use super::publish::{publish_inventory, publish_look, publish_status};
use super::session::GameChannels;
use super::state::Game;
use super::TICK;
use crate::input::InputUpdate;
use crate::network::ClientCommand;
use crate::screens::connection_end::ConnectionEnd;

/// The game thread loop.
pub(super) fn run(
    channels: GameChannels,
    stop: Arc<AtomicBool>,
    own_name: String,
    account_skin: Option<AccountSkinFetcher>,
    mods: GameMods,
) {
    let GameChannels {
        inputs,
        events,
        frames,
        chunks,
        commands,
        frame_micros,
    } = channels;
    // Phase E.9: the next instant the network events are taken (see below).
    let mut next_drain = Instant::now();
    // Events that arrived while the frame slot was closed.
    let mut deferred: VecDeque<GameEvent> = VecDeque::new();
    let mut game = Game::new();
    game.own_name = own_name;
    game.e1 = crate::demos::e1::take();
    game.account_skin = account_skin;
    game.bus = mods.bus;
    game.commands = mods.commands;
    game.physics = mods.physics;
    game.entity_facts = mods.entity_facts;
    // The first event of the queue, which is still empty: the render thread
    // reads the mouse look from this slot, never from the queue.
    let _ = frames.try_send(RenderEvent::LookSlot(Arc::clone(&game.look)));
    let mut queue = CommandQueue {
        ordered: VecDeque::new(),
        sender: Some(commands),
    };
    let mut next_tick = Instant::now();
    let mut ended: Option<ConnectionEnd> = None;

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        // 1. Drain high-priority inputs first: user actions never wait for network packets.
        while let Ok(update) = inputs.try_recv() {
            take_input(&mut game, update, &mut queue);
        }

        // 2. Drain incoming network events BEFORE ticking, matching vanilla Minecraft's
        // `scheduledExecutables` section in `Minecraft.runGameLoop()` (which drains
        // `scheduledTasks` before running `runTick()`).
        // This ensures that server velocity (S12 knockback), position updates (S08),
        // block changes (S23/S22), entity spawns/metadata, etc. take effect in the
        // current simulation tick rather than lagging one tick behind.
        const MAX_EVENTS_PER_PASS: usize = 256;
        // Phase E.9: vanilla takes the network tasks once per iteration of
        // `runGameLoop`, that is once per displayed frame, so a packet it answers
        // (a teleport is answered at once, `handlePlayerPosLook`) cannot be
        // answered faster than the frame rate. The window reports the frame
        // duration; events wait for the next slot, and a slot takes every queued
        // event (up to the pass limit), like the scheduled-task queue.
        let period = Duration::from_micros(u64::from(frame_micros.load(Ordering::Relaxed)));
        let pacing = !period.is_zero();
        let mut events_processed = 0;
        if !pacing || Instant::now() >= next_drain {
            while events_processed < MAX_EVENTS_PER_PASS {
                let Some(event) = deferred.pop_front().or_else(|| events.try_recv().ok()) else {
                    break;
                };
                events_processed += 1;
                if handle_event(&mut game, event, &mut queue, &frames) {
                    ended = game.end.take();
                    break;
                }
            }
            if pacing && events_processed > 0 {
                next_drain = Instant::now() + period;
            }
        }
        if ended.is_some() {
            break;
        }
        queue.flush();

        game.metrics.net_queue_len = events.len();

        // 3. Process game ticking if a tick is already due, so network packet floods
        // cannot starve the 20 TPS simulation loop.
        let mut now = Instant::now();
        if now >= next_tick {
            let due = ticks_due(now, next_tick);
            let mut catches = 0;
            // A congested queue stops the ticks (see `PENDING_LIMIT`); the
            // clamp below then waits one tick length before trying again.
            while catches < due && !queue.congested() {
                game.metrics
                    .record_tick_delay(now.saturating_duration_since(next_tick), catches > 0);
                run_tick(&mut game, &mut queue, &frames, &chunks, next_tick);
                next_tick += TICK;
                catches += 1;
            }
            if next_tick <= now {
                // The thread fell too far behind (more than
                // `MAX_CATCH_UP_TICKS` missed): the rest of the time is
                // dropped, as vanilla's timer drops it.
                next_tick = now + TICK;
            }
            queue.flush();
        }

        now = Instant::now();
        if now >= next_tick {
            continue;
        }
        if events_processed == MAX_EVENTS_PER_PASS && !events.is_empty() {
            continue;
        }

        let timeout = next_tick.saturating_duration_since(now);
        // Events are queued but the frame slot is not open yet: wait for the
        // slot (or an input, or the tick) without taking them.
        if pacing && (!events.is_empty() || !deferred.is_empty()) && now < next_drain {
            let wait = timeout.min(next_drain.saturating_duration_since(now));
            if let Ok(update) = inputs.recv_timeout(wait) {
                take_input(&mut game, update, &mut queue);
            }
            continue;
        }
        crossbeam_channel::select! {
            recv(inputs) -> msg => {
                if let Ok(update) = msg {
                    take_input(&mut game, update, &mut queue);
                }
            }
            recv(events) -> msg => {
                match msg {
                    Ok(event) if pacing && Instant::now() < next_drain => {
                        // The slot is closed: this event waits for the next one.
                        deferred.push_back(event);
                    }
                    Ok(event) => {
                        if handle_event(&mut game, event, &mut queue, &frames) {
                            ended = game.end.take();
                        }
                        if pacing {
                            next_drain = Instant::now() + period;
                        }
                    }
                    Err(_) => break,
                }
            }
            default(timeout) => {}
        }
    }

    if let Some(reason) = ended {
        let _ = frames.send(RenderEvent::Ended(reason));
    }
}

/// The most ticks one pass runs after a stall. MCP `Timer.updateTimer`
/// caps `elapsedTicks` at 10, so after a hitch the vanilla client runs up to
/// ten ticks (and sends ten movement packets) at once and drops the time
/// beyond; fewer ticks leave the server short of packets for that time.
pub(super) const MAX_CATCH_UP_TICKS: u32 = 10;

/// How many ticks are due at `now` when the next one was due at
/// `next_tick`, at most [`MAX_CATCH_UP_TICKS`].
pub(super) fn ticks_due(now: Instant, next_tick: Instant) -> u32 {
    if now < next_tick {
        return 0;
    }
    let late = now.duration_since(next_tick).as_nanos() / TICK.as_nanos();
    u32::try_from(late + 1).map_or(MAX_CATCH_UP_TICKS, |due| due.min(MAX_CATCH_UP_TICKS))
}

/// Handles one input update, timing its wait and its handling for the F3
/// screen.
///
/// When the view turned, it is published right away, so the mouse look does
/// not wait for the next tick.
fn take_input(game: &mut Game, update: InputUpdate, queue: &mut CommandQueue) {
    game.metrics
        .record_input_latency(update.created_at, Instant::now());
    let start = Instant::now();
    let before = (game.yaw, game.pitch);
    handle_input(game, update, queue);
    game.metrics.record_input_processing(start.elapsed());
    if (game.yaw, game.pitch) != before {
        publish_look(game);
    }
}

/// Handles one message of the inbox.
///
/// Returns `true` when the session must end.
pub(super) fn handle_event(
    game: &mut Game,
    event: GameEvent,
    queue: &mut CommandQueue,
    frames: &SyncSender<RenderEvent>,
) -> bool {
    match event {
        GameEvent::Net(event) => {
            let start = Instant::now();
            handle_net(game, event, queue, frames);
            game.metrics.record_net_event(start.elapsed());
            false
        }
        GameEvent::Disconnected(end) => {
            tracing::error!(%end, "network session ended");
            game.status = format!("ERROR {end}");
            game.status_data.status = game.status.clone();
            game.end = Some(end);
            publish_status(game, frames);
            true
        }
        GameEvent::ConnectStage(stage) => {
            let _ = frames.try_send(RenderEvent::Connect(stage));
            false
        }
        GameEvent::Input(update) => {
            take_input(game, update, queue);
            false
        }
        GameEvent::Chat(message) => {
            if let Some(message) = typed_chat(game, message) {
                queue.push(ClientCommand::Chat { message });
            }
            false
        }
        GameEvent::TabComplete(message) => {
            if message.chars().count() <= commands::MAX_LEN {
                queue.push(ClientCommand::TabComplete { message });
            }
            false
        }
        GameEvent::ServerPackStatus { hash, result } => {
            queue.push(ClientCommand::ResourcePackStatus { hash, result });
            false
        }
        GameEvent::ClientSettings {
            view_distance,
            chat_mode,
            chat_colors,
            skin_parts,
        } => {
            game.chat.mode = chat_mode;
            queue.push(ClientCommand::ClientSettings {
                view_distance,
                chat_mode,
                chat_colors,
                skin_parts,
            });
            false
        }
        GameEvent::ParticleSetting(index) => {
            game.particle_setting = umicraft_game::particles::ParticleSetting::from_index(index);
            false
        }
        GameEvent::LegacyAnimations(legacy) => {
            game.legacy = legacy;
            game.hand.set_legacy(legacy);
            false
        }
        GameEvent::DetachedCamera(detached) => {
            game.camera_detached = detached;
            false
        }
        GameEvent::LocalChat(text) => {
            let line = local_chat_line(&text);
            push_chat(game, line);
            false
        }
        GameEvent::ClearChat => {
            game.chat.clear();
            game.hud_dirty = true;
            false
        }
        GameEvent::ReMeshAll => {
            if let Some(world) = game.world.as_ref() {
                for (pos, _) in world.chunks() {
                    game.dirty.insert(pos);
                }
            }
            false
        }
        GameEvent::CloseInventory => {
            let window_id = game.windows.container().map_or(0_i8, |c| c.window_id as i8);
            game.windows.close_window(window_id as u8);
            queue.push(ClientCommand::CloseWindow { window_id });
            publish_inventory(game, frames);
            false
        }
        GameEvent::Inventory(action) => {
            apply_inventory_action(game, action, queue, frames);
            false
        }
        GameEvent::SignTyped { position, lines } => {
            game.server.block_entities_mut().set_sign(position, lines);
            false
        }
        GameEvent::SignEdited { position, lines } => {
            game.server
                .block_entities_mut()
                .set_sign(position, lines.clone());
            queue.push(ClientCommand::UpdateSign { position, lines });
            false
        }
        GameEvent::BookEdited(packet) => {
            queue.push(ClientCommand::PluginMessage {
                channel: packet.channel,
                data: packet.data,
            });
            false
        }
        GameEvent::Container(action) => {
            apply_container_action(game, action, queue, frames);
            false
        }
        GameEvent::Creative(event) => {
            apply_creative_event(game, event, queue, frames);
            false
        }
        GameEvent::RequestStats => {
            // MCP `GuiStats` / `GuiIngameMenu` button 6 send `Client Status`
            // (REQUEST_STATS); the answer is `Statistics` (0x37).
            queue.push(ClientCommand::RequestStats);
            false
        }
        GameEvent::InventoryOpened => {
            report_inventory_opened(game, queue);
            false
        }
        GameEvent::Respawn => {
            // MCP `GuiGameOver.actionPerformed`: `respawnPlayer` sends
            // `Client Status` (PERFORM_RESPAWN); the server answers with
            // `Respawn` and a new health.
            if game.dead() {
                queue.push(ClientCommand::Respawn);
            }
            false
        }
    }
}

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
use super::inventory_events::apply_inventory_action;
use super::messages::{GameEvent, RenderEvent};
use super::net::handle_net;
use super::publish::{publish_inventory, publish_look, publish_status};
use super::session::GameChannels;
use super::state::Game;
use super::tick::tick;
use super::TICK;
use crate::connection_end::ConnectionEnd;
use crate::network::ClientCommand;

/// The game thread loop.
pub(super) fn run(
    channels: GameChannels,
    stop: Arc<AtomicBool>,
    own_name: String,
    account_skin: Option<AccountSkinFetcher>,
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
    game.e1 = crate::e1::take();
    game.account_skin = account_skin;
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
            let queue_latency = Instant::now().saturating_duration_since(update.created_at);
            game.metrics.last_input_queue_latency_us =
                queue_latency.as_micros().min(u128::from(u32::MAX)) as u32;
            let start = Instant::now();
            let before = (game.yaw, game.pitch);
            handle_input(&mut game, update, &mut queue);
            let elapsed = start.elapsed();
            game.metrics.last_input_processing_us =
                elapsed.as_micros().min(u128::from(u32::MAX)) as u32;
            if (game.yaw, game.pitch) != before {
                publish_look(&game);
            }
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
            const MAX_CATCH_UP_TICKS: u32 = 3;
            let mut catches = 0;
            // A congested queue stops the ticks (see `PENDING_LIMIT`); the
            // clamp below then waits one tick length before trying again.
            while now >= next_tick && catches < MAX_CATCH_UP_TICKS && !queue.congested() {
                let delay = now.saturating_duration_since(next_tick);
                let delay_us = delay.as_micros().min(u128::from(u32::MAX)) as u32;
                if delay_us > game.metrics.max_tick_delay_us {
                    game.metrics.max_tick_delay_us = delay_us;
                }
                game.metrics.last_tick_delay_us = delay_us;
                if catches > 0 {
                    game.metrics.catch_up_ticks_total += 1;
                }
                tick(&mut game, &mut queue, &frames, &chunks, next_tick);
                next_tick += TICK;
                catches += 1;
            }
            if next_tick <= now {
                // The thread fell too far behind (more than 3 ticks missed);
                // clamp next_tick to prevent a spiral of death.
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
                let queue_latency = Instant::now().saturating_duration_since(update.created_at);
                game.metrics.last_input_queue_latency_us =
                    queue_latency.as_micros().min(u128::from(u32::MAX)) as u32;
                let before = (game.yaw, game.pitch);
                handle_input(&mut game, update, &mut queue);
                if (game.yaw, game.pitch) != before {
                    publish_look(&game);
                }
            }
            continue;
        }
        crossbeam_channel::select! {
            recv(inputs) -> msg => {
                if let Ok(update) = msg {
                    let queue_latency = Instant::now().saturating_duration_since(update.created_at);
                    game.metrics.last_input_queue_latency_us =
                        queue_latency.as_micros().min(u128::from(u32::MAX)) as u32;
                    let start = Instant::now();
                    let before = (game.yaw, game.pitch);
                    handle_input(&mut game, update, &mut queue);
                    let elapsed = start.elapsed();
                    game.metrics.last_input_processing_us =
                        elapsed.as_micros().min(u128::from(u32::MAX)) as u32;
                    if (game.yaw, game.pitch) != before {
                        publish_look(&game);
                    }
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
            let elapsed = start.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;
            game.metrics.net_micros_accum = game.metrics.net_micros_accum.saturating_add(elapsed);
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
            let queue_latency = Instant::now().saturating_duration_since(update.created_at);
            game.metrics.last_input_queue_latency_us =
                queue_latency.as_micros().min(u128::from(u32::MAX)) as u32;
            let start = Instant::now();
            let before = (game.yaw, game.pitch);
            handle_input(game, update, queue);
            let elapsed = start.elapsed();
            game.metrics.last_input_processing_us =
                elapsed.as_micros().min(u128::from(u32::MAX)) as u32;
            if (game.yaw, game.pitch) != before {
                // The view changed between two ticks: publish it right away,
                // so the mouse look does not wait for the next tick.
                publish_look(game);
            }
            false
        }
        GameEvent::Chat(message) => {
            // The server limit counts characters, not bytes.
            if !message.is_empty() && message.chars().count() <= commands::MAX_LEN {
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
        GameEvent::LocalChat(text) => {
            let line = local_chat_line(&text);
            push_chat(game, line);
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
        GameEvent::SignEdited { position, lines } => {
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

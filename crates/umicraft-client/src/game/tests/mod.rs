// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the game thread.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::time::{Duration, Instant};

use glam::{DVec3, Vec3};
use umicraft_assets::profile_textures::ProfileTextures;
use umicraft_game::entity::{EntityFlags, EntityId};
use umicraft_game::gamemode::GameMode;
use umicraft_game::input::RawInput;
use umicraft_game::item_use::{DropKind, UseAction};
use umicraft_game::raycast::look_direction;
use umicraft_game::view::Perspective;
use umicraft_game::{GameState, InputIntent, RenderState};
use umicraft_net::play::PlayEvent;
use umicraft_protocol::packets::play::clientbound::player_list_item::PlayerListItem;
use umicraft_protocol::packets::play::clientbound::{BlockBreakAnim, ChunkData, Explosion};
use umicraft_protocol::packets::play::serverbound::DiggingStatus;
use umicraft_protocol::packets::play::serverbound::UseEntityAction;
use umicraft_protocol::packets::play::ItemStack as WireItemStack;
use umicraft_protocol::packets::play::Metadata;
use umicraft_protocol::PlayServerbound;
use umicraft_render::plain_text;
use umicraft_render::{ChatLineView, OutlineBox};
use umicraft_world::{BlockPos, BlockState, ChunkPos, World};

use super::abilities::*;
use super::actions::attack::*;
use super::actions::hotbar::*;
use super::actions::use_item::*;
use super::actions::*;
use super::body::*;
use super::combat;
use super::commands::*;
use super::game_loop::*;
use super::hud::*;
use super::look::*;
use super::messages::*;
use super::movement::*;
use super::net::inventory::*;
use super::net::world::*;
use super::net::*;
use super::particles::*;
use super::publish::*;
use super::session::*;
use super::state::chat::CHAT_HISTORY;
use super::state::vitals::DEATH_TICKS;
use super::state::*;
use super::tick::*;
use super::view::*;
use crate::input::InputState;
use crate::input::InputUpdate;
use crate::network::{self, ClientCommand};
use crate::social_view;

mod abilities;
mod actions;
mod chat;
mod command_queue;
mod digging;
mod entities;
mod event_order;
mod game_loop;
mod hotbar;
mod hud;
mod join;
mod look;
mod movement;
mod particles;
mod view;
mod vitals;

/// An `ench` list with one enchantment, as a server would send it.
fn enchanted_nbt(id: i16, level: i16) -> Vec<u8> {
    let mut nbt = vec![10, 0, 0, 9, 0, 4];
    nbt.extend(b"ench");
    nbt.extend([10, 0, 0, 0, 1]);
    nbt.extend([2, 0, 2]);
    nbt.extend(b"id");
    nbt.extend(id.to_be_bytes());
    nbt.extend([2, 0, 3]);
    nbt.extend(b"lvl");
    nbt.extend(level.to_be_bytes());
    nbt.extend([0, 0]);
    nbt
}

/// A placed, playing game on the floor of a loaded chunk, for the
/// traffic tests.
pub(super) fn placed_game() -> Game {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.playing = true;
    game
}

fn digging_commands(queue: &CommandQueue) -> usize {
    queue
        .ordered
        .iter()
        .filter(|command| matches!(command, ClientCommand::PlayerDigging { .. }))
        .count()
}

fn health_update(health: f32) -> PlayEvent {
    PlayEvent::UpdateHealth(
        umicraft_protocol::packets::play::clientbound::UpdateHealth {
            health,
            food: 20,
            saturation: 5.0,
        },
    )
}

/// A grass floor (top at y = 64) with one block of `plant` standing on
/// it at (0, 64, 0), and a player whose feet are at y = 64 there.
fn game_on_floor_with(plant: BlockState) -> Game {
    let mut world = World::new(0);
    let payload = vec![0u8; umicraft_world::chunk::payload_len(0b1000, true, true)];
    world
        .apply_chunk(
            ChunkPos::new(0, 0),
            umicraft_world::ChunkPayload {
                primary_bit_mask: 0b1000,
                ground_up: true,
                data: &payload,
            },
        )
        .expect("an empty column");
    for x in 0..16 {
        for z in 0..16 {
            world.set_block(BlockPos::new(x, 63, z), BlockState::new(2, 0));
        }
    }
    world.set_block(BlockPos::new(0, 64, 0), plant);
    let mut game = Game::new();
    game.world = Some(world);
    game.state = Some(GameState::new(DVec3::new(0.5, 64.0, 0.5)));
    game
}

/// Presses the use button once and runs the tick of the button.
fn click_use(game: &mut Game, queue: &mut CommandQueue) {
    game.controls.use_pressed = true;
    run_use(game, queue);
}

fn wire_item(item_id: i16, damage: i16) -> WireItemStack {
    WireItemStack {
        item_id,
        stack_size: 1,
        damage,
        nbt: None,
    }
}

fn confirmation(window_id: u8, action_number: i16) -> PlayEvent {
    PlayEvent::ConfirmTransaction(
        umicraft_protocol::packets::play::clientbound::ConfirmTransaction {
            window_id,
            action_number,
            accepted: false,
        },
    )
}

fn commands(queue: &CommandQueue) -> Vec<&ClientCommand> {
    queue.ordered.iter().collect()
}

/// Puts a zombie or a player two blocks ahead of the player (yaw 0 looks
/// along +z) and returns its id.
fn spawn_ahead(game: &mut Game, player: bool) -> EntityId {
    let id = EntityId(7);
    let position = DVec3::new(0.5, 64.0, 2.5);
    if player {
        game.entities.spawn_player(
            id,
            [1; 16],
            position,
            0.0,
            0.0,
            None,
            EntityFlags::default(),
        );
    } else {
        game.entities.spawn_mob(
            id,
            54,
            position,
            0.0,
            0.0,
            0.0,
            Vec3::ZERO,
            EntityFlags::default(),
        );
    }
    id
}

fn hurt_status(entity_id: i32, status: i8) -> PlayEvent {
    PlayEvent::EntityStatus(
        umicraft_protocol::packets::play::clientbound::EntityStatus { entity_id, status },
    )
}

/// Runs `count` ticks without a flush and returns the feet Z after each.
fn run_ticks(game: &mut Game, queue: &mut CommandQueue, count: usize) -> Vec<f64> {
    let (frames, _frames_rx) = mpsc::sync_channel(64);
    let (chunks, _chunks_rx) = mpsc::sync_channel(64);
    (0..count)
        .map(|_| {
            tick(game, queue, &frames, &chunks, Instant::now());
            game.state.as_ref().expect("state").position().z
        })
        .collect()
}

/// A game that walks forward (+z) and has already reached its pace.
fn walking_game() -> Game {
    let mut game = placed_game();
    game.controls.input.forward = 1.0;
    run_ticks(&mut game, &mut CommandQueue::default(), 6);
    game
}

fn movement_of(queue: &CommandQueue) -> Vec<&ClientCommand> {
    queue.ordered.iter().filter(|c| is_movement(c)).collect()
}

// The congestion guard in the real game loop (`run`, on its own thread).

/// Slot changes that fill the channel to the socket thread and then the
/// queue up to its limit while nothing is read.
const CONGESTING: usize = network::COMMAND_CAPACITY + PENDING_LIMIT + 22;

/// The real game loop with channels the test holds. The commands channel
/// is only drained when the test reads it, which is how a
/// socket that does not take packets looks like to the game thread.
/// Every `RenderEvent::Frame` is one executed tick.
struct LoopHarness {
    inputs: crossbeam_channel::Sender<InputUpdate>,
    events: crossbeam_channel::Sender<GameEvent>,
    frames: mpsc::Receiver<RenderEvent>,
    _chunks: mpsc::Receiver<ChunkEvent>,
    commands: tokio::sync::mpsc::Receiver<PlayServerbound>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    spam: u32,
    /// Frame duration the game thread paces its events with (0 = off).
    frame_micros: std::sync::Arc<AtomicU32>,
}

impl LoopHarness {
    fn start(capacity: usize) -> Self {
        let frame_micros = std::sync::Arc::new(AtomicU32::new(0));
        let (inputs_tx, inputs) = crossbeam_channel::unbounded();
        let (events_tx, events) = crossbeam_channel::unbounded();
        let (frames_tx, frames) = mpsc::sync_channel(100_000);
        let (chunks_tx, chunks) = mpsc::sync_channel(100_000);
        let (commands_tx, commands) = tokio::sync::mpsc::channel(capacity);
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let channels = GameChannels {
            inputs,
            events,
            frames: frames_tx,
            chunks: chunks_tx,
            commands: commands_tx,
            frame_micros: std::sync::Arc::clone(&frame_micros),
        };
        let flag = std::sync::Arc::clone(&stop);
        let thread = std::thread::spawn(move || run(channels, flag, "tester".to_string(), None));
        Self {
            inputs: inputs_tx,
            events: events_tx,
            frames,
            _chunks: chunks,
            commands,
            stop,
            thread: Some(thread),
            spam: 0,
            frame_micros,
        }
    }

    /// Joins a world of empty air and places the player high above it,
    /// so it falls (and reports a new position every tick) for a long time.
    fn join_world(&self) {
        let send = |event| {
            self.events
                .send(GameEvent::Net(event))
                .expect("the loop runs")
        };
        send(PlayEvent::JoinGame(umicraft_net::play::JoinGameInfo {
            entity_id: 1,
            gamemode: 0,
            dimension: 0,
            difficulty: 0,
            max_players: 20,
            level_type: "flat".to_string(),
            reduced_debug_info: false,
        }));
        send(PlayEvent::ChunkData(Box::new(ChunkData {
            chunk_x: 0,
            chunk_z: 0,
            ground_up: true,
            primary_bit_mask: 1,
            data: vec![0u8; umicraft_world::chunk::payload_len(1, true, true)],
        })));
        send(PlayEvent::Placement(umicraft_net::play::PlayerPlacement {
            x: 0.5,
            y: 1000.0,
            z: 0.5,
            ..Default::default()
        }));
    }

    /// The ticks executed since the last call.
    fn ticks(&self) -> usize {
        let mut count = 0;
        while let Ok(event) = self.frames.try_recv() {
            if matches!(event, RenderEvent::Frame(_)) {
                count += 1;
            }
        }
        count
    }

    fn wait_for_ticks(&self, wanted: usize) -> usize {
        let mut seen = 0;
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        while seen < wanted && Instant::now() < deadline {
            seen += self.ticks();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(seen >= wanted, "only {seen} ticks ran");
        seen
    }

    /// Chat lines, each one a command in the queue. (Slot changes cannot
    /// fill it any more: the presses between two ticks make one packet.)
    fn spam_chats(&mut self, count: usize) -> Vec<String> {
        (0..count)
            .map(|_| {
                let message = format!("m{}", self.spam);
                self.spam += 1;
                self.events
                    .send(GameEvent::Chat(message.clone()))
                    .expect("the loop runs");
                message
            })
            .collect()
    }

    /// Reads what reached the socket side until `window` has passed.
    fn read_for(&mut self, window: std::time::Duration) -> Vec<PlayServerbound> {
        let mut packets = Vec::new();
        let end = Instant::now() + window;
        while Instant::now() < end {
            while let Ok(packet) = self.commands.try_recv() {
                packets.push(packet);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        packets
    }

    fn finish(&mut self) -> Vec<PlayServerbound> {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        self.thread
            .take()
            .expect("running")
            .join()
            .expect("the loop ends");
        self.read_for(std::time::Duration::from_millis(20))
    }
}

fn chats_of(packets: &[PlayServerbound]) -> Vec<String> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            PlayServerbound::Chat(chat) => Some(chat.message.clone()),
            _ => None,
        })
        .collect()
}

fn slots_of(packets: &[PlayServerbound]) -> Vec<i16> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            PlayServerbound::HeldItemChange(change) => Some(change.slot),
            _ => None,
        })
        .collect()
}

/// The feet Y of the falling player in each position report, without the
/// acknowledgement of the placement (the exact Y of the placement).
fn falling_ys(packets: &[PlayServerbound]) -> Vec<f64> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            PlayServerbound::PlayerPosition(p) => Some(p.y),
            PlayServerbound::PositionLook(p) => Some(p.y),
            _ => None,
        })
        .filter(|&y| y != 1000.0)
        .collect()
}

/// A lost report of a falling player doubles one step; the steps of a
/// fall grow by less than 30 % after the first ten ticks.
fn assert_fall_has_no_gap(ys: &[f64]) {
    assert!(
        ys.windows(2).all(|w| w[1] < w[0]),
        "reports out of order or twice"
    );
    let steps: Vec<f64> = ys.windows(2).map(|w| w[0] - w[1]).collect();
    for (i, pair) in steps.windows(2).enumerate().skip(8) {
        assert!(
            pair[1] / pair[0] < 1.3,
            "a report is missing near #{i}: {pair:?}"
        );
    }
}

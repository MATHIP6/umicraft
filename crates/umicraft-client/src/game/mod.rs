// SPDX-License-Identifier: MIT OR Apache-2.0

//! The game thread: the owner of the world, of the player state and of the
//! fixed 20 Hz tick (ADR-0005).
//!
//! The thread owns the two pieces of mutable state of a playing client: the
//! [`umicraft_world::World`] and the [`umicraft_game::GameState`]. Nothing
//! else touches them, so the collision path never takes a lock (spec §5.3).
//!
//! It receives two kinds of messages on one bounded inbox:
//!
//! * the decoded clientbound packets of the network thread, and
//! * the input updates of an input source (the window or the headless script).
//!
//! It answers with two bounded channels: the *frames* (camera, HUD, status)
//! and the *chunk snapshots* the mesher needs. Both are polled by the render
//! thread; the game thread never blocks on them, and a snapshot that does not
//! fit into the queue stays in the dirty set and is derived again later.
//!
//! # Layout
//!
//! * Thread and channels: `session` starts the threads, `game_loop` runs
//!   the loop, `messages` holds what crosses the threads and `look` the
//!   lock-free mouse look slot.
//! * State: `state` is the `Game` the thread owns, with its field groups.
//! * Inbound: `net` applies the clientbound packets; `inventory_events`,
//!   `container_events` and `creative_events` apply the window's screen events.
//! * The tick: `tick` runs one simulation step in the order of
//!   `Minecraft.runTick`, with `actions` (attack, use, hotbar), `abilities`,
//!   `movement`, `body`, `particles`, `combat` and `hud`.
//! * Outbound: `commands` queues the serverbound packets in order, `publish`
//!   and `view` build what the render thread draws, `chat` keeps the history.

use std::time::Duration;

mod abilities;
mod actions;
mod body;
mod chat;
mod combat;
mod commands;
mod container_events;
mod creative_events;
mod game_loop;
mod hud;
mod inventory_events;
mod look;
mod messages;
mod movement;
mod net;
mod particles;
mod publish;
mod session;
mod state;
#[cfg(test)]
mod tests;
mod tick;
mod view;

pub(crate) use hud::hud_after;
pub use look::{camera_angles, AtomicLookSlot, Look, LookSlot};
pub use messages::{
    ChunkEvent, Frame, GameEvent, GameStatus, RenderEvent, TextScreenRequest, ViewAngle,
};
pub use session::Session;

/// The tick period of the simulation: 20 Hz, as the 1.8 server (spec GAM-1).
pub const TICK: Duration = Duration::from_millis(50);

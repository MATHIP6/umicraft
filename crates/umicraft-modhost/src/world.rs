// SPDX-License-Identifier: MIT OR Apache-2.0

//! Read-only world queries for mods (F3.6, ADR-0086).
//!
//! The host does not own the world; the client does. It lends a [`WorldQuery`] to the
//! current thread with [`with_world`] around the code that runs mod callbacks, and
//! the `world_block` / `world_entities` host functions answer from it.
//!
//! The scope is a thread-local on purpose and only the game thread opens it: the world
//! is owned by the game thread and changes every tick, so a query from a worker or the
//! render thread would need a snapshot, which v1 does not build (the cost would be paid
//! every tick for a feature few mods use). Those threads get `UNSUPPORTED` instead.

use crate::ffi;

/// What a mod can ask about the world, answered by the client.
pub trait WorldQuery {
    /// The block at `(x, y, z)` as the wire state `id << 4 | metadata`, or `None`
    /// when its chunk is not loaded.
    fn block(&self, x: i32, y: i32, z: i32) -> Option<u16>;

    /// Calls `visit` for every entity but the local player until it returns `false`.
    fn visit_entities(&self, visit: &mut dyn FnMut(&EntityView<'_>) -> bool);
}

/// One entity as a mod sees it; the strings are only valid during the visit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntityView<'a> {
    /// The server's entity id.
    pub id: i32,
    /// Stable snake_case kind (`player`, `zombie`, ...).
    pub kind: &'a str,
    /// Position, in blocks.
    pub position: [f64; 3],
    /// View angle in degrees.
    pub yaw: f32,
    /// View angle in degrees.
    pub pitch: f32,
    /// A player's name or the name tag, empty when there is none.
    pub name: &'a str,
}

/// Lends `world` to the current thread while `f` runs, so mod callbacks made inside
/// `f` can query it.
///
/// The previous scope, if any, is restored when `f` returns or unwinds; a nested
/// scope hides the outer one meanwhile.
pub fn with_world<R>(world: &dyn WorldQuery, f: impl FnOnce() -> R) -> R {
    ffi::lend_world(world, f)
}

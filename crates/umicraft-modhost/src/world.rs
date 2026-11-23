// SPDX-License-Identifier: MIT OR Apache-2.0

//! Read-only world queries for mods (F3.6, ADR-0086).
//!
//! The host does not own the world; the client does. It lends a [`WorldQuery`] to the
//! current thread with [`with_world`] around the code that runs mod callbacks, and
//! the `world_block` / `world_entities` host functions answer from it.
//!
//! The scope is a thread-local on purpose and only the game thread opens it: the world
//! is owned by the game thread and changes every tick, so a query from a worker or the
//! render thread would need a snapshot. Those threads get `UNSUPPORTED` from
//! `world_block` and `world_entities`; the entities have their own render-thread
//! snapshot ([`EntitySnapshot`](crate::EntitySnapshot), API 1.1, ADR-0094).

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
    /// Health points; `0.0` unless [`UMI_ENTITY_LIVING`](umicraft_modapi_sys::UMI_ENTITY_LIVING)
    /// is set in `flags`.
    pub health: f32,
    /// Maximum health points; `0.0` unless the entity is living.
    pub max_health: f32,
    /// Armour points of the equipment the client sees.
    pub armor: f32,
    /// Scoreboard team name, empty when the entity is on none.
    pub team: &'a str,
    /// Width of the bounding box, in blocks.
    pub width: f32,
    /// Height of the bounding box, in blocks.
    pub height: f32,
    /// Position change over the last tick, in blocks per tick.
    pub velocity: [f64; 3],
    /// Bit set of the `UMI_ENTITY_*` flags.
    pub flags: u32,
    /// Whether the entity is a player.
    pub is_player: bool,
    /// Where the frame being drawn shows the entity; equal to `position` in a
    /// game-thread query.
    pub render_position: [f64; 3],
}

impl EntityView<'_> {
    /// The same facts with other strings, so a view can be kept without the text it
    /// was made with, or lent again with text of another lifetime.
    #[must_use]
    pub fn with_text<'b>(&self, kind: &'b str, name: &'b str, team: &'b str) -> EntityView<'b> {
        EntityView {
            id: self.id,
            kind,
            position: self.position,
            yaw: self.yaw,
            pitch: self.pitch,
            name,
            health: self.health,
            max_health: self.max_health,
            armor: self.armor,
            team,
            width: self.width,
            height: self.height,
            velocity: self.velocity,
            flags: self.flags,
            is_player: self.is_player,
            render_position: self.render_position,
        }
    }
}

/// Lends `world` to the current thread while `f` runs, so mod callbacks made inside
/// `f` can query it.
///
/// The previous scope, if any, is restored when `f` returns or unwinds; a nested
/// scope hides the outer one meanwhile.
pub fn with_world<R>(world: &dyn WorldQuery, f: impl FnOnce() -> R) -> R {
    ffi::lend_world(world, f)
}

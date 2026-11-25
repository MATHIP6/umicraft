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

/// Most potion effects an [`EntityView`] carries (API 1.4).
pub const MAX_ENTITY_EFFECTS: usize = umicraft_modapi_sys::UMI_MAX_ENTITY_EFFECTS;

/// One potion effect of an entity (API 1.4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Effect {
    /// 1.8 potion id.
    pub id: u8,
    /// Level minus one.
    pub amplifier: u8,
    /// Whether the server hides its particles.
    pub hide_particles: bool,
    /// Ticks left as the server sent them.
    pub duration: i32,
}

/// The potion effects of an entity, at most [`MAX_ENTITY_EFFECTS`], kept inline so
/// a view stays `Copy`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EntityEffects {
    list: [Effect; MAX_ENTITY_EFFECTS],
    len: u8,
}

impl EntityEffects {
    /// Adds `effect`; returns `false`, adding nothing, once full.
    pub fn push(&mut self, effect: Effect) -> bool {
        let Some(slot) = self.list.get_mut(usize::from(self.len)) else {
            return false;
        };
        *slot = effect;
        self.len += 1;
        true
    }

    /// The effects pushed so far.
    #[must_use]
    pub fn as_slice(&self) -> &[Effect] {
        &self.list[..usize::from(self.len)]
    }
}

impl FromIterator<Effect> for EntityEffects {
    /// The first [`MAX_ENTITY_EFFECTS`] effects of `iter`.
    fn from_iter<I: IntoIterator<Item = Effect>>(iter: I) -> Self {
        let mut effects = Self::default();
        for effect in iter {
            if !effects.push(effect) {
                break;
            }
        }
        effects
    }
}

/// The item an entity holds (API 1.4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HeldItem {
    /// Numeric item id.
    pub id: i32,
    /// Damage or metadata value.
    pub damage: i32,
}

/// The strings of an [`EntityView`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EntityTexts<'a> {
    /// See [`EntityView::kind`].
    pub kind: &'a str,
    /// See [`EntityView::name`].
    pub name: &'a str,
    /// See [`EntityView::team`].
    pub team: &'a str,
    /// See [`EntityView::display_name`].
    pub display_name: &'a str,
}

/// One entity as a mod sees it; the strings are only valid during the visit.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
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
    /// Ticks left of the hurt flash (API 1.4).
    pub hurt_time: u32,
    /// Ticks into the death fall, `0` while alive (API 1.4).
    pub death_time: u32,
    /// The item in the main hand (API 1.4).
    pub held_item: Option<HeldItem>,
    /// Team prefix, name and suffix of a player, else the name tag (API 1.4).
    pub display_name: &'a str,
    /// Team colour as `0xRRGGBB` (API 1.4).
    pub team_color: Option<u32>,
    /// Tab-list latency in milliseconds, `None` when not listed (API 1.4).
    pub ping: Option<i32>,
    /// Version nibble of the UUID, `0` when unknown (API 1.4).
    pub uuid_version: u8,
    /// Potion effects (API 1.4).
    pub effects: EntityEffects,
}

impl<'a> EntityView<'a> {
    /// The strings of the view.
    #[must_use]
    pub fn texts(&self) -> EntityTexts<'a> {
        EntityTexts {
            kind: self.kind,
            name: self.name,
            team: self.team,
            display_name: self.display_name,
        }
    }

    /// The same facts with other strings, so a view can be kept without the text it
    /// was made with, or lent again with text of another lifetime.
    #[must_use]
    pub fn with_text<'b>(&self, texts: EntityTexts<'b>) -> EntityView<'b> {
        EntityView {
            id: self.id,
            kind: texts.kind,
            position: self.position,
            yaw: self.yaw,
            pitch: self.pitch,
            name: texts.name,
            health: self.health,
            max_health: self.max_health,
            armor: self.armor,
            team: texts.team,
            width: self.width,
            height: self.height,
            velocity: self.velocity,
            flags: self.flags,
            is_player: self.is_player,
            render_position: self.render_position,
            hurt_time: self.hurt_time,
            death_time: self.death_time,
            held_item: self.held_item,
            display_name: texts.display_name,
            team_color: self.team_color,
            ping: self.ping,
            uuid_version: self.uuid_version,
            effects: self.effects,
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

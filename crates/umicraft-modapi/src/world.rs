// SPDX-License-Identifier: MIT OR Apache-2.0

//! Read-only questions about the world.

use core::ffi::c_void;
use std::cell::Cell;
use std::marker::PhantomData;

use umicraft_modapi_sys::{
    UmiEffect, UmiEntity, UmiStatus, UmiStr, UMI_EFFECT_HIDE_PARTICLES, UMI_ENTITY_INVISIBLE,
    UMI_ENTITY_IN_TAB_LIST, UMI_ENTITY_LIVING, UMI_ENTITY_ON_FIRE, UMI_ENTITY_SNEAKING,
    UMI_ENTITY_SPECTATOR, UMI_ENTITY_SPRINTING, UMI_ENTITY_TEAMMATE, UMI_ENTITY_USING_ITEM,
    UMI_ENTITY_VISIBLE, UMI_MAX_ENTITY_EFFECTS, UMI_NO_COLOR,
};

use crate::ffi::{entity_trampoline, str_of, Visit};
use crate::{host, ModError};

/// A block as the server sends it: an id and its metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    /// The block id (0 is air).
    pub id: u16,
    /// The metadata nibble, `0..=15`.
    pub meta: u8,
}

impl Block {
    /// Splits the wire state `id << 4 | meta`.
    fn from_state(state: u32) -> Self {
        Self {
            id: u16::try_from(state >> 4).unwrap_or(u16::MAX),
            meta: u8::try_from(state & 0xF).unwrap_or(0),
        }
    }
}

/// One entity of the world, lent to an [`World::entities`] visitor.
#[derive(Debug, Clone, Copy)]
pub struct Entity<'a> {
    raw: UmiEntity,
    _call: PhantomData<&'a ()>,
}

impl Entity<'_> {
    pub(crate) fn new(raw: UmiEntity) -> Self {
        Self {
            raw,
            _call: PhantomData,
        }
    }

    /// The server's entity id.
    pub fn id(&self) -> i32 {
        self.raw.id
    }

    /// Stable snake_case kind: `player`, `zombie`, `item`, ...
    pub fn kind(&self) -> &str {
        str_of(&self.raw.kind)
    }

    /// Position, in blocks.
    pub fn position(&self) -> [f64; 3] {
        self.raw.position
    }

    /// View angle in degrees.
    pub fn yaw(&self) -> f32 {
        self.raw.yaw
    }

    /// View angle in degrees.
    pub fn pitch(&self) -> f32 {
        self.raw.pitch
    }

    /// A player's name or the name tag, empty when there is none.
    pub fn name(&self) -> &str {
        str_of(&self.raw.name)
    }

    /// Health points, in half hearts; `0.0` unless [`is_living`](Self::is_living).
    pub fn health(&self) -> f32 {
        self.raw.health
    }

    /// Maximum health points; `0.0` unless [`is_living`](Self::is_living).
    ///
    /// The value the server sent as `generic.maxHealth`, else the vanilla default of
    /// the kind.
    pub fn max_health(&self) -> f32 {
        self.raw.max_health
    }

    /// Armour points of the equipment the client sees (vanilla: 20 is full
    /// diamond).
    pub fn armor(&self) -> f32 {
        self.raw.armor
    }

    /// The scoreboard team the entity is on, empty when none. Only players are
    /// looked up (by name).
    pub fn team(&self) -> &str {
        str_of(&self.raw.team)
    }

    /// Width of the bounding box, in blocks.
    pub fn width(&self) -> f32 {
        self.raw.width
    }

    /// Height of the bounding box, in blocks.
    pub fn height(&self) -> f32 {
        self.raw.height
    }

    /// Position change over the last tick, in blocks per tick.
    pub fn velocity(&self) -> [f64; 3] {
        self.raw.velocity
    }

    /// The raw `UMI_ENTITY_*` bit set; the typed helpers below read it.
    pub fn flags(&self) -> u32 {
        self.raw.flags
    }

    /// Whether the entity burns.
    pub fn on_fire(&self) -> bool {
        self.has(UMI_ENTITY_ON_FIRE)
    }

    /// Whether the entity is crouching.
    pub fn sneaking(&self) -> bool {
        self.has(UMI_ENTITY_SNEAKING)
    }

    /// Whether the entity is sprinting.
    pub fn sprinting(&self) -> bool {
        self.has(UMI_ENTITY_SPRINTING)
    }

    /// Whether the entity is using its held item (eating, blocking, drawing a bow).
    pub fn using_item(&self) -> bool {
        self.has(UMI_ENTITY_USING_ITEM)
    }

    /// Whether the entity is invisible.
    pub fn invisible(&self) -> bool {
        self.has(UMI_ENTITY_INVISIBLE)
    }

    /// Whether the entity is a living one, so its health fields mean something.
    pub fn is_living(&self) -> bool {
        self.has(UMI_ENTITY_LIVING)
    }

    /// Whether the entity is a player.
    pub fn is_player(&self) -> bool {
        self.raw.is_player != 0
    }

    /// Where the frame being drawn shows the entity (interpolated between two
    /// ticks). In a game-thread query it is [`position`](Self::position).
    pub fn render_position(&self) -> [f64; 3] {
        self.raw.render_position
    }

    /// Ticks left of the hurt flash: 10 right after a hit, then down to 0
    /// (vanilla `hurtTime`). API 1.4.
    pub fn hurt_time(&self) -> u32 {
        self.raw.hurt_time
    }

    /// Whether the entity flashes red from a recent hit. API 1.4.
    pub fn is_hurt(&self) -> bool {
        self.raw.hurt_time > 0
    }

    /// Ticks into the death fall, 0 while alive (vanilla `deathTime`). API 1.4.
    pub fn death_time(&self) -> u32 {
        self.raw.death_time
    }

    /// The item in the main hand as `(id, damage)`, `None` when empty. API 1.4.
    pub fn held_item(&self) -> Option<(i32, i32)> {
        (self.raw.held_item >= 0).then_some((self.raw.held_item, self.raw.held_damage))
    }

    /// A player's team prefix, name and suffix with their `§` colour codes, else
    /// the name tag; empty when there is none. API 1.4.
    pub fn display_name(&self) -> &str {
        str_of(&self.raw.display_name)
    }

    /// The team colour as `0xRRGGBB`: the team's colour, else the last colour
    /// code of its prefix; `None` when there is none. API 1.4.
    pub fn team_color(&self) -> Option<u32> {
        (self.raw.team_color != UMI_NO_COLOR).then_some(self.raw.team_color)
    }

    /// The latency the tab list shows, in milliseconds; `None` for an entity the
    /// tab list does not show. API 1.4.
    pub fn ping(&self) -> Option<i32> {
        (self.raw.ping >= 0).then_some(self.raw.ping)
    }

    /// The version nibble of the UUID: 4 for an online-mode player, 3 for an
    /// offline-mode one, 2 for many server-made NPCs; 0 when unknown. API 1.4.
    pub fn uuid_version(&self) -> u32 {
        self.raw.uuid_version
    }

    /// The potion effects the server told the client about, at most 8, in id
    /// order. API 1.4.
    pub fn effects(&self) -> impl ExactSizeIterator<Item = Effect> + '_ {
        let count = (self.raw.effect_count as usize).min(UMI_MAX_ENTITY_EFFECTS);
        self.raw.effects[..count].iter().map(|raw| Effect {
            id: raw.id,
            amplifier: raw.amplifier,
            hide_particles: raw.flags & UMI_EFFECT_HIDE_PARTICLES != 0,
            duration: raw.duration,
        })
    }

    /// The effect with the 1.8 potion `id`, if the entity has it. API 1.4.
    pub fn effect(&self, id: u8) -> Option<Effect> {
        self.effects().find(|effect| effect.id == id)
    }

    /// Whether the entity is a player in spectator mode. API 1.4.
    pub fn spectator(&self) -> bool {
        self.has(UMI_ENTITY_SPECTATOR)
    }

    /// Whether the tab list shows the entity. Players missing from it are often
    /// server-made NPCs; the client does not decide. API 1.4.
    pub fn in_tab_list(&self) -> bool {
        self.has(UMI_ENTITY_IN_TAB_LIST)
    }

    /// Whether a block-free line runs from the local player's eye to the
    /// entity's box, tested once per tick, up to 128 blocks. It is the player's
    /// eye, not a mod camera's. API 1.4.
    pub fn visible(&self) -> bool {
        self.has(UMI_ENTITY_VISIBLE)
    }

    /// Whether the entity is on the local player's scoreboard team. API 1.4.
    pub fn teammate(&self) -> bool {
        self.has(UMI_ENTITY_TEAMMATE)
    }

    fn has(&self, flag: u32) -> bool {
        self.raw.flags & flag != 0
    }
}

/// One potion effect of an [`Entity`] (API 1.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Effect {
    /// The 1.8 potion id (1 speed, 5 strength, 14 invisibility, …).
    pub id: u8,
    /// Level minus one: 0 for level I.
    pub amplifier: u8,
    /// Whether the server hides the effect's particles.
    pub hide_particles: bool,
    /// Ticks left as the server sent them (32767 shows as `**:**`).
    pub duration: i32,
}

/// Read-only world queries.
///
/// Cheap to copy and `Send + Sync`, but the host only answers on the game
/// thread inside a callback: a game-event intercept handler (for example
/// [`PostTick`](crate::PostTick)), a command, or a physics layer. Anywhere else
/// (observers, the HUD, other threads) it refuses with `HostRefused(UNSUPPORTED)`.
/// The exception is [`snapshot`](Self::snapshot), which is the render thread's
/// view of the entities. No permission is needed. Get one with
/// [`InitCx::world`](crate::InitCx::world).
#[derive(Debug, Clone, Copy)]
pub struct World;

impl World {
    /// The block at `(x, y, z)`.
    ///
    /// # Errors
    /// - [`ModError::Unavailable`] when the chunk is not loaded.
    /// - [`ModError::HostRefused`] with `UNSUPPORTED` outside a callback on
    ///   the game thread (or before `init`).
    pub fn block(&self, x: i32, y: i32, z: i32) -> Result<Block, ModError> {
        let mut state = 0_u32;
        host::call(|host, me| (host.world_block)(me, x, y, z, &raw mut state))
            .and_then(ModError::check)?;
        Ok(Block::from_state(state))
    }

    /// Calls `visitor` for every entity but the local player; it returns
    /// `false` to stop. A panic in the visitor is caught and stops the visit.
    ///
    /// ```ignore
    /// world.entities(|entity| {
    ///     println!("{} at {:?}", entity.kind(), entity.position());
    ///     true
    /// })?;
    /// ```
    ///
    /// # Errors
    /// [`ModError::HostRefused`] with `UNSUPPORTED` outside a callback on the
    /// game thread (or before `init`).
    pub fn entities(&self, mut visitor: impl FnMut(&Entity<'_>) -> bool) -> Result<(), ModError> {
        let mut visit = Visit::new(&mut visitor);
        let user = (&raw mut visit).cast::<c_void>();
        let status =
            host::call(|host, me| (host.world_entities)(me, Some(entity_trampoline), user))?;
        // Stopping early is a success, whatever status the host gives it.
        if visit.stopped() || status == UmiStatus::OK {
            Ok(())
        } else {
            ModError::check(status)
        }
    }

    /// Calls `visitor` for the entities nearest to the camera, nearest first, as the
    /// client saw them when it began drawing the frame; it returns `false` to
    /// stop. The local player is not among them.
    ///
    /// This is the render-thread counterpart of [`entities`](Self::entities): it
    /// answers inside a callback of the render thread (a `Frame` intercept, a HUD
    /// widget draw) and nowhere else. The list holds at most 256 entities and the
    /// strings of an [`Entity`] are valid only inside the visitor. Everything but
    /// [`render_position`](Entity::render_position) comes from the last game tick,
    /// so it can be one tick (50 ms) old; `render_position` is where this frame
    /// draws the entity.
    ///
    /// The rows are read into a buffer the thread keeps, so a call allocates only
    /// the first time. A panic in the visitor unwinds to the callback, as any other
    /// panic of the mod.
    ///
    /// ```ignore
    /// world.snapshot(|entity| {
    ///     if entity.is_living() {
    ///         hud.text(0.0, y, &format!("{} {:.0}", entity.kind(), entity.health()), WHITE, 1.0);
    ///     }
    ///     true
    /// })?;
    /// ```
    ///
    /// # Errors
    /// - [`ModError::Unavailable`] when there is no world (no session).
    /// - [`ModError::HostRefused`] with `UNSUPPORTED` outside a render-thread
    ///   callback (or before `init`), `DENIED` when the mod is off.
    pub fn snapshot(&self, mut visitor: impl FnMut(&Entity<'_>) -> bool) -> Result<(), ModError> {
        let mut rows = SNAPSHOT_ROWS.take();
        if rows.len() < SNAPSHOT_CAPACITY {
            rows.resize(SNAPSHOT_CAPACITY, blank_entity());
        }
        let mut count = 0_usize;
        let result = host::call(|host, me| {
            (host.world_entities_snapshot)(me, rows.as_mut_ptr(), rows.len(), &raw mut count)
        })
        .and_then(ModError::check);
        if result.is_ok() {
            for raw in rows.iter().take(count) {
                // A host older than this struct leaves `size` short: stop there.
                if (raw.size as usize) < size_of::<UmiEntity>() || !visitor(&Entity::new(*raw)) {
                    break;
                }
            }
        }
        SNAPSHOT_ROWS.set(rows);
        result
    }
}

impl World {
    /// Calls `f` with the local player as the snapshot sees it (API 1.4): its
    /// team, display name, effects, held item, health… The same rules as
    /// [`snapshot`](Self::snapshot): render-thread callbacks only, strings valid
    /// inside `f`.
    ///
    /// ```ignore
    /// let my_team = world.local_player(|me| me.team().to_owned())?;
    /// ```
    ///
    /// # Errors
    /// As [`snapshot`](Self::snapshot); [`ModError::Unavailable`] also before the
    /// client knows the local player.
    pub fn local_player<R>(&self, f: impl FnOnce(&Entity<'_>) -> R) -> Result<R, ModError> {
        let mut raw = blank_entity();
        host::call(|host, me| (host.world_local_player)(me, &raw mut raw))
            .and_then(ModError::check)?;
        Ok(f(&Entity::new(raw)))
    }
}

/// How many rows [`World::snapshot`] asks for: the host's bound (ADR-0094).
const SNAPSHOT_CAPACITY: usize = 256;

thread_local! {
    /// The rows of the last snapshot read, kept so the next one does not allocate.
    /// Taken while a visitor runs, so a visitor that reads a snapshot itself gets a
    /// buffer of its own instead of a borrow panic.
    static SNAPSHOT_ROWS: Cell<Vec<UmiEntity>> = const { Cell::new(Vec::new()) };
}

/// An all-zero row to fill the buffer with; the host overwrites what it answers.
fn blank_entity() -> UmiEntity {
    UmiEntity {
        size: 0,
        id: 0,
        kind: UmiStr::EMPTY,
        position: [0.0; 3],
        yaw: 0.0,
        pitch: 0.0,
        name: UmiStr::EMPTY,
        health: 0.0,
        max_health: 0.0,
        armor: 0.0,
        team: UmiStr::EMPTY,
        width: 0.0,
        height: 0.0,
        velocity: [0.0; 3],
        flags: 0,
        is_player: 0,
        render_position: [0.0; 3],
        hurt_time: 0,
        death_time: 0,
        held_item: -1,
        held_damage: 0,
        display_name: UmiStr::EMPTY,
        team_color: UMI_NO_COLOR,
        ping: -1,
        uuid_version: 0,
        effect_count: 0,
        effects: [UmiEffect::default(); UMI_MAX_ENTITY_EFFECTS],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_row_reads_as_nothing_known() {
        let entity = Entity::new(blank_entity());
        assert_eq!(entity.held_item(), None);
        assert_eq!((entity.team_color(), entity.ping()), (None, None));
        assert_eq!(entity.effects().len(), 0);
        assert!(!entity.is_hurt() && !entity.visible() && !entity.teammate());
    }

    #[test]
    fn the_1_4_accessors_read_the_row() {
        let mut raw = blank_entity();
        raw.hurt_time = 9;
        raw.held_item = 276;
        raw.held_damage = 2;
        raw.team_color = 0x55ff55;
        raw.ping = 0;
        raw.flags = UMI_ENTITY_VISIBLE | UMI_ENTITY_TEAMMATE | UMI_ENTITY_IN_TAB_LIST;
        raw.effect_count = 2;
        raw.effects[0] = UmiEffect {
            id: 1,
            amplifier: 1,
            flags: UMI_EFFECT_HIDE_PARTICLES,
            reserved: 0,
            duration: 40,
        };
        raw.effects[1].id = 14;
        let entity = Entity::new(raw);
        assert!(entity.is_hurt() && entity.hurt_time() == 9);
        assert_eq!(entity.held_item(), Some((276, 2)));
        assert_eq!(
            (entity.team_color(), entity.ping()),
            (Some(0x55ff55), Some(0))
        );
        assert!(entity.visible() && entity.teammate() && entity.in_tab_list());
        assert!(!entity.spectator());
        let speed = entity.effect(1).expect("speed");
        assert_eq!(
            (speed.amplifier, speed.hide_particles, speed.duration),
            (1, true, 40)
        );
        assert_eq!(entity.effect(14).map(|e| e.duration), Some(0));
        assert_eq!(entity.effect(5), None);
    }

    #[test]
    fn an_effect_count_past_the_array_is_cut() {
        let mut raw = blank_entity();
        raw.effect_count = 99;
        assert_eq!(Entity::new(raw).effects().len(), UMI_MAX_ENTITY_EFFECTS);
    }
}

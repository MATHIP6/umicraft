// SPDX-License-Identifier: MIT OR Apache-2.0

//! Services a mod may wrap or replace, and the read-only world queries (API 0.5,
//! ADR-0086).
//!
//! `player_physics` is the step that moves the local player once per tick; a mod
//! declares it in `mod.toml` (`wraps` or `replaces`) and needs the `movement`
//! permission. API 1.1 adds `render_camera` (see the `camera` module).

use core::ffi::c_void;

use crate::{UmiStatus, UmiStr};

/// How a mod takes part in a service.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiServiceMode(pub u32);

impl UmiServiceMode {
    /// Runs around the inner implementation, which it reaches through
    /// `UmiHost::physics_next`. Wrappers nest in load order: the first loaded is
    /// the outermost.
    pub const WRAP: Self = Self(0);
    /// Takes the place of the built-in implementation; at most one mod per
    /// service (two replacing mods are a load conflict).
    pub const REPLACE: Self = Self(1);
}

/// [`UmiPlayerPhysics::flags`]: the player stands on a block. Writable.
pub const UMI_PHYSICS_ON_GROUND: u32 = 1;
/// [`UmiPlayerPhysics::flags`]: the player sprints. Writable.
pub const UMI_PHYSICS_SPRINTING: u32 = 2;
/// [`UmiPlayerPhysics::flags`]: the player sneaks. Read-only.
pub const UMI_PHYSICS_SNEAKING: u32 = 4;
/// [`UmiPlayerPhysics::flags`]: the player flies. Writable since API 1.1 (a
/// change is reported to the server like a vanilla toggle).
pub const UMI_PHYSICS_FLYING: u32 = 8;
/// [`UmiPlayerPhysics::flags`]: the player is in water. Read-only.
pub const UMI_PHYSICS_IN_WATER: u32 = 16;
/// [`UmiPlayerPhysics::flags`]: the abilities allow flight. Read-only. Added in
/// API 1.1.
pub const UMI_PHYSICS_ALLOW_FLYING: u32 = 32;

/// [`UmiPlayerPhysics::game_mode`]: survival.
pub const UMI_GAME_MODE_SURVIVAL: u32 = 0;
/// [`UmiPlayerPhysics::game_mode`]: creative.
pub const UMI_GAME_MODE_CREATIVE: u32 = 1;
/// [`UmiPlayerPhysics::game_mode`]: adventure.
pub const UMI_GAME_MODE_ADVENTURE: u32 = 2;
/// [`UmiPlayerPhysics::game_mode`]: spectator.
pub const UMI_GAME_MODE_SPECTATOR: u32 = 3;

/// The local player's movement state, lent to a `player_physics` layer.
///
/// The host writes back only the fields a layer changed (position, velocity,
/// fall distance and the writable flags), so a layer that changes nothing
/// leaves the vanilla step bit for bit as it was (ADR-0076).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UmiPlayerPhysics {
    /// Size in bytes of the struct, so later minors can append fields.
    pub size: u32,
    /// Bit set of `UMI_PHYSICS_*` flags.
    pub flags: u32,
    /// Feet position, in blocks.
    pub position: [f64; 3],
    /// Motion per tick, in blocks.
    pub velocity: [f64; 3],
    /// View angle in degrees (read-only; the step takes it from the input).
    pub yaw: f32,
    /// View angle in degrees, positive looks down (read-only).
    pub pitch: f32,
    /// Distance fallen since the last landing, in blocks.
    pub fall_distance: f32,
    /// Flying speed of the abilities (read-only). Added in API 1.1: check
    /// `size`.
    pub fly_speed: f32,
    /// `UMI_GAME_MODE_*`. Writable: a change is a local prediction, the
    /// server's next game-mode packet wins. Added in API 1.1.
    pub game_mode: u32,
}

/// [`UmiPhysicsInput::flags`]: the jump key is held.
pub const UMI_INPUT_JUMP: u32 = 1;
/// [`UmiPhysicsInput::flags`]: the sneak key is held.
pub const UMI_INPUT_SNEAK: u32 = 2;
/// [`UmiPhysicsInput::flags`]: the sprint key is held.
pub const UMI_INPUT_SPRINT: u32 = 4;

/// What the player asks for this tick; read-only.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UmiPhysicsInput {
    /// Size in bytes of the struct.
    pub size: u32,
    /// Bit set of `UMI_INPUT_*` flags.
    pub flags: u32,
    /// Forward movement, `-1.0` (backwards) ..= `1.0`.
    pub forward: f32,
    /// Sideways movement, `-1.0` (right) ..= `1.0` (left).
    pub strafe: f32,
    /// View angle in degrees.
    pub yaw: f32,
    /// View angle in degrees.
    pub pitch: f32,
}

/// The host's state for one call of a physics layer; opaque to mods.
#[repr(C)]
pub struct UmiPhysicsCx {
    _private: [u8; 0],
}

/// One `player_physics` layer: changes `state` for this tick, calling
/// `UmiHost::physics_next(cx)` at most once to run the inner layers (a
/// wrapper). Called on the game thread within a time budget, and again for past
/// ticks when the server corrects the position: keep it a function of `state`
/// and `input`.
pub type UmiPhysicsFn = extern "C" fn(
    cx: *mut UmiPhysicsCx,
    state: *mut UmiPlayerPhysics,
    input: *const UmiPhysicsInput,
    user: *mut c_void,
) -> UmiStatus;

/// One entity of the world, lent to a `world_entities` visitor. The local
/// player is not listed.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UmiEntity {
    /// Size in bytes of the struct.
    pub size: u32,
    /// The server's entity id.
    pub id: i32,
    /// Stable snake_case kind (`player`, `zombie`, `item`, …); the list is in
    /// `docs/modding/world.md` and names are never reused.
    pub kind: UmiStr,
    /// Position, in blocks.
    pub position: [f64; 3],
    /// View angle in degrees.
    pub yaw: f32,
    /// View angle in degrees.
    pub pitch: f32,
    /// A player's name or the name tag, empty when there is none.
    pub name: UmiStr,
    /// Health points; 0 unless `UMI_ENTITY_LIVING`. Added in API 1.1: check
    /// `size`.
    pub health: f32,
    /// Maximum health points; 0 unless `UMI_ENTITY_LIVING`. Added in API 1.1.
    pub max_health: f32,
    /// Armour points of the equipment the client sees. Added in API 1.1.
    pub armor: f32,
    /// Scoreboard team name, empty when none. Added in API 1.1.
    pub team: UmiStr,
    /// Width of the bounding box, in blocks. Added in API 1.1.
    pub width: f32,
    /// Height of the bounding box, in blocks. Added in API 1.1.
    pub height: f32,
    /// Position change over the last tick, in blocks per tick. Added in API 1.1.
    pub velocity: [f64; 3],
    /// Bit set of `UMI_ENTITY_*`. Added in API 1.1.
    pub flags: u32,
    /// `1` for a player, else `0`. Added in API 1.1.
    pub is_player: u8,
    /// Where the frame being drawn shows the entity (interpolated); equal to
    /// `position` in a game-thread query. Added in API 1.1.
    pub render_position: [f64; 3],
}

/// [`UmiEntity::flags`]: the entity burns.
pub const UMI_ENTITY_ON_FIRE: u32 = 1;
/// [`UmiEntity::flags`]: the entity sneaks.
pub const UMI_ENTITY_SNEAKING: u32 = 2;
/// [`UmiEntity::flags`]: the entity sprints.
pub const UMI_ENTITY_SPRINTING: u32 = 4;
/// [`UmiEntity::flags`]: the entity eats, drinks, blocks or draws a bow.
pub const UMI_ENTITY_USING_ITEM: u32 = 8;
/// [`UmiEntity::flags`]: the entity is invisible.
pub const UMI_ENTITY_INVISIBLE: u32 = 16;
/// [`UmiEntity::flags`]: a living entity; the health fields are meaningful.
pub const UMI_ENTITY_LIVING: u32 = 32;

/// Receives one entity; any status but `OK` stops the visit.
pub type UmiEntityVisitFn = extern "C" fn(entity: *const UmiEntity, user: *mut c_void) -> UmiStatus;

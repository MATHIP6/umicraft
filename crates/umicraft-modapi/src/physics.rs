// SPDX-License-Identifier: MIT OR Apache-2.0

//! Wrapping or replacing the player's movement step (`player_physics`).

use std::mem::size_of;

use umicraft_modapi_sys::{
    UmiPhysicsCx, UmiPhysicsInput, UmiPlayerPhysics, UMI_GAME_MODE_ADVENTURE,
    UMI_GAME_MODE_CREATIVE, UMI_GAME_MODE_SPECTATOR, UMI_GAME_MODE_SURVIVAL, UMI_INPUT_JUMP,
    UMI_INPUT_SNEAK, UMI_INPUT_SPRINT, UMI_PHYSICS_ALLOW_FLYING, UMI_PHYSICS_FLYING,
    UMI_PHYSICS_IN_WATER, UMI_PHYSICS_ON_GROUND, UMI_PHYSICS_SNEAKING, UMI_PHYSICS_SPRINTING,
};

use crate::ffi::StatePtr;
use crate::{host, ModError};

/// The flying speed of a player whose host does not report one: the vanilla
/// default of the abilities (MCP `PlayerCapabilities.flySpeed`).
const DEFAULT_FLY_SPEED: f32 = 0.05;

/// A game mode of the local player.
///
/// ```
/// use umicraft_modapi::GameMode;
///
/// assert_eq!(GameMode::from_raw(1), Some(GameMode::Creative));
/// assert_eq!(GameMode::from_raw(7), None);
/// assert_eq!(u32::from(GameMode::Spectator), 3);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameMode {
    /// Survival (0).
    Survival,
    /// Creative (1).
    Creative,
    /// Adventure (2).
    Adventure,
    /// Spectator (3).
    Spectator,
}

impl GameMode {
    /// The game mode of a `UMI_GAME_MODE_*` value, `None` for any other value
    /// (a host newer than the mod may know more modes).
    pub const fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            UMI_GAME_MODE_SURVIVAL => Some(Self::Survival),
            UMI_GAME_MODE_CREATIVE => Some(Self::Creative),
            UMI_GAME_MODE_ADVENTURE => Some(Self::Adventure),
            UMI_GAME_MODE_SPECTATOR => Some(Self::Spectator),
            _ => None,
        }
    }

    /// The `UMI_GAME_MODE_*` value of the game mode.
    pub const fn to_raw(self) -> u32 {
        match self {
            Self::Survival => UMI_GAME_MODE_SURVIVAL,
            Self::Creative => UMI_GAME_MODE_CREATIVE,
            Self::Adventure => UMI_GAME_MODE_ADVENTURE,
            Self::Spectator => UMI_GAME_MODE_SPECTATOR,
        }
    }
}

impl From<GameMode> for u32 {
    fn from(mode: GameMode) -> Self {
        mode.to_raw()
    }
}

/// The local player's movement state for one tick, as a physics layer sees it.
///
/// A safe view over the host's struct. Only the fields the host reads back
/// have setters: position, velocity, fall distance, `on_ground`, `sprinting`,
/// `flying` and the game mode. Everything else is read-only on purpose; the
/// host would ignore a change anyway.
///
/// The flight and game-mode fields exist from API 1.1. A host that gives a
/// smaller struct (`size` below the one this crate was built with) has none of
/// them: the getters answer the documented fallback and the setters do
/// nothing.
#[repr(transparent)]
#[derive(Debug)]
pub struct PlayerState(UmiPlayerPhysics);

impl PlayerState {
    /// Whether the host's struct covers the fields added in API 1.1.
    fn has_flight_fields(&self) -> bool {
        self.0.size as usize >= size_of::<UmiPlayerPhysics>()
    }

    fn flag(&self, bit: u32) -> bool {
        self.0.flags & bit != 0
    }

    fn set_flag(&mut self, bit: u32, on: bool) {
        if on {
            self.0.flags |= bit;
        } else {
            self.0.flags &= !bit;
        }
    }

    /// Feet position, in blocks.
    pub fn position(&self) -> [f64; 3] {
        self.0.position
    }

    /// Moves the player to `position`, in blocks.
    pub fn set_position(&mut self, position: [f64; 3]) {
        self.0.position = position;
    }

    /// Motion per tick, in blocks (y is up).
    pub fn velocity(&self) -> [f64; 3] {
        self.0.velocity
    }

    /// Sets the motion per tick, in blocks.
    pub fn set_velocity(&mut self, velocity: [f64; 3]) {
        self.0.velocity = velocity;
    }

    /// Whether the player stands on a block.
    pub fn on_ground(&self) -> bool {
        self.flag(UMI_PHYSICS_ON_GROUND)
    }

    /// Sets whether the player stands on a block.
    pub fn set_on_ground(&mut self, on_ground: bool) {
        self.set_flag(UMI_PHYSICS_ON_GROUND, on_ground);
    }

    /// Whether the player sprints.
    pub fn sprinting(&self) -> bool {
        self.flag(UMI_PHYSICS_SPRINTING)
    }

    /// Sets whether the player sprints.
    pub fn set_sprinting(&mut self, sprinting: bool) {
        self.set_flag(UMI_PHYSICS_SPRINTING, sprinting);
    }

    /// Distance fallen since the last landing, in blocks.
    pub fn fall_distance(&self) -> f32 {
        self.0.fall_distance
    }

    /// Sets the distance fallen since the last landing, in blocks.
    pub fn set_fall_distance(&mut self, fall_distance: f32) {
        self.0.fall_distance = fall_distance;
    }

    /// Whether the player sneaks. Read-only.
    pub fn sneaking(&self) -> bool {
        self.flag(UMI_PHYSICS_SNEAKING)
    }

    /// Whether the player flies.
    pub fn flying(&self) -> bool {
        self.flag(UMI_PHYSICS_FLYING)
    }

    /// Starts or stops the flight (API 1.1).
    ///
    /// The host applies it even when [`allow_flying`](Self::allow_flying) is
    /// false, and tells the server once; the server has the last word and may
    /// stop a flight it refuses. Does nothing on a host older than 1.1.
    pub fn set_flying(&mut self, flying: bool) {
        if self.has_flight_fields() {
            self.set_flag(UMI_PHYSICS_FLYING, flying);
        }
    }

    /// Whether the abilities the server granted allow flight (creative,
    /// spectator, or a plugin's grant). Read-only; `false` on a host older
    /// than 1.1.
    pub fn allow_flying(&self) -> bool {
        self.has_flight_fields() && self.flag(UMI_PHYSICS_ALLOW_FLYING)
    }

    /// The flying speed of the abilities, in the units of the server's
    /// `flySpeed` (0.05 by default). Read-only; `0.05` on a host older than
    /// 1.1.
    pub fn fly_speed(&self) -> f32 {
        if self.has_flight_fields() {
            self.0.fly_speed
        } else {
            DEFAULT_FLY_SPEED
        }
    }

    /// The game mode the client is in now. [`GameMode::Survival`] on a host
    /// older than 1.1 and for a value this crate does not know.
    pub fn game_mode(&self) -> GameMode {
        self.has_flight_fields()
            .then(|| GameMode::from_raw(self.0.game_mode))
            .flatten()
            .unwrap_or(GameMode::Survival)
    }

    /// Predicts a game mode locally (API 1.1).
    ///
    /// No packet is sent: the client applies the mode as it applies the
    /// server's own change, and the next game-mode packet of the server wins.
    /// Does nothing on a host older than 1.1.
    pub fn set_game_mode(&mut self, mode: GameMode) {
        if self.has_flight_fields() {
            self.0.game_mode = mode.to_raw();
        }
    }

    /// Whether the player is in water. Read-only.
    pub fn in_water(&self) -> bool {
        self.flag(UMI_PHYSICS_IN_WATER)
    }

    /// View angle in degrees. Read-only: the step takes it from the input.
    pub fn yaw(&self) -> f32 {
        self.0.yaw
    }

    /// View angle in degrees, positive looks down. Read-only.
    pub fn pitch(&self) -> f32 {
        self.0.pitch
    }
}

/// What the player asks for this tick. Read-only.
#[repr(transparent)]
#[derive(Debug)]
pub struct Input(UmiPhysicsInput);

impl Input {
    /// Forward movement, `-1.0` (backwards) ..= `1.0`.
    pub fn forward(&self) -> f32 {
        self.0.forward
    }

    /// Sideways movement, `-1.0` (right) ..= `1.0` (left).
    pub fn strafe(&self) -> f32 {
        self.0.strafe
    }

    /// Whether the jump key is held.
    pub fn jump(&self) -> bool {
        self.0.flags & UMI_INPUT_JUMP != 0
    }

    /// Whether the sneak key is held.
    pub fn sneak(&self) -> bool {
        self.0.flags & UMI_INPUT_SNEAK != 0
    }

    /// Whether the sprint key is held.
    pub fn sprint(&self) -> bool {
        self.0.flags & UMI_INPUT_SPRINT != 0
    }

    /// View angle in degrees.
    pub fn yaw(&self) -> f32 {
        self.0.yaw
    }

    /// View angle in degrees.
    pub fn pitch(&self) -> f32 {
        self.0.pitch
    }
}

/// One call of a physics layer, handed to the closure given to
/// [`InitCx::wrap_physics`](crate::InitCx::wrap_physics) or
/// [`replace_physics`](crate::InitCx::replace_physics).
///
/// It only lives for that call. A wrapper that changes nothing leaves the
/// vanilla step bit for bit as it was.
#[derive(Debug)]
pub struct Physics<'a> {
    cx: *mut UmiPhysicsCx,
    state: StatePtr<PlayerState>,
    input: &'a Input,
}

impl<'a> Physics<'a> {
    pub(crate) fn new(
        cx: *mut UmiPhysicsCx,
        state: StatePtr<PlayerState>,
        input: &'a Input,
    ) -> Self {
        Self { cx, state, input }
    }

    /// The state as it is now: what the layer received, or what the inner
    /// layers made of it after [`next`](Self::next).
    pub fn state(&self) -> &PlayerState {
        self.state.get()
    }

    /// The state, to change before or after [`next`](Self::next).
    pub fn state_mut(&mut self) -> &mut PlayerState {
        self.state.get_mut()
    }

    /// What the player asks for this tick.
    pub fn input(&self) -> &Input {
        self.input
    }

    /// Runs the inner layers and the built-in step on the state, once.
    ///
    /// Call it at most once per tick; a replacing layer has nothing inside
    /// and cannot call it. Skip it in a wrapper to suppress the movement.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] with `DENIED` for a second call and
    /// `UNSUPPORTED` in a replacing layer.
    // The name mirrors the ABI's `physics_next` and the frozen interface; this
    // type is not an iterator, so the confusion clippy fears cannot happen.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<(), ModError> {
        host::call(|host, _| (host.physics_next)(self.cx)).and_then(ModError::check)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(size: usize) -> PlayerState {
        PlayerState(UmiPlayerPhysics {
            size: size as u32,
            flags: 0,
            position: [0.0; 3],
            velocity: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            fall_distance: 0.0,
            fly_speed: 0.1,
            game_mode: UMI_GAME_MODE_CREATIVE,
        })
    }

    /// The size a 1.0 host reports: the struct ended after `fall_distance`.
    const SIZE_1_0: usize = 72;

    #[test]
    fn game_modes_convert_both_ways() {
        for mode in [
            GameMode::Survival,
            GameMode::Creative,
            GameMode::Adventure,
            GameMode::Spectator,
        ] {
            assert_eq!(GameMode::from_raw(mode.to_raw()), Some(mode));
            assert_eq!(u32::from(mode), mode.to_raw());
        }
        assert_eq!(GameMode::Spectator.to_raw(), 3);
    }

    #[test]
    fn an_unknown_game_mode_value_is_none() {
        assert_eq!(GameMode::from_raw(4), None);
        assert_eq!(GameMode::from_raw(u32::MAX), None);
    }

    #[test]
    fn the_flight_fields_are_read_from_a_1_1_host() {
        let mut state = state(size_of::<UmiPlayerPhysics>());
        state.0.flags = UMI_PHYSICS_ALLOW_FLYING;
        assert!(state.allow_flying());
        assert!(!state.flying());
        assert_eq!(state.fly_speed(), 0.1);
        assert_eq!(state.game_mode(), GameMode::Creative);
    }

    #[test]
    fn the_flight_and_mode_setters_write_the_struct() {
        let mut state = state(size_of::<UmiPlayerPhysics>());
        state.set_flying(true);
        assert!(state.flying());
        assert_eq!(state.0.flags, UMI_PHYSICS_FLYING);
        state.set_flying(false);
        assert!(!state.flying());
        state.set_game_mode(GameMode::Adventure);
        assert_eq!(state.0.game_mode, UMI_GAME_MODE_ADVENTURE);
        assert_eq!(state.game_mode(), GameMode::Adventure);
    }

    #[test]
    fn an_unknown_mode_of_the_host_reads_as_survival() {
        let mut state = state(size_of::<UmiPlayerPhysics>());
        state.0.game_mode = 9;
        assert_eq!(state.game_mode(), GameMode::Survival);
    }

    #[test]
    fn a_1_0_host_gets_the_fallbacks_and_inert_setters() {
        let mut state = state(SIZE_1_0);
        // Garbage where a 1.0 host has padding must not be believed.
        state.0.flags = UMI_PHYSICS_ALLOW_FLYING;
        assert!(!state.allow_flying());
        assert_eq!(state.fly_speed(), DEFAULT_FLY_SPEED);
        assert_eq!(state.game_mode(), GameMode::Survival);
        state.0.flags = 0;
        state.set_flying(true);
        state.set_game_mode(GameMode::Spectator);
        assert!(!state.flying());
        assert_eq!(state.0.flags, 0);
        assert_eq!(state.0.game_mode, UMI_GAME_MODE_CREATIVE);
    }
}

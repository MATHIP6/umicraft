// SPDX-License-Identifier: MIT OR Apache-2.0

//! Wrapping or replacing the player's movement step (`player_physics`).

use umicraft_modapi_sys::{
    UmiPhysicsCx, UmiPhysicsInput, UmiPlayerPhysics, UMI_INPUT_JUMP, UMI_INPUT_SNEAK,
    UMI_INPUT_SPRINT, UMI_PHYSICS_FLYING, UMI_PHYSICS_IN_WATER, UMI_PHYSICS_ON_GROUND,
    UMI_PHYSICS_SNEAKING, UMI_PHYSICS_SPRINTING,
};

use crate::ffi::StatePtr;
use crate::{host, ModError};

/// The local player's movement state for one tick, as a physics layer sees it.
///
/// A safe view over the host's struct. Only the fields the host reads back
/// have setters: position, velocity, fall distance, `on_ground` and
/// `sprinting`. Everything else is read-only on purpose; the host would ignore
/// a change anyway.
#[repr(transparent)]
#[derive(Debug)]
pub struct PlayerState(UmiPlayerPhysics);

impl PlayerState {
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

    /// Whether the player flies (creative). Read-only.
    pub fn flying(&self) -> bool {
        self.flag(UMI_PHYSICS_FLYING)
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
    state: StatePtr,
    input: &'a Input,
}

impl<'a> Physics<'a> {
    pub(crate) fn new(cx: *mut UmiPhysicsCx, state: StatePtr, input: &'a Input) -> Self {
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

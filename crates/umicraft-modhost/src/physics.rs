// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `player_physics` service: mods wrap or replace the step that moves the local
//! player (F3.6, ADR-0086).
//!
//! Everything here is on the game thread: the layers run inside the tick and again
//! when the server corrects the position, so they must be a function of their
//! arguments. The chain itself (nesting, budgets, `next`) is the shared
//! [`ServiceChain`](crate::service::ServiceChain).

use core::ffi::c_void;
use std::cell::Cell;
use std::sync::Arc;
use std::thread::LocalKey;

use umicraft_modapi_sys::{
    UmiPhysicsCx, UmiPhysicsFn, UmiPhysicsInput, UmiPlayerPhysics, UmiStatus,
};

use crate::context::ModContext;
use crate::ffi::PHYSICS_ACTIVE;
use crate::manifest::PLAYER_PHYSICS;
use crate::service::{Service, ServiceChain, ServiceSlot};

/// The `player_physics` service.
#[derive(Debug)]
pub(crate) struct PlayerPhysics;

impl Service for PlayerPhysics {
    type State = UmiPlayerPhysics;
    type Input = UmiPhysicsInput;
    type Step = UmiPhysicsFn;
    const ID: &'static str = PLAYER_PHYSICS;
    const LAYER: &'static str = "its `player_physics` layer";

    fn call(
        step: UmiPhysicsFn,
        cx: *mut c_void,
        state: *mut UmiPlayerPhysics,
        input: *const UmiPhysicsInput,
        user: *mut c_void,
    ) -> UmiStatus {
        step(cx.cast::<UmiPhysicsCx>(), state, input, user)
    }

    fn slot(context: &ModContext) -> &ServiceSlot<Self> {
        context.physics_slot()
    }

    fn active() -> &'static LocalKey<Cell<*mut c_void>> {
        &PHYSICS_ACTIVE
    }
}

/// The `player_physics` layers of every loaded mod, ready to run a step.
///
/// Cheap to clone; `Send`, so the client can hand it to the game state. Layers are
/// looked up per step, so a mod that is switched off or fails takes effect at once.
#[derive(Debug, Clone)]
pub struct PhysicsChain(ServiceChain<PlayerPhysics>);

impl PhysicsChain {
    /// The chain of the layers `contexts` registered, in the order given (load
    /// order), with replacing layers after the wrappers that surround them. `None`
    /// when no context registered one.
    pub fn new<'a>(contexts: impl IntoIterator<Item = &'a Arc<ModContext>>) -> Option<Self> {
        ServiceChain::new(contexts).map(Self)
    }

    /// Whether some layer is enabled now; when not, [`step`](Self::step) is just
    /// `vanilla`.
    pub fn is_active(&self) -> bool {
        self.0.is_active()
    }

    /// Runs one step on `state`: the outermost enabled wrapper first, each reaching
    /// the next through `physics_next`, and `vanilla` at the end unless an enabled
    /// replacing layer takes its place.
    ///
    /// A layer that fails (panic, error status, budget) fails its mod and its own
    /// changes are undone. If it had called `physics_next` the state is what the
    /// inner chain produced; if not, it is what the layer received and the rest of
    /// the chain runs on it. Either way the player still moves.
    pub fn step(
        &self,
        state: &mut UmiPlayerPhysics,
        input: &UmiPhysicsInput,
        vanilla: &mut dyn FnMut(&mut UmiPlayerPhysics),
    ) {
        self.0.run(state, input, vanilla);
    }
}

#[cfg(test)]
mod tests;

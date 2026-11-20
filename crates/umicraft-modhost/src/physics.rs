// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `player_physics` service: mods wrap or replace the step that moves the local
//! player (F3.6, ADR-0086).
//!
//! A mod registers at most one [`PhysicsLayer`] during init. [`PhysicsChain`] strings
//! the layers of every loaded mod around the built-in step: wrappers nest in load
//! order (the first loaded is the outermost) and the enabled replacing layer, if any,
//! takes the place of the built-in step. Everything here is on the game thread: the
//! layers run inside the tick and again when the server corrects the position, so
//! they must be a function of their arguments.
//!
//! This module is the policy; the raw-pointer edge (the opaque `UmiPhysicsCx`, the
//! pointer to the state, the host functions) is in `ffi::physics`.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use umicraft_modapi_sys::{
    UmiPhysicsFn, UmiPhysicsInput, UmiPlayerPhysics, UmiServiceMode, UmiStatus,
};

use crate::budget::{Stats, BUDGET};
use crate::context::ModContext;
use crate::ffi::{with_physics_cx, StatePtr, UserPtr};
use crate::hook::run_hook_excluding;
use crate::manifest::{Load, PLAYER_PHYSICS};

/// The layer a mod registered.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PhysicsLayer {
    replacing: bool,
    step: UmiPhysicsFn,
    user: UserPtr,
}

#[derive(Debug, Default)]
struct Registration {
    layer: Option<PhysicsLayer>,
    /// Set when `umi_mod_init` returned: no registration after that.
    sealed: bool,
}

/// The `player_physics` half of a mod's context: what its manifest allows, the layer
/// it registered and the layer's counters.
#[derive(Debug)]
pub(crate) struct PhysicsSlot {
    wraps: bool,
    replaces: bool,
    movement: bool,
    registration: Mutex<Registration>,
    stats: Stats,
}

impl PhysicsSlot {
    /// A slot for a mod whose manifest has `load` and which holds the `movement`
    /// permission or not.
    pub(crate) fn new(load: &Load, movement: bool) -> Self {
        Self {
            wraps: load.wraps_service(PLAYER_PHYSICS),
            replaces: load.replaces_service(PLAYER_PHYSICS),
            movement,
            registration: Mutex::default(),
            stats: Stats::default(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Registration> {
        // A panic while holding the lock cannot leave the registration inconsistent.
        self.registration
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Records the mod's layer; see `ModContext::register_physics` for the statuses.
    pub(crate) fn register(
        &self,
        mode: UmiServiceMode,
        step: Option<UmiPhysicsFn>,
        user: UserPtr,
    ) -> UmiStatus {
        let replacing = match mode {
            UmiServiceMode::WRAP => false,
            UmiServiceMode::REPLACE => true,
            _ => return UmiStatus::INVALID_ARGUMENT,
        };
        let Some(step) = step else {
            return UmiStatus::INVALID_ARGUMENT;
        };
        let declared = if replacing { self.replaces } else { self.wraps };
        let mut registration = self.lock();
        if !self.movement || !declared || registration.sealed {
            return UmiStatus::DENIED;
        }
        if registration.layer.is_some() {
            return UmiStatus::CONFLICT;
        }
        registration.layer = Some(PhysicsLayer {
            replacing,
            step,
            user,
        });
        UmiStatus::OK
    }

    /// Closes registration; a failed init also drops the layer it had registered.
    pub(crate) fn seal(&self, keep: bool) {
        let mut registration = self.lock();
        registration.sealed = true;
        if !keep {
            registration.layer = None;
        }
    }

    pub(crate) fn layer(&self) -> Option<PhysicsLayer> {
        self.lock().layer
    }

    pub(crate) fn stats(&self) -> &Stats {
        &self.stats
    }
}

/// A layer together with the context that owns it.
#[derive(Debug, Clone)]
struct Layer {
    context: Arc<ModContext>,
    spec: PhysicsLayer,
}

/// What one step is working on: the player's state, the input and the built-in step.
pub(crate) struct Frame<'a> {
    state: StatePtr<'a>,
    input: &'a UmiPhysicsInput,
    vanilla: &'a mut dyn FnMut(&mut UmiPlayerPhysics),
}

/// One running layer, which `UmiPhysicsCx` points at for the duration of its call.
pub(crate) struct LayerRun<'a, 'f> {
    rest: &'a [Layer],
    frame: &'a mut Frame<'f>,
    next_called: bool,
    replacing: bool,
    /// Time spent running the rest of the chain, not charged to the layer.
    inner: Duration,
    /// The state as the rest of the chain left it, once `next` ran.
    after_next: Option<UmiPlayerPhysics>,
}

impl LayerRun<'_, '_> {
    /// `physics_next`: runs the rest of the chain on the same state, once.
    pub(crate) fn next(&mut self) -> UmiStatus {
        if self.replacing {
            return UmiStatus::UNSUPPORTED;
        }
        if self.next_called {
            return UmiStatus::DENIED;
        }
        self.next_called = true;
        let started = Instant::now();
        run_chain(self.rest, self.frame);
        self.inner = started.elapsed();
        self.after_next = Some(self.frame.state.read());
        UmiStatus::OK
    }
}

impl Layer {
    /// Runs the layer; `rest` is what its `physics_next` runs. Returns whether the
    /// step is complete: the layer succeeded, or it failed after running `rest`.
    ///
    /// A failed layer's own changes are discarded because they may be half-made; the
    /// mod is already failed. The state goes back to what the rest of the chain
    /// produced if the layer got that far (so the player still moves by vanilla),
    /// else to what the layer received, for the rest of the chain to run on.
    fn call(&self, rest: &[Layer], frame: &mut Frame<'_>) -> bool {
        let received = frame.state.read();
        let (state, input) = (frame.state.as_ptr(), core::ptr::from_ref(frame.input));
        let mut run = LayerRun {
            rest,
            frame: &mut *frame,
            next_called: false,
            replacing: self.spec.replacing,
            inner: Duration::ZERO,
            after_next: None,
        };
        let succeeded = run_hook_excluding(
            &self.context,
            self.context.physics_counters(),
            BUDGET,
            "its `player_physics` layer",
            || {
                let status = with_physics_cx(&mut run, |cx| {
                    (self.spec.step)(cx, state, input, self.spec.user.as_ptr())
                });
                (status, run.inner)
            },
        );
        let after_next = run.after_next;
        if !succeeded {
            frame.state.write(after_next.unwrap_or(received));
        }
        succeeded || after_next.is_some()
    }
}

/// Runs the first enabled layer of `layers` (a disabled one is skipped: it never
/// sees the step), else the built-in step.
fn run_chain(mut layers: &[Layer], frame: &mut Frame<'_>) {
    while let Some((layer, rest)) = layers.split_first() {
        layers = rest;
        if layer.context.is_enabled() && layer.call(rest, frame) {
            return;
        }
    }
    let state = frame.state;
    state.with_mut(|state| (frame.vanilla)(state));
}

/// The `player_physics` layers of every loaded mod, ready to run a step.
///
/// Cheap to clone; `Send`, so the client can hand it to the game state. Layers are
/// looked up per step, so a mod that is switched off or fails takes effect at once.
#[derive(Debug, Clone)]
pub struct PhysicsChain {
    layers: Arc<[Layer]>,
}

impl PhysicsChain {
    /// The chain of the layers `contexts` registered, in the order given (load
    /// order), with replacing layers after the wrappers that surround them. `None`
    /// when no context registered one.
    pub fn new<'a>(contexts: impl IntoIterator<Item = &'a Arc<ModContext>>) -> Option<Self> {
        let (replacing, mut layers): (Vec<_>, Vec<_>) = contexts
            .into_iter()
            .filter_map(|context| {
                let spec = context.physics_layer()?;
                Some(Layer {
                    context: Arc::clone(context),
                    spec,
                })
            })
            .partition(|layer| layer.spec.replacing);
        layers.extend(replacing);
        (!layers.is_empty()).then(|| Self {
            layers: layers.into(),
        })
    }

    /// Whether some layer is enabled now; when not, [`step`](Self::step) is just
    /// `vanilla`.
    pub fn is_active(&self) -> bool {
        self.layers.iter().any(|l| l.context.is_enabled())
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
        let mut frame = Frame {
            state: StatePtr::new(state),
            input,
            vanilla,
        };
        run_chain(&self.layers, &mut frame);
    }
}

#[cfg(test)]
mod tests;

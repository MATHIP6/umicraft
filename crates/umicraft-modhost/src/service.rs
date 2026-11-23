// SPDX-License-Identifier: MIT OR Apache-2.0

//! The machinery shared by the services mods wrap or replace (ADR-0086, ADR-0094):
//! `player_physics` on the game thread and `render_camera` on the render thread.
//!
//! A mod registers at most one layer per service during init. A [`ServiceChain`]
//! strings the layers of every loaded mod around the built-in implementation:
//! wrappers nest in load order (the first loaded is the outermost) and the enabled
//! replacing layer, if any, takes the place of the built-in one. A wrapper reaches
//! the rest of the chain through its service's `*_next` host function, at most
//! once. A layer's own time is charged to its budget, the time spent inside `next`
//! is not.
//!
//! This module is the policy; the raw-pointer edge (the opaque `cx`, the pointer to
//! the state, the host functions) is in `ffi::service`.

use core::ffi::c_void;
use std::cell::Cell;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::LocalKey;
use std::time::{Duration, Instant};

use umicraft_modapi_sys::{UmiServiceMode, UmiStatus};

use crate::budget::{Stats, BUDGET};
use crate::context::ModContext;
use crate::ffi::{with_service_cx, StatePtr, UserPtr};
use crate::hook::run_hook_excluding;
use crate::manifest::Load;

/// One service of the ABI: its types, its id and where a context keeps it.
pub(crate) trait Service: Sized + 'static {
    /// The state a layer edits (`UmiPlayerPhysics`, `UmiRenderCamera`).
    type State: Copy;
    /// What the layer is told, read-only.
    type Input;
    /// The layer's `extern "C"` function.
    type Step: Copy + core::fmt::Debug;
    /// The service id of `mod.toml`.
    const ID: &'static str;
    /// How a failure names the layer: "its `<id>` layer".
    const LAYER: &'static str;

    /// Calls `step` with the opaque `cx` the layer hands back to `*_next`.
    fn call(
        step: Self::Step,
        cx: *mut c_void,
        state: *mut Self::State,
        input: *const Self::Input,
        user: *mut c_void,
    ) -> UmiStatus;

    /// The slot of this service in `context`.
    fn slot(context: &ModContext) -> &ServiceSlot<Self>;

    /// The per-thread pointer to the running layer, which `*_next` checks its `cx`
    /// against (one per service, so a camera `cx` never passes for a physics one).
    fn active() -> &'static LocalKey<Cell<*mut c_void>>;
}

/// The layer a mod registered.
#[derive(Debug)]
pub(crate) struct LayerSpec<S: Service> {
    replacing: bool,
    step: S::Step,
    user: UserPtr,
}

// Derived `Clone`/`Copy` would ask `S` itself to be `Copy`.
impl<S: Service> Clone for LayerSpec<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S: Service> Copy for LayerSpec<S> {}

#[derive(Debug)]
struct Registration<S: Service> {
    layer: Option<LayerSpec<S>>,
    /// Set when `umi_mod_init` returned: no registration after that.
    sealed: bool,
}

/// One service's half of a mod's context: what its manifest allows, the layer it
/// registered and the layer's counters.
#[derive(Debug)]
pub(crate) struct ServiceSlot<S: Service> {
    wraps: bool,
    replaces: bool,
    permitted: bool,
    registration: Mutex<Registration<S>>,
    stats: Stats,
}

impl<S: Service> ServiceSlot<S> {
    /// A slot for a mod whose manifest has `load` and which holds the service's
    /// permission (`permitted`) or not.
    pub(crate) fn new(load: &Load, permitted: bool) -> Self {
        Self {
            wraps: load.wraps_service(S::ID),
            replaces: load.replaces_service(S::ID),
            permitted,
            registration: Mutex::new(Registration {
                layer: None,
                sealed: false,
            }),
            stats: Stats::default(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Registration<S>> {
        // A panic while holding the lock cannot leave the registration inconsistent.
        self.registration
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Records the mod's layer: `INVALID_ARGUMENT` for a null function or an unknown
    /// mode, `DENIED` without the permission, without the service in the matching
    /// `mod.toml` list or after init, `CONFLICT` for a second layer.
    pub(crate) fn register(
        &self,
        mode: UmiServiceMode,
        step: Option<S::Step>,
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
        if !self.permitted || !declared || registration.sealed {
            return UmiStatus::DENIED;
        }
        if registration.layer.is_some() {
            return UmiStatus::CONFLICT;
        }
        registration.layer = Some(LayerSpec {
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

    pub(crate) fn layer(&self) -> Option<LayerSpec<S>> {
        self.lock().layer
    }

    pub(crate) fn stats(&self) -> &Stats {
        &self.stats
    }
}

/// A layer together with the context that owns it.
#[derive(Debug)]
struct Layer<S: Service> {
    context: Arc<ModContext>,
    spec: LayerSpec<S>,
}

/// What one run is working on: the state, the input and the built-in step.
pub(crate) struct Work<'a, S: Service> {
    state: StatePtr<'a, S::State>,
    input: &'a S::Input,
    builtin: &'a mut dyn FnMut(&mut S::State),
}

/// One running layer, which the opaque `cx` points at for the duration of its call.
pub(crate) struct LayerRun<'a, 'w, S: Service> {
    rest: &'a [Layer<S>],
    work: &'a mut Work<'w, S>,
    next_called: bool,
    replacing: bool,
    /// Time spent running the rest of the chain, not charged to the layer.
    inner: Duration,
    /// The state as the rest of the chain left it, once `next` ran.
    after_next: Option<S::State>,
}

impl<S: Service> LayerRun<'_, '_, S> {
    /// `*_next`: runs the rest of the chain on the same state, once.
    pub(crate) fn next(&mut self) -> UmiStatus {
        if self.replacing {
            return UmiStatus::UNSUPPORTED;
        }
        if self.next_called {
            return UmiStatus::DENIED;
        }
        self.next_called = true;
        let started = Instant::now();
        run_chain(self.rest, self.work);
        self.inner = started.elapsed();
        self.after_next = Some(self.work.state.read());
        UmiStatus::OK
    }
}

impl<S: Service> Layer<S> {
    /// Runs the layer; `rest` is what its `next` runs. Returns whether the run is
    /// complete: the layer succeeded, or it failed after running `rest`.
    ///
    /// A failed layer's own changes are discarded because they may be half-made; the
    /// mod is already failed. The state goes back to what the rest of the chain
    /// produced if the layer got that far, else to what the layer received, for the
    /// rest of the chain to run on.
    fn call(&self, rest: &[Layer<S>], work: &mut Work<'_, S>) -> bool {
        let received = work.state.read();
        let (state, input) = (work.state.as_ptr(), core::ptr::from_ref(work.input));
        let mut run = LayerRun {
            rest,
            work: &mut *work,
            next_called: false,
            replacing: self.spec.replacing,
            inner: Duration::ZERO,
            after_next: None,
        };
        let succeeded = run_hook_excluding(
            &self.context,
            S::slot(&self.context).stats(),
            BUDGET,
            S::LAYER,
            || {
                let status = with_service_cx(&mut run, |cx| {
                    S::call(self.spec.step, cx, state, input, self.spec.user.as_ptr())
                });
                (status, run.inner)
            },
        );
        let after_next = run.after_next;
        if !succeeded {
            work.state.write(after_next.unwrap_or(received));
        }
        succeeded || after_next.is_some()
    }
}

/// Runs the first enabled layer of `layers` (a disabled one is skipped: it never
/// sees the run), else the built-in step.
fn run_chain<S: Service>(mut layers: &[Layer<S>], work: &mut Work<'_, S>) {
    while let Some((layer, rest)) = layers.split_first() {
        layers = rest;
        if layer.context.is_enabled() && layer.call(rest, work) {
            return;
        }
    }
    let state = work.state;
    state.with_mut(|state| (work.builtin)(state));
}

/// The layers of one service from every loaded mod, ready to run.
///
/// Cheap to clone and `Send`. Layers are looked up per run, so a mod that is
/// switched off or fails takes effect at once.
#[derive(Debug)]
pub(crate) struct ServiceChain<S: Service> {
    layers: Arc<[Layer<S>]>,
}

impl<S: Service> Clone for ServiceChain<S> {
    fn clone(&self) -> Self {
        Self {
            layers: Arc::clone(&self.layers),
        }
    }
}

impl<S: Service> ServiceChain<S> {
    /// The chain of the layers `contexts` registered, in the order given (load
    /// order), with replacing layers after the wrappers that surround them. `None`
    /// when no context registered one.
    pub(crate) fn new<'a>(contexts: impl IntoIterator<Item = &'a Arc<ModContext>>) -> Option<Self> {
        let (replacing, mut layers): (Vec<_>, Vec<_>) = contexts
            .into_iter()
            .filter_map(|context| {
                let spec = S::slot(context).layer()?;
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

    /// Whether some layer is enabled now; when not, [`run`](Self::run) is just
    /// `builtin`.
    pub(crate) fn is_active(&self) -> bool {
        self.layers.iter().any(|l| l.context.is_enabled())
    }

    /// Runs the chain on `state`: the outermost enabled wrapper first, each reaching
    /// the next through `*_next`, and `builtin` at the end unless an enabled
    /// replacing layer takes its place.
    ///
    /// A layer that fails (panic, error status, budget) fails its mod and its own
    /// changes are undone. If it had called `next` the state is what the inner chain
    /// produced; if not, it is what the layer received and the rest of the chain runs
    /// on it.
    pub(crate) fn run(
        &self,
        state: &mut S::State,
        input: &S::Input,
        builtin: &mut dyn FnMut(&mut S::State),
    ) {
        let mut work = Work {
            state: StatePtr::new(state),
            input,
            builtin,
        };
        run_chain(&self.layers, &mut work);
    }
}

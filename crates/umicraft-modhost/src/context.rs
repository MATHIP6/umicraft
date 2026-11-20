// SPDX-License-Identifier: MIT OR Apache-2.0

//! The host's per-mod state, reached by mods through an opaque handle.

use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

use umicraft_modapi_sys::{
    UmiEventId, UmiHandlerFn, UmiLevel, UmiMode, UmiPhysicsFn, UmiServiceMode, UmiStatus,
};

use crate::budget::{ModStats, Stats};
use crate::events::is_known;
use crate::ffi::UserPtr;
use crate::manifest::{Load, ModId, Permissions};
use crate::physics::{PhysicsLayer, PhysicsSlot};
use crate::ui::shared::HostShared;
use crate::ui::state::UiState;
use crate::worker::WorkerSlot;

/// A handler a mod registered through `subscribe`.
#[derive(Debug, Clone)]
pub struct Subscription {
    /// The mod that registered the handler.
    pub owner: ModId,
    /// The event it listens to.
    pub event: UmiEventId,
    /// Whether it observes or may change the event.
    pub mode: UmiMode,
    /// Lower runs first.
    pub priority: i32,
    /// The mod's callback.
    pub handler: UmiHandlerFn,
    /// Opaque pointer to give back to `handler`.
    pub user: UserPtr,
}

/// State of one loaded mod; its address is the `UmiModHandle` the mod holds.
///
/// Everything is behind a mutex or an atomic because `log` may be called from any
/// thread at any time.
///
/// **Enable model** (F3.4): a mod is enabled when the player wants it
/// ([`ModContext::is_wanted`], set by the player's switch) and it has not failed
/// ([`ModContext::is_failed`], set by the host for a panic, an error status, an
/// exhausted budget or a failed init, never cleared). In server-compat mode a mod
/// that [`alters_game`](ModContext::alters_game) is held off as well. Switching a mod off and on
/// again is safe because nothing is freed until `umi_mod_shutdown` at exit; a
/// failed mod stays off because its state may be broken.
#[derive(Debug)]
pub struct ModContext {
    id: ModId,
    permissions: Permissions,
    subscriptions: Mutex<Vec<Subscription>>,
    init_done: AtomicBool,
    wanted: AtomicBool,
    failed: AtomicBool,
    /// Set once `umi_mod_init` succeeded; cleared by whoever calls `umi_mod_shutdown`.
    shutdown_due: AtomicBool,
    stats: Stats,
    physics: PhysicsSlot,
    worker: WorkerSlot,
    pub(crate) ui: UiState,
    pub(crate) shared: Arc<HostShared>,
    /// This context as an `Arc`, for the command table's weak back-pointer.
    pub(crate) me: Weak<ModContext>,
}

/// Longest log message forwarded, so a mod cannot flood the log with one call.
const MAX_LOG_BYTES: usize = 4096;

/// Decodes mod-provided bytes, replacing invalid UTF-8 and capping the length.
fn log_text(bytes: &[u8]) -> Cow<'_, str> {
    String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_LOG_BYTES)])
}

/// Whether `event` is one of the two network events, which need `network_packets`.
fn is_packet_event(event: UmiEventId) -> bool {
    matches!(event, UmiEventId::PACKET_RECEIVED | UmiEventId::PACKET_SEND)
}

impl ModContext {
    /// Creates the context of mod `id`: not initialised, not enabled, without
    /// permissions, with a command table and outboxes of its own (for tests and
    /// benchmarks).
    pub fn new(id: ModId) -> Arc<Self> {
        Self::with_shared(id, Permissions::default(), Arc::default())
    }

    /// Creates the context of mod `id`, which holds `permissions` (its manifest's),
    /// inside a host whose mods share `shared`.
    ///
    /// Returns an `Arc` because the command table needs a weak pointer back to the
    /// context that registered a command.
    pub fn with_shared(id: ModId, permissions: Permissions, shared: Arc<HostShared>) -> Arc<Self> {
        Self::with_services(id, permissions, &Load::default(), shared)
    }

    /// Like [`with_shared`](Self::with_shared) for a mod whose manifest has `load`:
    /// its `wraps` and `replaces` lists decide which services it may register for.
    pub fn with_services(
        id: ModId,
        permissions: Permissions,
        load: &Load,
        shared: Arc<HostShared>,
    ) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            id,
            permissions,
            subscriptions: Mutex::new(Vec::new()),
            init_done: AtomicBool::new(false),
            wanted: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            shutdown_due: AtomicBool::new(false),
            stats: Stats::default(),
            physics: PhysicsSlot::new(load, permissions.movement),
            worker: WorkerSlot::default(),
            ui: UiState::default(),
            shared,
            me: me.clone(),
        })
    }

    /// The mod this context belongs to.
    pub fn id(&self) -> &ModId {
        &self.id
    }

    /// The permissions the mod's manifest declared.
    pub fn permissions(&self) -> Permissions {
        self.permissions
    }

    /// Whether the mod may change what the server sees: it declared `network_packets`
    /// or `movement`. Server-compat mode holds such a mod off.
    pub fn alters_game(&self) -> bool {
        self.permissions.network_packets || self.permissions.movement
    }

    /// Whether server-compat mode holds this mod off: the mode is on and the mod
    /// [`alters_game`](Self::alters_game).
    pub fn is_held_by_compat(&self) -> bool {
        self.alters_game() && self.shared.server_compat()
    }

    /// Whether the mod runs: the player wants it, it has not failed and server-compat
    /// mode is not holding it off.
    pub fn is_enabled(&self) -> bool {
        self.is_wanted() && !self.is_failed() && !self.is_held_by_compat()
    }

    /// The player's switch: on after a successful init, until the player turns it off.
    pub fn is_wanted(&self) -> bool {
        self.wanted.load(Ordering::Acquire)
    }

    /// Whether the mod failed (failed init, panic, error status or exhausted budget).
    /// A failed mod stays disabled until the client restarts.
    pub fn is_failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }

    /// Sets the player's switch. Only a flag, because a dispatch to the mod may be in
    /// flight and `umi_mod_shutdown` frees the closures its trampolines use
    /// (shutdown-quiescence rule, ADR-0086). Returns `false` and changes nothing for
    /// a failed mod.
    pub fn set_wanted(&self, wanted: bool) -> bool {
        if self.is_failed() {
            return false;
        }
        self.wanted.store(wanted, Ordering::Release);
        true
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Subscription>> {
        // A panic while holding the lock cannot leave the list inconsistent.
        self.subscriptions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Forwards a mod's message to `tracing` with the mod id as a field.
    pub fn log(&self, level: UmiLevel, bytes: &[u8]) {
        let text = log_text(bytes);
        let mod_id = &self.id;
        match level {
            UmiLevel::ERROR => tracing::error!(%mod_id, "{text}"),
            UmiLevel::WARN => tracing::warn!(%mod_id, "{text}"),
            UmiLevel::DEBUG => tracing::debug!(%mod_id, "{text}"),
            UmiLevel::TRACE => tracing::trace!(%mod_id, "{text}"),
            // INFO and any level a newer mod invents.
            _ => tracing::info!(%mod_id, "{text}"),
        }
    }

    /// Records a handler.
    ///
    /// Refuses with `INVALID_ARGUMENT` a null handler, an unknown event or an
    /// unknown mode, and with `DENIED` a packet event without the `network_packets`
    /// permission or any call after `init` returned: the dispatcher reads the lists
    /// without coordinating with late registrations.
    pub fn subscribe(
        &self,
        event: UmiEventId,
        mode: UmiMode,
        priority: i32,
        handler: Option<UmiHandlerFn>,
        user: UserPtr,
    ) -> UmiStatus {
        let known_event = is_known(event);
        let known_mode = matches!(mode, UmiMode::OBSERVE | UmiMode::INTERCEPT);
        let Some(handler) = handler.filter(|_| known_event && known_mode) else {
            return UmiStatus::INVALID_ARGUMENT;
        };
        if is_packet_event(event) && !self.permissions.network_packets {
            return UmiStatus::DENIED;
        }
        if self.init_done.load(Ordering::Acquire) {
            return UmiStatus::DENIED;
        }
        self.lock().push(Subscription {
            owner: self.id.clone(),
            event,
            mode,
            priority,
            handler,
            user,
        });
        UmiStatus::OK
    }

    /// Registers the mod's `player_physics` layer.
    ///
    /// Refuses with `INVALID_ARGUMENT` a null function or an unknown mode, with
    /// `DENIED` a mod without the `movement` permission, one whose `mod.toml` does
    /// not list `player_physics` under `wraps` (for `WRAP`) or `replaces` (for
    /// `REPLACE`), or any call after `init` returned, and with `CONFLICT` a second
    /// layer: a mod has one.
    pub fn register_physics(
        &self,
        mode: UmiServiceMode,
        step: Option<UmiPhysicsFn>,
        user: UserPtr,
    ) -> UmiStatus {
        self.physics.register(mode, step, user)
    }

    /// Closes the registration window once `umi_mod_init` returned. On success the
    /// mod becomes enabled and owes a `umi_mod_shutdown`; on failure it is marked
    /// failed and its partial registrations, commands included, are dropped.
    pub fn finish_init(&self, succeeded: bool) {
        self.init_done.store(true, Ordering::Release);
        self.physics.seal(succeeded);
        let withdrawn = self.ui.seal(succeeded);
        if succeeded {
            self.shutdown_due.store(true, Ordering::Release);
            self.wanted.store(true, Ordering::Release);
        } else {
            self.failed.store(true, Ordering::Release);
            self.lock().clear();
            self.shared.remove_commands(&withdrawn);
        }
    }

    /// Fails the mod because it misbehaved and logs `reason` once, however many
    /// handlers fail at the same time. The dispatcher skips it from now on.
    pub(crate) fn fail(&self, reason: &str) {
        if !self.failed.swap(true, Ordering::AcqRel) {
            let mod_id = &self.id;
            tracing::error!(%mod_id, "mod disabled: {reason}");
        }
    }

    /// Marks the mod as finished for good, silently: used at exit so nothing
    /// re-enables it while it shuts down.
    pub(crate) fn retire(&self) {
        self.failed.store(true, Ordering::Release);
    }

    /// Whether `umi_mod_shutdown` is still owed; true for exactly one caller, and
    /// only if init succeeded (a mod that failed to initialise has nothing to undo).
    pub(crate) fn take_shutdown_due(&self) -> bool {
        self.shutdown_due.swap(false, Ordering::AcqRel)
    }

    /// The mod's time and drop counters, for the profiler.
    pub fn stats(&self) -> ModStats {
        self.stats.snapshot()
    }

    /// Counters of the mod's physics layer (budget [`BUDGET`](crate::BUDGET), the time
    /// inside `physics_next` excluded).
    pub fn physics_stats(&self) -> ModStats {
        self.physics.stats().snapshot()
    }

    pub(crate) fn physics_counters(&self) -> &Stats {
        self.physics.stats()
    }

    pub(crate) fn physics_layer(&self) -> Option<PhysicsLayer> {
        self.physics.layer()
    }

    pub(crate) fn counters(&self) -> &Stats {
        &self.stats
    }

    /// Counters of the widget draw functions (budget [`HUD_BUDGET`](crate::HUD_BUDGET)).
    pub fn hud_stats(&self) -> ModStats {
        self.ui.hud_stats().snapshot()
    }

    pub(crate) fn hud_counters(&self) -> &Stats {
        self.ui.hud_stats()
    }

    pub(crate) fn command_counters(&self) -> &Stats {
        self.ui.command_stats()
    }

    pub(crate) fn worker(&self) -> &WorkerSlot {
        &self.worker
    }

    /// A copy of every subscription, in registration order.
    pub(crate) fn all_subscriptions(&self) -> Vec<Subscription> {
        self.lock().clone()
    }

    /// A copy of the subscriptions to `event`, in registration order.
    pub fn subscriptions(&self, event: UmiEventId) -> Vec<Subscription> {
        self.lock()
            .iter()
            .filter(|s| s.event == event)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests;

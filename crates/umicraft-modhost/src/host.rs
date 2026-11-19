// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`ModHost`]: the loaded mods and their lifecycle.

use std::path::Path;
use std::sync::Arc;

use libloading::Library;
use umicraft_core::EventBus;
use umicraft_modapi_sys::{UmiEventId, UmiHost, UmiModShutdownFn, UMI_API_VERSION};

use crate::budget::ModStats;
use crate::dispatch::install_mod;
use crate::events::EventThread;

use crate::context::{ModContext, Subscription};
use crate::error::{LoadError, LoadFailure};
use crate::ffi::{exports_of, handle_of, host_table, open_library};
use crate::load::{authorize, locate_library};
use crate::manifest::{Manifest, ModId};
use crate::resolve::Resolution;
use crate::trust::TrustList;

/// One mod whose `umi_mod_init` ran (successfully or not).
///
/// Field order is drop order: the context (and the function pointers held in its
/// subscriptions) go before the `Library` they point into. A mod whose init failed
/// stays here, disabled, because the mod may have kept its handle or started a
/// thread; unmapping the library or freeing the context could then crash the client.
struct LoadedMod {
    context: Arc<ModContext>,
    shutdown: UmiModShutdownFn,
    _library: Library,
}

/// The native mods loaded into the client.
///
/// Libraries stay mapped until the host is dropped: a hot unload is unreliable
/// (thread-local destructors, threads the mod started), so "unload" means
/// [`ModHost::disable`] (ADR-0086). Field order is drop order: the mods (contexts,
/// then libraries) go before the function table, and nothing a mod can reach is
/// freed while the mod might still run, because [`Drop`] disables every mod first.
pub struct ModHost {
    mods: Vec<LoadedMod>,
    /// Boxed so its address, which every mod keeps, never changes.
    table: Box<UmiHost>,
}

impl ModHost {
    /// Loads every mod of `resolution`, in order; a failure never stops the others.
    ///
    /// A mod `<id>` is looked up at `mods_dir/<id>/bin/<os>-<arch>/<library>`. Its
    /// library must be trusted: hash listed in `trust`, or accepted by `confirm`
    /// (called with the manifest and the lowercase hex SHA-256), which then records it.
    /// Callers should save `trust` afterwards. Every failure is logged and returned.
    pub fn load<F>(
        resolution: &Resolution,
        mods_dir: &Path,
        trust: &mut TrustList,
        mut confirm: F,
    ) -> (Self, Vec<LoadFailure>)
    where
        F: FnMut(&Manifest, &str) -> bool,
    {
        let mut host = Self {
            mods: Vec::new(),
            table: Box::new(host_table()),
        };
        let mut failures = Vec::new();
        for manifest in &resolution.order {
            if let Err(reason) = host.load_one(manifest, mods_dir, trust, &mut confirm) {
                tracing::warn!(mod_id = %manifest.id, %reason, "mod failed to load");
                failures.push(LoadFailure {
                    id: manifest.id.clone(),
                    reason,
                });
            }
        }
        (host, failures)
    }

    fn load_one(
        &mut self,
        manifest: &Manifest,
        mods_dir: &Path,
        trust: &mut TrustList,
        confirm: &mut dyn FnMut(&Manifest, &str) -> bool,
    ) -> Result<(), LoadError> {
        let path = locate_library(manifest, mods_dir)?;
        authorize(manifest, &path, trust, confirm)?;
        let library = open_library(&path)?;
        let exports = exports_of(&library)?;
        let module = (exports.api_version)();
        if !UMI_API_VERSION.accepts(module) {
            return Err(LoadError::ApiMismatch {
                host: UMI_API_VERSION,
                module,
            });
        }
        let context = Arc::new(ModContext::new(manifest.id.clone()));
        let status = (exports.init)(&*self.table, handle_of(&context));
        context.finish_init(status.is_ok());
        self.mods.push(LoadedMod {
            context,
            shutdown: exports.shutdown,
            _library: library,
        });
        if status.is_ok() {
            Ok(())
        } else {
            // No `umi_mod_shutdown`: a mod that failed to initialise has nothing to undo.
            Err(LoadError::InitFailed(status))
        }
    }

    /// The ids of the enabled mods, in load order.
    pub fn enabled(&self) -> impl Iterator<Item = &ModId> {
        self.mods
            .iter()
            .filter(|m| m.context.is_enabled())
            .map(|m| m.context.id())
    }

    /// The handlers registered for `event` by enabled mods: ascending priority, ties
    /// in load order (ADR-0086). A snapshot, so it stays valid if a mod is disabled.
    pub fn subscriptions(&self, event: UmiEventId) -> impl Iterator<Item = Subscription> {
        let mut all: Vec<Subscription> = self
            .mods
            .iter()
            .filter(|m| m.context.is_enabled())
            .flat_map(|m| m.context.subscriptions(event))
            .collect();
        all.sort_by_key(|s| s.priority);
        all.into_iter()
    }

    /// Registers the handlers of every enabled mod for the events owned by `thread`
    /// on `bus`, which the caller dispatches on that thread only.
    ///
    /// Call once per thread after loading: the tables are built here and never
    /// change (ADR-0086). Mods in load order, so equal priorities run in load order.
    /// The first observer of a mod starts its `umicraft-mod-<id>` worker.
    pub fn install(&self, bus: &mut EventBus, thread: EventThread) {
        for loaded in &self.mods {
            install_mod(bus, thread, &loaded.context);
        }
    }

    /// The counters of every mod, in load order, for the profiler.
    pub fn stats(&self) -> Vec<(ModId, ModStats)> {
        self.mods
            .iter()
            .map(|m| (m.context.id().clone(), m.context.stats()))
            .collect()
    }

    /// Disables mod `id`: the dispatcher skips it from now on. Only a flag; its
    /// `umi_mod_shutdown` runs in [`ModHost::shutdown_all`], once nothing can still
    /// be calling into it (shutdown-quiescence rule, ADR-0086). Returns whether the
    /// mod was enabled.
    pub fn disable(&self, id: &ModId) -> bool {
        self.mods
            .iter()
            .find(|m| m.context.id() == id)
            .is_some_and(|m| m.context.disable())
    }

    /// Stops the mods for good: no handler runs afterwards.
    ///
    /// Every mod is disabled, every worker drains and is joined, and only then does
    /// each mod that initialised get `umi_mod_shutdown`, once, last loaded first so a
    /// mod can rely on its dependencies during its own shutdown. The client must
    /// already have stopped the threads that dispatch.
    pub fn shutdown_all(&mut self) {
        shutdown_mods(self.mods.iter().map(|m| (&*m.context, m.shutdown)));
    }
}

/// The ordered part of [`ModHost::shutdown_all`], on contexts and entry points so a
/// test can drive it without a library.
fn shutdown_mods<'a>(
    mods: impl DoubleEndedIterator<Item = (&'a ModContext, UmiModShutdownFn)> + Clone,
) {
    for (context, _) in mods.clone() {
        context.disable();
    }
    for (context, _) in mods.clone() {
        context.worker().stop();
    }
    for (context, shutdown) in mods.rev() {
        if context.take_shutdown_due() {
            shutdown();
        }
    }
}

impl Drop for ModHost {
    fn drop(&mut self) {
        self.shutdown_all();
    }
}

#[cfg(test)]
mod tests {
    use core::ffi::c_void;
    use std::sync::Mutex;

    use umicraft_modapi_sys::{UmiEventHeader, UmiMode, UmiStatus};

    use super::*;
    use crate::events::PreTick;
    use crate::ffi::UserPtr;

    /// Records, in order, what happened: handler runs and shutdowns.
    static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

    extern "C" fn observer(_: *mut UmiEventHeader, _: *mut c_void) -> UmiStatus {
        LOG.lock().unwrap().push("observer start".to_owned());
        std::thread::sleep(std::time::Duration::from_millis(30));
        LOG.lock().unwrap().push("observer end".to_owned());
        UmiStatus::OK
    }

    extern "C" fn shutdown() {
        LOG.lock().unwrap().push("shutdown".to_owned());
    }

    fn started(id: &str) -> Arc<ModContext> {
        let cx = Arc::new(ModContext::new(id.to_owned().try_into().unwrap()));
        let status = cx.subscribe(
            UmiEventId::PRE_TICK,
            UmiMode::OBSERVE,
            0,
            Some(observer),
            UserPtr::new(std::ptr::null_mut()),
        );
        assert_eq!(status, UmiStatus::OK);
        cx.finish_init(true);
        cx
    }

    #[test]
    fn shutdown_joins_workers_before_calling_shutdown_once() {
        let cx = started("order");
        let mut bus = EventBus::default();
        install_mod(&mut bus, EventThread::Game, &cx);
        bus.dispatch(&mut PreTick::new(1));
        while !LOG.lock().unwrap().iter().any(|e| e == "observer start") {
            std::thread::yield_now();
        }

        let entries = [(&*cx, shutdown as UmiModShutdownFn)];
        shutdown_mods(entries.iter().copied());
        shutdown_mods(entries.iter().copied());

        // The running handler finished before `shutdown`, which ran once.
        let log = LOG.lock().unwrap();
        assert_eq!(*log, ["observer start", "observer end", "shutdown"]);
        assert!(!cx.is_enabled());
    }
}

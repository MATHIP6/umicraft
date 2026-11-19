//! [`ModHost`]: the loaded mods and their lifecycle.

use std::path::Path;
use std::sync::Arc;

use libloading::Library;
use umicraft_modapi_sys::{UmiEventId, UmiHost, UmiModShutdownFn, UMI_API_VERSION};

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
            .flat_map(|m| m.context.subscriptions(event))
            .collect();
        all.sort_by_key(|s| s.priority);
        all.into_iter()
    }

    /// Disables mod `id`: calls `umi_mod_shutdown` once and drops its subscriptions.
    /// The library stays mapped. Returns whether the mod was enabled.
    pub fn disable(&mut self, id: &ModId) -> bool {
        let Some(loaded) = self.mods.iter().find(|m| m.context.id() == id) else {
            return false;
        };
        let was_enabled = loaded.context.disable();
        if was_enabled {
            (loaded.shutdown)();
        }
        was_enabled
    }

    /// Disables every enabled mod, last loaded first, so a mod can rely on its
    /// dependencies still running during its own shutdown.
    pub fn shutdown_all(&mut self) {
        for loaded in self.mods.iter().rev() {
            if loaded.context.disable() {
                (loaded.shutdown)();
            }
        }
    }
}

impl Drop for ModHost {
    fn drop(&mut self) {
        self.shutdown_all();
    }
}

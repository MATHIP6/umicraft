// SPDX-License-Identifier: MIT OR Apache-2.0

//! The UI half of a mod's context: declarations, retained config and hook counters.

use std::mem;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use umicraft_modapi_sys::UmiSettingValue;

use super::config::ExternalModConfig;
use super::declared::Declared;
use super::settings::SettingSlot;
use crate::budget::Stats;
use crate::error::DeclareError;

/// What `finish_init` shows to readers before a mod has been sealed.
static EMPTY: Declared = Declared::new();

/// Declarations are collected in a draft while `umi_mod_init` runs, then sealed into
/// a `OnceLock`: readers on any thread (`setting_get` from a worker, the render
/// thread's draw loop) reach the sealed lists without taking a lock.
#[derive(Debug, Default)]
pub(crate) struct UiState {
    draft: Mutex<Declared>,
    sealed: OnceLock<Declared>,
    /// Config entries no declaration claims (a setting a newer version of the mod
    /// has, say), kept so saving the config does not lose them.
    retained: Mutex<ExternalModConfig>,
    hud_stats: Stats,
    command_stats: Stats,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic while holding the lock cannot leave these lists inconsistent.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl UiState {
    /// Runs `declare` on the draft; refused with [`DeclareError::Late`] once sealed.
    ///
    /// The check and the change happen under the draft's lock, which [`seal`] also
    /// takes, so a registration cannot slip in after `umi_mod_init` returned.
    ///
    /// [`seal`]: Self::seal
    pub(crate) fn register<R>(
        &self,
        declare: impl FnOnce(&mut Declared) -> Result<R, DeclareError>,
    ) -> Result<R, DeclareError> {
        let mut draft = lock(&self.draft);
        if self.sealed.get().is_some() {
            return Err(DeclareError::Late);
        }
        declare(&mut draft)
    }

    /// Freezes the declarations. A failed mod is sealed empty; the names of the
    /// commands it had put in the shared table are returned so the caller can
    /// withdraw them.
    pub(crate) fn seal(&self, keep: bool) -> Vec<String> {
        let mut draft = lock(&self.draft);
        let declared = mem::take(&mut *draft);
        let (sealed, withdrawn) = if keep {
            (declared, Vec::new())
        } else {
            (Declared::new(), declared.commands().to_vec())
        };
        // Only the first seal counts; a second `finish_init` is a caller bug.
        let _ = self.sealed.set(sealed);
        withdrawn
    }

    /// Whether `umi_mod_init` has returned.
    pub(crate) fn is_sealed(&self) -> bool {
        self.sealed.get().is_some()
    }

    /// The sealed declarations; empty before `finish_init`.
    pub(crate) fn declared(&self) -> &Declared {
        self.sealed.get().unwrap_or(&EMPTY)
    }

    /// The current value of setting `key`. Lock-free once sealed; during
    /// `umi_mod_init` it reads the draft, so a mod may read what it just declared.
    pub(crate) fn setting_value(&self, key: &str) -> Option<UmiSettingValue> {
        match self.sealed.get() {
            Some(declared) => declared.setting(key).map(SettingSlot::abi_value),
            None => lock(&self.draft).setting(key).map(SettingSlot::abi_value),
        }
    }

    pub(crate) fn retained(&self) -> MutexGuard<'_, ExternalModConfig> {
        lock(&self.retained)
    }

    pub(crate) fn hud_stats(&self) -> &Stats {
        &self.hud_stats
    }

    pub(crate) fn command_stats(&self) -> &Stats {
        &self.command_stats
    }
}

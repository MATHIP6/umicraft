// SPDX-License-Identifier: MIT OR Apache-2.0

//! External native mods (ADR-0086): discovery, trust, loading and the crash sentinel.
//!
//! Mods are optional, so nothing here is fatal: every I/O or load problem is logged
//! and the client carries on without the mod. The sentinel file exists exactly while
//! the client runs; finding it at start means the previous run crashed, possibly
//! inside a mod, so the next run starts in safe mode (spec §8.6).

use std::path::{Path, PathBuf};

use crossbeam_channel::Receiver;
use umicraft_core::events::EventBus;
use umicraft_modapi_sys::UMI_API_VERSION;
use umicraft_modhost::{
    discover, resolve, ApiVersion, Candidate, CommandTable, CrashSentinel, Discovered, EventThread,
    ModHost, ModId, PacketOutbox, SentinelGuard, TrustList,
};

use super::external_config;

/// Folder of the installed mods, inside the data directory.
const MODS_FOLDER: &str = "mods";
/// The trust list, inside the mods folder.
const TRUST_FILE: &str = "trusted.toml";
/// The crash sentinel, inside the data directory.
const SENTINEL_FILE: &str = "umicraft.running";

fn mods_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(MODS_FOLDER)
}

fn trust_path(data_dir: &Path) -> PathBuf {
    mods_dir(data_dir).join(TRUST_FILE)
}

fn sentinel_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SENTINEL_FILE)
}

/// Why no external mod is loaded in this run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SafeModeReason {
    /// The player passed `--safe-mode`.
    Flag,
    /// The previous run left the crash sentinel behind.
    PreviousCrash,
}

/// Decides whether this run is in safe mode; the flag is reported first because it
/// is the player's explicit request.
fn safe_mode_reason(flag: bool, crashed: bool) -> Option<SafeModeReason> {
    if flag {
        Some(SafeModeReason::Flag)
    } else if crashed {
        Some(SafeModeReason::PreviousCrash)
    } else {
        None
    }
}

/// The external mods of a run and the sentinel that guards it.
///
/// Holds a [`ModHost`], which is not `Send`: keep it on the main thread. There is
/// deliberately no `Drop`: only [`ExternalMods::shutdown`] disarms the sentinel.
pub(crate) struct ExternalMods {
    host: Option<ModHost>,
    sentinel: Option<SentinelGuard>,
    /// Where each mod's choices are saved; `None` when the options folder is unknown.
    config_dir: Option<PathBuf>,
    /// The local chat lines the mods print, read each frame by the window.
    outbox: Option<Receiver<String>>,
}

impl ExternalMods {
    /// Arms the sentinel and, outside safe mode, loads every installed mod.
    ///
    /// `data_dir` is the Umicraft data directory and `config_dir` the folder of the
    /// per-mod choice files (see [`external_config`]); the saved choices, switch
    /// included, are applied right after loading, so a mod the player switched off
    /// still loads and can be switched back on. The trust confirmation dialog arrives
    /// with F3.8: an untrusted library is refused with instructions in the log.
    pub(crate) fn start(
        data_dir: &Path,
        config_dir: Option<PathBuf>,
        safe_mode_flag: bool,
    ) -> Self {
        let sentinel_file = sentinel_path(data_dir);
        let reason = safe_mode_reason(safe_mode_flag, CrashSentinel::armed(&sentinel_file));
        // Armed in every case, so a crash in this run is detected at the next start.
        let sentinel = arm_sentinel(&sentinel_file);
        let host = match reason {
            Some(reason) => {
                log_safe_mode(reason);
                None
            }
            None => Some(load_mods(data_dir)),
        };
        if let (Some(host), Some(dir)) = (&host, &config_dir) {
            external_config::load_all(host, dir);
        }
        let outbox = host.as_ref().map(ModHost::chat_outbox);
        Self {
            host,
            sentinel,
            config_dir,
            outbox,
        }
    }

    /// A bus with the subscriptions of the mods that live on `thread`.
    ///
    /// Empty in safe mode, so an emitter that checks `has_subscribers` pays
    /// nothing. Build one per thread and move it there: a bus is dispatched on
    /// one thread only.
    pub(crate) fn bus(&self, thread: EventThread) -> EventBus {
        let mut bus = EventBus::default();
        if let Some(host) = &self.host {
            host.install(&mut bus, thread);
        }
        bus
    }

    /// The packets the mods injected, for the network thread of a new session;
    /// `None` in safe mode.
    pub(crate) fn packet_outbox(&self) -> Option<PacketOutbox> {
        self.host.as_ref().map(ModHost::packet_outbox)
    }

    /// Turns the server-compatible mode on or off: while on, the mods that alter
    /// packets or movement stay off (ADR-0086).
    pub(crate) fn set_server_compat(&self, on: bool) {
        if let Some(host) = &self.host {
            host.set_server_compat(on);
        }
    }

    /// Shuts every mod down, then disarms the sentinel. Call on a clean exit only,
    /// after every thread that dispatches to a mod has stopped and every bus built
    /// by [`ExternalMods::bus`] is dropped (shutdown-quiescence rule, ADR-0086).
    pub(crate) fn shutdown(mut self) {
        if let Some(host) = &mut self.host {
            host.shutdown_all();
        }
        // The mods must be unmapped before the sentinel goes: a crash while they
        // unload still counts as an unclean exit.
        drop(self.host.take());
        if let Some(Err(error)) = self.sentinel.take().map(SentinelGuard::disarm) {
            tracing::warn!(%error, "cannot remove the crash sentinel; the next start will use safe mode");
        }
    }

    /// The loaded host, `None` in safe mode.
    pub(crate) fn host(&self) -> Option<&ModHost> {
        self.host.as_ref()
    }

    /// The command table for the game thread of a session, `None` in safe mode.
    pub(crate) fn commands(&self) -> Option<CommandTable> {
        self.host.as_ref().map(ModHost::commands)
    }

    /// Takes the local chat lines the mods printed since the last call.
    pub(crate) fn drain_chat(&self) -> impl Iterator<Item = String> + '_ {
        self.outbox.iter().flat_map(Receiver::try_iter)
    }

    /// The names of the mods' commands that start with `partial`, for tab completion.
    pub(crate) fn complete_commands(&self, partial: &str) -> Vec<String> {
        self.host
            .as_ref()
            .map_or_else(Vec::new, |host| host.commands().complete(partial))
    }

    /// Saves the choices of mod `id`; a failure is logged.
    pub(crate) fn save_config(&self, id: &ModId) {
        if let (Some(host), Some(dir)) = (&self.host, &self.config_dir) {
            external_config::save(host, dir, id);
        }
    }
}

fn arm_sentinel(path: &Path) -> Option<SentinelGuard> {
    match CrashSentinel::arm(path) {
        Ok(guard) => Some(guard),
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "cannot arm the crash sentinel");
            None
        }
    }
}

fn log_safe_mode(reason: SafeModeReason) {
    match reason {
        SafeModeReason::Flag => {
            tracing::warn!("safe mode (--safe-mode): no external mod is loaded")
        }
        SafeModeReason::PreviousCrash => tracing::warn!(
            "the previous run did not exit cleanly: safe mode, no external mod is loaded \
             (remove the crashed mod or restart to load them again)"
        ),
    }
}

/// Discovers, resolves and loads the mods under `<data_dir>/mods`.
fn load_mods(data_dir: &Path) -> ModHost {
    let mods_dir = mods_dir(data_dir);
    let builtin = builtin_ids();
    let candidates = discover(&mods_dir)
        .into_iter()
        .filter_map(log_discovery)
        .filter(|candidate| !clashes_with_builtin(candidate, &builtin));
    let host_api = ApiVersion {
        major: UMI_API_VERSION.major,
        minor: UMI_API_VERSION.minor,
    };
    let resolution = resolve(candidates, host_api);
    for rejection in &resolution.rejected {
        tracing::warn!(mod_id = %rejection.id, reason = %rejection.reason, "mod not loaded");
    }
    let trust_path = trust_path(data_dir);
    let mut trust = read_trust(&trust_path);
    let before = trust.clone();
    // The confirmation dialog is F3.8; until then trust is granted by editing the file.
    let (host, failures) = ModHost::load(&resolution, &mods_dir, &mut trust, |manifest, sha256| {
        tracing::warn!(
            mod_id = %manifest.id,
            sha256,
            "untrusted mod library not loaded; to trust it add `{} = \"{sha256}\"` under \
             `[trusted]` in {}",
            manifest.id,
            trust_path.display()
        );
        false
    });
    for failure in &failures {
        tracing::warn!(mod_id = %failure.id, reason = %failure.reason, "mod failed to load");
    }
    if trust != before {
        if let Err(error) = trust.save(&trust_path) {
            tracing::warn!(%error, "cannot save the trust list");
        }
    }
    tracing::info!(count = host.enabled().count(), "external mods loaded");
    host
}

/// The ids of the built-in mods, which an external mod may not take: both would
/// claim the same `[mods.<id>]` table and the same menu entry.
fn builtin_ids() -> Vec<&'static str> {
    umicraft_builtin_mods::mods::all()
        .iter()
        .map(|builtin| builtin.meta().id.as_str())
        .collect()
}

/// Whether `candidate` has the id of a built-in mod; it is then refused, with a
/// warning that names both sides of the clash.
fn clashes_with_builtin(candidate: &Candidate, builtin: &[&str]) -> bool {
    let id = candidate.manifest.id.as_str();
    let clash = builtin.contains(&id);
    if clash {
        tracing::warn!(
            mod_id = id,
            "external mod not loaded: its id is the one of a built-in mod, rename it in mod.toml"
        );
    }
    clash
}

/// Turns a discovery result into a candidate, logging a manifest that cannot be used.
fn log_discovery(found: Discovered) -> Option<Candidate> {
    match found.manifest {
        Ok(manifest) => Some(Candidate {
            manifest,
            enabled: true,
        }),
        Err(error) => {
            tracing::warn!(dir = %found.dir.display(), %error, "mod skipped");
            None
        }
    }
}

/// The trust list, or an empty one (so nothing unknown loads) if it cannot be read.
fn read_trust(path: &Path) -> TrustList {
    TrustList::load(path).unwrap_or_else(|error| {
        tracing::warn!(%error, path = %path.display(), "cannot read the trust list; trusting nothing");
        TrustList::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("umi-external-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn safe_mode_reason_prefers_the_flag() {
        assert_eq!(safe_mode_reason(false, false), None);
        assert_eq!(safe_mode_reason(true, false), Some(SafeModeReason::Flag));
        assert_eq!(
            safe_mode_reason(false, true),
            Some(SafeModeReason::PreviousCrash)
        );
        assert_eq!(safe_mode_reason(true, true), Some(SafeModeReason::Flag));
    }

    #[test]
    fn a_clean_run_arms_the_sentinel_and_shutdown_removes_it() {
        let dir = scratch("clean");
        let mods = ExternalMods::start(&dir, None, false);
        assert!(mods.host().is_some());
        assert!(CrashSentinel::armed(&sentinel_path(&dir)));
        mods.shutdown();
        assert!(!CrashSentinel::armed(&sentinel_path(&dir)));
    }

    #[test]
    fn a_leftover_sentinel_means_safe_mode_and_stays_armed() {
        let dir = scratch("crashed");
        std::fs::write(sentinel_path(&dir), "1").expect("leftover sentinel");
        let mods = ExternalMods::start(&dir, None, false);
        assert!(mods.host().is_none());
        assert!(CrashSentinel::armed(&sentinel_path(&dir)));
        drop(mods);
        assert!(
            CrashSentinel::armed(&sentinel_path(&dir)),
            "dropping must not disarm"
        );
    }

    #[test]
    fn a_run_without_mods_builds_empty_buses() {
        let dir = scratch("buses");
        let mods = ExternalMods::start(&dir, None, true);
        for thread in [EventThread::Game, EventThread::Render] {
            let bus = mods.bus(thread);
            assert!(!bus.has_subscribers::<umicraft_modhost::PreTick>());
        }
        mods.shutdown();
    }

    fn candidate(id: &str) -> Candidate {
        let text = format!(
            "[mod]\nid = \"{id}\"\nname = \"X\"\nversion = \"1.0.0\"\nauthors = []\napi_version = \"0.3\"\n"
        );
        Candidate {
            manifest: umicraft_modhost::Manifest::parse(&text).expect("a valid manifest"),
            enabled: true,
        }
    }

    #[test]
    fn a_mod_with_a_builtin_id_is_refused_and_others_are_kept() {
        let builtin = builtin_ids();
        assert!(builtin.contains(&"fps"), "the list comes from the registry");
        assert!(clashes_with_builtin(&candidate("fps"), &builtin));
        assert!(!clashes_with_builtin(&candidate("hello_hud"), &builtin));
    }

    #[test]
    fn a_run_without_mods_has_no_commands_and_no_chat() {
        let dir = scratch("commands");
        let mods = ExternalMods::start(&dir, None, true);
        assert!(mods.commands().is_none());
        assert_eq!(mods.drain_chat().count(), 0);
        mods.shutdown();
    }

    #[test]
    fn the_flag_forces_safe_mode() {
        let dir = scratch("flag");
        let mods = ExternalMods::start(&dir, None, true);
        assert!(mods.host().is_none());
        mods.shutdown();
        assert!(!CrashSentinel::armed(&sentinel_path(&dir)));
    }
}

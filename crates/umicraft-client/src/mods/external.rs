// SPDX-License-Identifier: MIT OR Apache-2.0

//! External native mods (ADR-0086): discovery, trust, loading and the crash sentinel.
//!
//! Mods are optional, so nothing here is fatal: every I/O or load problem is logged
//! and the client carries on without the mod. The sentinel file exists exactly while
//! the client runs; finding it at start means the previous run crashed, possibly
//! inside a mod, so the next run starts in safe mode (spec §8.6).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crossbeam_channel::Receiver;
use umicraft_core::events::EventBus;
use umicraft_modapi_sys::UMI_API_VERSION;
use umicraft_modhost::{
    discover, resolve, ApiVersion, Arming, CameraChain, Candidate, CommandTable, CrashSentinel,
    Discovered, EventThread, InputRequest, LoadError, LoadFailure, ModHost, ModId, PacketOutbox,
    Permissions, PhysicsChain, SentinelGuard, TrustList,
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

/// A mod whose library is not on the trust list: it was not loaded, and the mods menu
/// offers to trust it (F3.8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UntrustedMod {
    /// The mod's id.
    pub(crate) id: ModId,
    /// Display name from its manifest.
    pub(crate) name: String,
    /// Its version, as written in the manifest.
    pub(crate) version: String,
    /// What the manifest asks for, shown before the player trusts the file.
    pub(crate) permissions: Permissions,
    /// Lowercase hex SHA-256 of the library: what trusting records.
    pub(crate) hash: String,
    /// Whether the player trusted it in this run; it then loads at the next start.
    pub(crate) trusted: bool,
}

/// The untrusted mod behind a failed load, if that is why it failed. A failure
/// without a manifest cannot be shown, so it stays a log line.
fn untrusted_of(failure: &LoadFailure) -> Option<UntrustedMod> {
    let (LoadError::Untrusted { hash }, Some(manifest)) = (&failure.reason, &failure.manifest)
    else {
        return None;
    };
    Some(UntrustedMod {
        id: failure.id.clone(),
        name: manifest.name.clone(),
        version: manifest.version.to_string(),
        permissions: manifest.permissions,
        hash: hash.clone(),
        trusted: false,
    })
}

/// What [`load_mods`] produced.
struct Loaded {
    host: ModHost,
    untrusted: Vec<UntrustedMod>,
    trust: TrustList,
}

/// The ids in `mods` that are failed and not in `seen` yet, as their display names;
/// they are added to `seen`, so each failure is reported once.
///
/// Items are `(id, name, failed)`: the pure core of the "stopped working" notice,
/// which covers every kind of failure (a command, a handler, a widget, a budget).
fn newly_failed<'a>(
    seen: &mut HashSet<ModId>,
    mods: impl IntoIterator<Item = (&'a ModId, &'a str, bool)>,
) -> Vec<String> {
    mods.into_iter()
        .filter(|(id, _, failed)| *failed && seen.insert((*id).clone()))
        .map(|(_, name, _)| name.to_owned())
        .collect()
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
    /// The libraries refused for lack of trust, for the mods menu.
    untrusted: Vec<UntrustedMod>,
    /// The trust list as loaded, plus what the player trusted in this run.
    trust: TrustList,
    /// Where the trust list is saved.
    trust_path: PathBuf,
    /// The mods already reported as stopped, so each gets one notice.
    reported_failed: HashSet<ModId>,
}

impl ExternalMods {
    /// Arms the sentinel and, outside safe mode, loads every installed mod.
    ///
    /// `data_dir` is the Umicraft data directory and `config_dir` the folder of the
    /// per-mod choice files (see [`external_config`]); the saved choices, switch
    /// included, are applied right after loading, so a mod the player switched off
    /// still loads and can be switched back on. An untrusted library is refused; the
    /// mods menu lists it with its hash and can trust it for the next start.
    pub(crate) fn start(
        data_dir: &Path,
        config_dir: Option<PathBuf>,
        safe_mode_flag: bool,
    ) -> Self {
        // Armed in every case, so a crash in this run is detected at the next start.
        let (sentinel, crashed) = arm_sentinel(&sentinel_path(data_dir));
        let reason = safe_mode_reason(safe_mode_flag, crashed);
        let loaded = match reason {
            Some(reason) => {
                log_safe_mode(reason);
                None
            }
            None => Some(load_mods(data_dir)),
        };
        let (host, untrusted, trust) = match loaded {
            Some(Loaded {
                host,
                untrusted,
                trust,
            }) => (Some(host), untrusted, trust),
            None => (None, Vec::new(), TrustList::default()),
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
            untrusted,
            trust,
            trust_path: trust_path(data_dir),
            reported_failed: HashSet::new(),
        }
    }

    /// The libraries that were refused for lack of trust.
    pub(crate) fn untrusted(&self) -> &[UntrustedMod] {
        &self.untrusted
    }

    /// Trusts the library of the untrusted mod `mod_id` and saves the trust list, so
    /// the mod loads at the next start; the library was not loaded in this run and
    /// loading it now would skip the restart that the host's checks assume.
    ///
    /// Trust is one way here: the host has no "forget this hash", and an off switch
    /// would have to edit `trusted.toml` behind a file the player may have changed.
    /// An unknown id, or one already trusted, does nothing; a save failure is logged
    /// and leaves the mod untrusted.
    pub(crate) fn trust(&mut self, mod_id: &str) {
        let Some(entry) = self
            .untrusted
            .iter_mut()
            .find(|entry| entry.id.as_str() == mod_id && !entry.trusted)
        else {
            return;
        };
        let mut trust = self.trust.clone();
        trust.trust(&entry.id, &entry.hash);
        match trust.save(&self.trust_path) {
            Ok(()) => {
                self.trust = trust;
                entry.trusted = true;
            }
            Err(error) => tracing::warn!(%error, mod_id, "cannot save the trust list"),
        }
    }

    /// The names of the mods that stopped working since the last call, each once.
    pub(crate) fn take_stopped(&mut self) -> Vec<String> {
        let Some(host) = &self.host else {
            return Vec::new();
        };
        let views = host.mods();
        newly_failed(
            &mut self.reported_failed,
            views.map(|view| (view.id, view.name, view.failed)),
        )
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

    /// The mods' `player_physics` layers for the game thread of a new session;
    /// `None` in safe mode or when no mod registered one.
    pub(crate) fn physics(&self) -> Option<PhysicsChain> {
        self.host.as_ref().and_then(ModHost::physics)
    }

    /// The mods' `render_camera` layers for the render thread; `None` in safe mode
    /// or when no mod registered one (API 1.1).
    pub(crate) fn camera(&self) -> Option<CameraChain> {
        self.host.as_ref().and_then(ModHost::camera)
    }

    /// Tells the mods whether the game owns the input (API 1.5): while it does not,
    /// their input requests are refused.
    pub(crate) fn set_game_focus(&self, on: bool) {
        if let Some(host) = &self.host {
            host.set_game_focus(on);
        }
    }

    /// Hands the input requests the mods queued to `apply`, oldest first.
    pub(crate) fn drain_input(&self, apply: impl FnMut(InputRequest)) {
        if let Some(host) = &self.host {
            host.input_queue().drain(apply);
        }
    }

    /// Drops the input requests the mods queued: the game lost the input first.
    pub(crate) fn clear_input(&self) {
        if let Some(host) = &self.host {
            host.input_queue().clear();
        }
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

/// Arms the sentinel of this instance; returns its guard (none when another instance
/// holds it or the file cannot be written) and whether the last run crashed.
fn arm_sentinel(path: &Path) -> (Option<SentinelGuard>, bool) {
    match CrashSentinel::arm(path) {
        Ok(Arming::Armed {
            guard,
            previous_crash,
        }) => (Some(guard), previous_crash),
        Ok(Arming::AnotherInstance) => {
            tracing::info!(
                "another Umicraft instance is running: this one keeps no crash sentinel"
            );
            (None, false)
        }
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "cannot arm the crash sentinel");
            (None, false)
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
fn load_mods(data_dir: &Path) -> Loaded {
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
    // Nothing is trusted here: the mods menu lists an untrusted library and trusts it
    // for the next start, or the player edits the file.
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
    tracing::info!(mods = %loaded_list(&host), "external mods loaded");
    Loaded {
        host,
        untrusted: failures.iter().filter_map(untrusted_of).collect(),
        trust,
    }
}

/// `id version` of every loaded mod, comma separated; `none` when nothing loaded.
fn loaded_list(host: &ModHost) -> String {
    let list: Vec<String> = host
        .mods()
        .map(|view| format!("{} {}", view.id, view.version))
        .collect();
    if list.is_empty() {
        "none".to_owned()
    } else {
        list.join(", ")
    }
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
        assert!(sentinel_path(&dir).exists());
        mods.shutdown();
        assert!(!sentinel_path(&dir).exists());
    }

    #[test]
    fn a_leftover_sentinel_means_safe_mode_and_stays_armed() {
        let dir = scratch("crashed");
        std::fs::write(sentinel_path(&dir), "1").expect("leftover sentinel");
        let mods = ExternalMods::start(&dir, None, false);
        assert!(mods.host().is_none());
        assert!(sentinel_path(&dir).exists());
        drop(mods);
        assert!(sentinel_path(&dir).exists(), "dropping must not disarm");
    }

    /// Regression: a second client started while the first ran saw its sentinel,
    /// started in safe mode, and the first one's clean exit removed the file the
    /// second relied on (F3.2 known limit).
    #[test]
    fn a_second_instance_is_not_a_crash_and_leaves_the_first_ones_sentinel() {
        let dir = scratch("second");
        let first = ExternalMods::start(&dir, None, false);
        let second = ExternalMods::start(&dir, None, false);
        assert!(
            second.host().is_some(),
            "no safe mode for the second instance"
        );
        second.shutdown();
        assert!(
            sentinel_path(&dir).exists(),
            "the first instance still runs"
        );
        first.shutdown();
        assert!(!sentinel_path(&dir).exists());
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

    fn untrusted_failure(manifest: Option<umicraft_modhost::Manifest>) -> LoadFailure {
        LoadFailure {
            id: "hello".parse().expect("valid id"),
            reason: LoadError::Untrusted {
                hash: "ab".repeat(32),
            },
            manifest,
        }
    }

    #[test]
    fn an_untrusted_failure_with_a_manifest_becomes_a_manager_entry() {
        let manifest = candidate("hello").manifest;
        let entry = untrusted_of(&untrusted_failure(Some(manifest))).expect("an entry");
        assert_eq!(entry.id.as_str(), "hello");
        assert_eq!(entry.version, "1.0.0");
        assert_eq!(entry.hash, "ab".repeat(32));
        assert!(!entry.trusted);
        assert_eq!(untrusted_of(&untrusted_failure(None)), None);
    }

    fn with_untrusted(dir: &Path) -> ExternalMods {
        let entry =
            untrusted_of(&untrusted_failure(Some(candidate("hello").manifest))).expect("an entry");
        ExternalMods {
            host: None,
            sentinel: None,
            config_dir: None,
            outbox: None,
            untrusted: vec![entry],
            trust: TrustList::default(),
            trust_path: trust_path(dir),
            reported_failed: HashSet::new(),
        }
    }

    #[test]
    fn trusting_writes_the_hash_to_the_trust_file_once() {
        let dir = scratch("trust");
        let mut mods = with_untrusted(&dir);
        mods.trust("nobody");
        assert!(!trust_path(&dir).exists(), "an unknown id writes nothing");
        mods.trust("hello");
        assert!(mods.untrusted()[0].trusted);
        let saved = TrustList::load(&trust_path(&dir)).expect("a readable list");
        let id: ModId = "hello".parse().expect("valid id");
        assert!(saved.is_trusted(&id, &"ab".repeat(32)));
    }

    #[test]
    fn a_failed_save_leaves_the_mod_untrusted() {
        let dir = scratch("trust-fail");
        let mut mods = with_untrusted(&dir);
        // A directory where the file must go makes the rename fail.
        std::fs::create_dir_all(trust_path(&dir)).expect("blocking dir");
        mods.trust("hello");
        assert!(!mods.untrusted()[0].trusted);
    }

    #[test]
    fn a_failure_is_reported_once_whatever_its_kind() {
        let one: ModId = "one".parse().expect("valid id");
        let two: ModId = "two".parse().expect("valid id");
        let mut seen = HashSet::new();
        let views = |two_failed| [(&one, "One", true), (&two, "Two", two_failed)];
        assert_eq!(newly_failed(&mut seen, views(false)), ["One"]);
        assert!(newly_failed(&mut seen, views(false)).is_empty());
        assert_eq!(newly_failed(&mut seen, views(true)), ["Two"]);
        assert!(newly_failed(&mut seen, views(true)).is_empty());
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
        assert!(!sentinel_path(&dir).exists());
    }
}

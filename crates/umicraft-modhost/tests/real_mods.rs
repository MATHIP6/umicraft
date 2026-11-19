// SPDX-License-Identifier: MIT OR Apache-2.0

//! Loads the real `hello` and `panicking` example cdylibs (F3.2 acceptance test) and
//! dispatches events to them through a core `EventBus` (F3.3).
//!
//! The libraries are built once per test binary with `cargo build --offline`. The
//! target directory is `CARGO_TARGET_DIR` when set, otherwise it is derived from the
//! test executable's location (`<target>/<profile>/deps/<test>`): that stays correct
//! for custom profiles and avoids parsing `cargo metadata` output.
#![cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use umicraft_builtin_mods::SettingValue;
use umicraft_core::EventBus;
use umicraft_modapi_sys::{UmiEventId, UmiMode, UmiStatus};
use umicraft_modhost::{
    discover, library_file_name, platform_dir, resolve, ApiVersion, Arch, Candidate, ChatSend,
    CommandOutcome, EventThread, KeyPressed, LoadError, ModHost, ModId, Os, PostTick, PreTick,
    TrustList,
};

/// The directory holding the built example libraries (`<target>/<profile>`).
fn built_libraries() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let status = Command::new(env!("CARGO"))
            .args([
                "build",
                "-p",
                "umicraft-example-hello",
                "-p",
                "umicraft-example-panicking",
                "--offline",
            ])
            .status()
            .unwrap();
        assert!(status.success(), "building the example mods failed");
        profile_dir()
    })
}

/// `<target>/debug`, where `cargo build` (dev profile) puts the cdylibs.
fn profile_dir() -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(
        || {
            let exe = std::env::current_exe().unwrap();
            // <target>/<profile>/deps/<test> -> <target>
            exe.ancestors().nth(3).unwrap().to_path_buf()
        },
        PathBuf::from,
    );
    target.join("debug")
}

/// A scratch mods directory removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        // One directory per test: the tests of this file run in parallel.
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "umi-real-mods-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Installs example `id` into `mods_dir` the way a player would.
fn install(mods_dir: &Path, id: &str) {
    let (os, arch) = (Os::current().unwrap(), Arch::current().unwrap());
    let file = library_file_name(id, os);
    let dir = mods_dir.join(id);
    let bin = dir.join("bin").join(platform_dir(os, arch));
    fs::create_dir_all(&bin).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../mods/examples/{id}"));
    fs::copy(source.join("mod.toml"), dir.join("mod.toml")).unwrap();
    fs::copy(built_libraries().join(&file), bin.join(&file)).unwrap();
}

fn id(text: &str) -> ModId {
    text.to_owned().try_into().unwrap()
}

#[test]
fn hello_loads_panicking_fails_and_handlers_run() {
    let tmp = TempDir::new();
    install(&tmp.0, "hello");
    install(&tmp.0, "panicking");

    let candidates = discover(&tmp.0).into_iter().map(|d| Candidate {
        manifest: d.manifest.unwrap(),
        enabled: true,
    });
    let resolution = resolve(candidates, ApiVersion { major: 0, minor: 3 });
    assert_eq!(resolution.order.len(), 2);

    let mut trust = TrustList::default();
    let mut asked = Vec::new();
    let (host, failures) = ModHost::load(&resolution, &tmp.0, &mut trust, |m, hash| {
        asked.push((m.id.to_string(), hash.to_owned()));
        true
    });

    // The panic is contained: only `panicking` failed, and the process is alive.
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(failures[0].id.as_str(), "panicking");
    assert!(matches!(
        failures[0].reason,
        LoadError::InitFailed(UmiStatus::PANICKED)
    ));
    let enabled: Vec<_> = host.enabled().map(ModId::as_str).collect();
    assert_eq!(enabled, ["hello"]);

    // Both libraries were put to the player, and both hashes are now trusted.
    assert_eq!(asked.len(), 2);
    for (mod_id, hash) in &asked {
        assert!(trust.is_trusted(&id(mod_id), hash));
    }

    let pre: Vec<_> = host.subscriptions(UmiEventId::PRE_TICK).collect();
    assert_eq!(pre.len(), 1);
    assert_eq!(pre[0].mode, UmiMode::OBSERVE);
    let post: Vec<_> = host.subscriptions(UmiEventId::POST_TICK).collect();
    assert_eq!(post.len(), 1);
    assert_eq!(post[0].mode, UmiMode::INTERCEPT);
    assert_eq!(post[0].priority, 10);

    // The handlers are installed on the game bus only: no render event is hooked.
    let mut game = EventBus::default();
    host.install(&mut game, EventThread::Game);
    let mut render = EventBus::default();
    host.install(&mut render, EventThread::Render);
    assert!(game.has_subscribers::<PreTick>() && game.has_subscribers::<ChatSend>());
    assert!(!render.has_subscribers::<PreTick>());

    assert!(!game.dispatch(&mut PreTick::new(7)));
    assert!(!game.dispatch(&mut PostTick::new(7)));
    let mut chat = ChatSend::new("hi :shrug:");
    assert!(!game.dispatch(&mut chat));
    assert_eq!(
        chat.text(),
        "hi \u{af}\\_(\u{30c4})_/\u{af}",
        "hello rewrote the line"
    );
    let stats = host.stats();
    let hello = &stats
        .iter()
        .find(|(id, _)| id.as_str() == "hello")
        .unwrap()
        .1;
    assert_eq!(hello.calls, 2, "the post-tick and the chat interceptors");

    // Switching off is a flag: the mod stops being dispatched to, and can come back.
    host.set_enabled(&id("hello"), false).unwrap();
    let mut untouched = ChatSend::new(":shrug:");
    game.dispatch(&mut untouched);
    assert_eq!(untouched.text(), ":shrug:");
    assert_eq!(host.subscriptions(UmiEventId::PRE_TICK).count(), 0);
    assert_eq!(host.enabled().count(), 0);
    host.set_enabled(&id("hello"), true).unwrap();
    let mut back = ChatSend::new(":shrug:");
    game.dispatch(&mut back);
    assert_ne!(back.text(), ":shrug:");
}

/// What a player of the real `hello` mod sees: settings, a widget, a key, a command.
#[test]
fn hello_declares_settings_a_widget_a_key_and_a_command() {
    let tmp = TempDir::new();
    install(&tmp.0, "hello");
    let candidates = discover(&tmp.0).into_iter().map(|d| Candidate {
        manifest: d.manifest.unwrap(),
        enabled: true,
    });
    let resolution = resolve(candidates, ApiVersion { major: 0, minor: 3 });
    let (host, failures) =
        ModHost::load(&resolution, &tmp.0, &mut TrustList::default(), |_, _| true);
    assert!(failures.is_empty(), "{failures:?}");
    let hello = id("hello");

    let view = host.mods().next().unwrap();
    assert_eq!(
        (view.name, view.enabled, view.failed),
        ("Hello", true, false)
    );
    let keys: Vec<_> = view.settings().map(|s| s.spec.key.clone()).collect();
    assert_eq!(keys, ["show_ticks", "tick_color"]);
    let bindings: Vec<_> = host.key_bindings().collect();
    assert_eq!(bindings.len(), 1);
    assert_eq!(
        (bindings[0].key, bindings[0].default_binding),
        ("greet", "KeyH")
    );

    // The widget draws a line on the render thread.
    host.draw_widgets();
    let mut lines = Vec::new();
    host.visit_widgets(|w| {
        lines.extend(w.content.lines().iter().map(|l| l.text.clone()));
        assert_eq!((w.mod_id.as_str(), w.key), ("hello", "ticks"));
    });
    assert_eq!(lines.len(), 1);
    assert!(lines[0].starts_with("ticks: "), "{lines:?}");

    // A setting changes what the mod's closure reads.
    host.set_setting(&hello, "show_ticks", &SettingValue::Bool(false))
        .unwrap();
    host.draw_widgets();
    let mut shown = 0;
    host.visit_widgets(|w| shown += w.content.lines().len());
    assert_eq!(shown, 0);

    // The key binding fires an observer on the render thread's worker, which prints.
    let mut render = EventBus::default();
    host.install(&mut render, EventThread::Render);
    render.dispatch(&mut KeyPressed::new(hello.clone(), "greet"));
    let outbox = host.chat_outbox();
    let line = outbox.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(line, "hello from the key binding");

    // `/hello bob` runs on the client and prints; `/hello` alone is refused with
    // its usage, and the mod stays enabled.
    let commands = host.commands();
    assert_eq!(commands.complete("he"), ["hello"]);
    assert_eq!(commands.run("/hello bob"), CommandOutcome::Ran);
    assert_eq!(outbox.try_recv().as_deref(), Ok("hello, bob"));
    assert_eq!(commands.run("/hello"), CommandOutcome::Rejected);
    assert!(outbox
        .try_recv()
        .is_ok_and(|line| line.starts_with("Usage: /hello")));
    assert_eq!(host.set_enabled(&hello, true), Ok(()));
    assert!(host.enabled().any(|id| id == &hello));
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Loads the real `hello` and `panicking` example cdylibs (F3.2 acceptance test).
//!
//! The libraries are built once per test binary with `cargo build --offline`. The
//! target directory is `CARGO_TARGET_DIR` when set, otherwise it is derived from the
//! test executable's location (`<target>/<profile>/deps/<test>`): that stays correct
//! for custom profiles and avoids parsing `cargo metadata` output.
#![cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use umicraft_modapi_sys::{
    UmiEventHeader, UmiEventId, UmiMode, UmiPostTick, UmiPreTick, UmiStatus, UMI_EVENT_CANCELLABLE,
};
use umicraft_modhost::{
    discover, library_file_name, platform_dir, resolve, ApiVersion, Arch, Candidate, LoadError,
    ModHost, ModId, Os, TrustList,
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
        let dir = std::env::temp_dir().join(format!("umi-real-mods-{}", std::process::id()));
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

fn header(id: UmiEventId, flags: u32, size: usize) -> UmiEventHeader {
    UmiEventHeader {
        size: u32::try_from(size).unwrap(),
        id,
        flags,
    }
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
    let resolution = resolve(candidates, ApiVersion { major: 0, minor: 1 });
    assert_eq!(resolution.order.len(), 2);

    let mut trust = TrustList::default();
    let mut asked = Vec::new();
    let (mut host, failures) = ModHost::load(&resolution, &tmp.0, &mut trust, |m, hash| {
        asked.push((m.id.to_string(), hash.to_owned()));
        true
    });

    // The panic is contained: only `panicking` failed, and the process is alive.
    assert_eq!(failures.len(), 1);
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

    // Call the handlers the way the dispatcher will. Calling an `extern "C"` fn
    // pointer and casting pointers are safe; the mod does the dereferencing.
    let mut pre_tick = UmiPreTick {
        header: header(UmiEventId::PRE_TICK, 0, size_of::<UmiPreTick>()),
        tick: 7,
    };
    let status = (pre[0].handler)(
        std::ptr::from_mut(&mut pre_tick).cast(),
        pre[0].user.as_ptr(),
    );
    assert_eq!(status, UmiStatus::OK);

    let mut post_tick = UmiPostTick {
        header: header(
            UmiEventId::POST_TICK,
            UMI_EVENT_CANCELLABLE,
            size_of::<UmiPostTick>(),
        ),
        tick: 7,
    };
    let status = (post[0].handler)(
        std::ptr::from_mut(&mut post_tick).cast(),
        post[0].user.as_ptr(),
    );
    assert_eq!(status, UmiStatus::OK);

    // Disabling shuts the mod down once and drops its subscriptions.
    assert!(host.disable(&id("hello")));
    assert!(!host.disable(&id("hello")));
    assert_eq!(host.subscriptions(UmiEventId::PRE_TICK).count(), 0);
    assert_eq!(host.subscriptions(UmiEventId::POST_TICK).count(), 0);
    assert_eq!(host.enabled().count(), 0);
}

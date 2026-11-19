//! Tests of discovery and of the load failure paths that need no real library.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use umicraft_modhost::{
    discover, library_file_name, platform_dir, Arch, DiscoverError, LoadError, Manifest, ModHost,
    ModId, Os, Resolution, TrustList,
};

/// A scratch directory removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "umi-modhost-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn manifest_text(id: &str) -> String {
    format!("[mod]\nid = \"{id}\"\nname = \"{id}\"\nversion = \"1.0.0\"\nauthors = []\napi_version = \"0.1\"\n")
}

fn write_mod(root: &Path, dir: &str, manifest: Option<&str>) {
    let dir = root.join(dir);
    fs::create_dir_all(&dir).unwrap();
    if let Some(text) = manifest {
        fs::write(dir.join("mod.toml"), text).unwrap();
    }
}

fn resolution_of(id: &str) -> Resolution {
    Resolution {
        order: vec![Manifest::parse(&manifest_text(id)).unwrap()],
        rejected: Vec::new(),
    }
}

/// Creates the library file of mod `id` with `content` where the loader expects it.
fn write_library(root: &Path, id: &str, content: &[u8]) {
    let (os, arch) = (Os::current().unwrap(), Arch::current().unwrap());
    let dir = root.join(id).join("bin").join(platform_dir(os, arch));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(library_file_name(id, os)), content).unwrap();
}

fn mod_id(id: &str) -> ModId {
    id.to_owned().try_into().unwrap()
}

#[test]
fn discover_sorts_and_reports_each_entry() {
    let tmp = TempDir::new();
    write_mod(tmp.path(), "b_mod", Some(&manifest_text("b_mod")));
    write_mod(tmp.path(), "a_bad", Some("not toml ["));
    write_mod(tmp.path(), "c_empty", None);
    fs::write(tmp.path().join("stray.txt"), "x").unwrap();

    let found = discover(tmp.path());
    let dirs: Vec<_> = found
        .iter()
        .map(|d| d.dir.file_name().unwrap().to_str().unwrap())
        .collect();
    assert_eq!(dirs, ["a_bad", "b_mod"]);
    assert!(matches!(found[0].manifest, Err(DiscoverError::Manifest(_))));
    assert_eq!(found[1].manifest.as_ref().unwrap().id.as_str(), "b_mod");
}

#[test]
fn discover_rejects_a_folder_not_named_after_the_id() {
    let tmp = TempDir::new();
    write_mod(tmp.path(), "renamed", Some(&manifest_text("b_mod")));

    let found = discover(tmp.path());
    assert!(matches!(
        &found[0].manifest,
        Err(DiscoverError::FolderName { folder, id }) if folder == "renamed" && id == "b_mod"
    ));
}

#[test]
fn discover_of_a_missing_directory_is_empty() {
    let tmp = TempDir::new();
    assert!(discover(&tmp.path().join("absent")).is_empty());
}

#[test]
fn missing_library_is_reported() {
    let tmp = TempDir::new();
    let (host, failures) = ModHost::load(
        &resolution_of("hud"),
        tmp.path(),
        &mut TrustList::default(),
        |_, _| true,
    );
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].id.as_str(), "hud");
    assert!(matches!(
        failures[0].reason,
        LoadError::LibraryMissing { .. }
    ));
    assert_eq!(host.enabled().count(), 0);
}

#[test]
fn untrusted_library_is_refused_and_not_remembered() {
    let tmp = TempDir::new();
    write_library(tmp.path(), "hud", b"dummy");
    let mut trust = TrustList::default();
    let (_, failures) = ModHost::load(&resolution_of("hud"), tmp.path(), &mut trust, |_, _| false);
    assert!(matches!(failures[0].reason, LoadError::Untrusted { .. }));
    assert_eq!(trust, TrustList::default());
}

#[test]
fn confirmed_garbage_library_fails_to_open_and_is_remembered() {
    let tmp = TempDir::new();
    write_library(tmp.path(), "hud", b"not a shared object");
    let mut trust = TrustList::default();
    let mut asked = Vec::new();
    let (host, failures) =
        ModHost::load(&resolution_of("hud"), tmp.path(), &mut trust, |m, hash| {
            asked.push((m.id.to_string(), hash.len()));
            true
        });
    assert!(matches!(failures[0].reason, LoadError::Open(_)));
    assert_eq!(asked, [("hud".to_owned(), 64)]);
    assert_eq!(host.enabled().count(), 0);

    // The accepted hash is trusted next time: no second question.
    let (_, again) = ModHost::load(&resolution_of("hud"), tmp.path(), &mut trust, |_, _| {
        panic!("a trusted hash must not be asked again")
    });
    assert!(matches!(again[0].reason, LoadError::Open(_)));
    assert!(trust.is_trusted(&mod_id("hud"), &asked_hash(tmp.path())));
}

fn asked_hash(root: &Path) -> String {
    let (os, arch) = (Os::current().unwrap(), Arch::current().unwrap());
    let path = root
        .join("hud/bin")
        .join(platform_dir(os, arch))
        .join(library_file_name("hud", os));
    umicraft_modhost::sha256_file(&path).unwrap()
}

#[test]
fn one_failure_does_not_stop_the_others() {
    let tmp = TempDir::new();
    let mut resolution = resolution_of("one");
    resolution
        .order
        .push(Manifest::parse(&manifest_text("two")).unwrap());
    let (_, failures) = ModHost::load(
        &resolution,
        tmp.path(),
        &mut TrustList::default(),
        |_, _| true,
    );
    let ids: Vec<_> = failures.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, ["one", "two"]);
}

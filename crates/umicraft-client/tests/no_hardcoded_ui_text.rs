// SPDX-License-Identifier: MIT OR Apache-2.0

//! Guard of the language system (ADR-0039): UI code holds translation keys,
//! never the vanilla English texts.
//!
//! The test reads the sources of `umicraft-ui` and `umicraft-client`, cuts
//! each file at its first `#[cfg(test)]` module, skips comments and test-only
//! files, and fails when a string literal is exactly one of a denylist of
//! well-known vanilla UI texts. The offline fallback texts live in
//! `umicraft_core::lang` (outside the scanned crates), which is the one
//! allowed place for them.

use std::path::{Path, PathBuf};

/// Vanilla UI texts (`en_US.lang` values) that must come from a key.
const DENYLIST: &[&str] = &[
    "Back to Game",
    "Game Menu",
    "Game menu",
    "Options...",
    "Options",
    "Done",
    "Cancel",
    "Disconnect",
    "Respawn",
    "You died!",
    "Title Screen",
    "Title screen",
    "Achievements",
    "Statistics",
    "Open to LAN",
    "Save and Quit to Title",
    "Video Settings",
    "Video Settings...",
    "Chat Settings",
    "Chat Settings...",
    "Controls",
    "Controls...",
    "Language...",
    "Skin Customization...",
    "Resource Packs...",
    "Snooper Settings...",
    "Music & Sounds...",
    "Broadcast Settings...",
    "Reset Keys",
    "Walk Forward",
    "Walk Backwards",
    "Strafe Left",
    "Strafe Right",
    "Open Chat",
    "List Players",
    "Edit sign message:",
    "Enter Book Title:",
    "Delete Items",
    "Invalid Item!",
    "Invalid Entity!",
];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Whether the file only exists for tests (`tests.rs`, `test_support.rs`).
fn is_test_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "tests.rs" || name == "test_support.rs")
}

/// The production part of a source file: everything before the first
/// `#[cfg(test)]`, without comment lines.
fn production_lines(source: &str) -> impl Iterator<Item = (usize, &str)> {
    source
        .lines()
        .enumerate()
        .take_while(|(_, line)| line.trim() != "#[cfg(test)]")
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
}

/// The denylisted texts a line holds as a whole string literal.
fn denied_in(line: &str) -> impl Iterator<Item = &'static str> + '_ {
    DENYLIST
        .iter()
        .copied()
        .filter(move |text| line.contains(&format!("\"{text}\"")))
}

fn offences(path: &Path) -> Vec<String> {
    let Ok(source) = std::fs::read_to_string(path) else {
        return vec![format!("{}: unreadable", path.display())];
    };
    production_lines(&source)
        .flat_map(|(number, line)| {
            denied_in(line)
                .map(move |text| format!("{}:{}: \"{text}\"", path.display(), number + 1))
        })
        .collect()
}

#[test]
fn no_hardcoded_ui_text() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for crate_src in ["umicraft-ui/src", "umicraft-client/src"] {
        rust_files(&root.join(crate_src), &mut files);
    }
    assert!(
        files.len() > 50,
        "the scan found only {} files",
        files.len()
    );
    let found: Vec<String> = files
        .iter()
        .filter(|path| !is_test_file(path))
        .flat_map(|path| offences(path))
        .collect();
    assert!(
        found.is_empty(),
        "vanilla UI texts must come from translation keys (ADR-0039):\n{}",
        found.join("\n")
    );
}

#[test]
fn the_guard_catches_a_literal_outside_tests_only() {
    let source =
        "let a = 1;\nlet b = \"Back to Game\";\n// \"Done\"\n#[cfg(test)]\nlet c = \"Done\";\n";
    let hits: Vec<(usize, &str)> = production_lines(source)
        .filter(|(_, line)| denied_in(line).next().is_some())
        .collect();
    assert_eq!(hits, vec![(1, "let b = \"Back to Game\";")]);
}

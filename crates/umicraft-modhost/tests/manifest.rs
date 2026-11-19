// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of `mod.toml` parsing, identifiers, API versions and library names.

use umicraft_modhost::{
    library_file_name, platform_dir, ApiVersion, Arch, Manifest, ManifestError, ModId, Os,
    ServiceId,
};

const FULL: &str = r#"
[mod]
id = "example_hud"
name = "Example HUD"
version = "0.1.0"
authors = ["Someone"]
api_version = "0.1"
license = "MIT"
description = "A HUD."
library = "example_hud_lib"

[dependencies]
other_mod = ">=0.2, <1.0"

[load]
priority = 100
replaces = ["render.sky"]
wraps = ["render.hud"]

[permissions]
network_packets = true
movement = false
filesystem = true
network_http = false
"#;

const MINIMAL: &str = r#"
[mod]
id = "tiny"
name = "Tiny"
version = "1.2.3"
authors = []
api_version = "1.0"
"#;

fn with_mod_line(line: &str) -> String {
    format!("{MINIMAL}{line}\n")
}

fn parse_err(text: &str) -> ManifestError {
    Manifest::parse(text).unwrap_err()
}

#[test]
fn parses_the_full_example() {
    let m = Manifest::parse(FULL).unwrap();
    assert_eq!(m.id.as_str(), "example_hud");
    assert_eq!(m.name, "Example HUD");
    assert_eq!(m.version.to_string(), "0.1.0");
    assert_eq!(m.authors, ["Someone"]);
    assert_eq!(m.api_version, ApiVersion { major: 0, minor: 1 });
    assert_eq!(m.license.as_deref(), Some("MIT"));
    assert_eq!(m.description.as_deref(), Some("A HUD."));
    assert_eq!(m.library, "example_hud_lib");
    let other: ModId = "other_mod".parse().unwrap();
    assert!(m.dependencies[&other].matches(&"0.5.0".parse().unwrap()));
    assert!(!m.dependencies[&other].matches(&"1.0.0".parse().unwrap()));
    assert_eq!(m.load.priority, 100);
    assert_eq!(
        m.load.replaces,
        ["render.sky".parse::<ServiceId>().unwrap()]
    );
    assert_eq!(m.load.wraps, ["render.hud".parse::<ServiceId>().unwrap()]);
    assert!(m.permissions.network_packets && m.permissions.filesystem);
    assert!(!m.permissions.movement && !m.permissions.network_http);
}

#[test]
fn defaults_apply() {
    let m = Manifest::parse(MINIMAL).unwrap();
    assert_eq!(m.library, "tiny");
    assert!(m.dependencies.is_empty());
    assert_eq!(m.load.priority, 0);
    assert!(m.load.replaces.is_empty() && m.load.wraps.is_empty());
    assert_eq!(m.permissions, Default::default());
    assert_eq!(m.license, None);
}

#[test]
fn rejects_unknown_keys_in_every_table() {
    let in_mod = MINIMAL.replace("authors = []", "authors = []\nextra = 1");
    let cases = [
        in_mod,
        with_mod_line("[load]\nbogus = 1"),
        with_mod_line("[permissions]\nbogus = true"),
        with_mod_line("[bogus]\nx = 1"),
    ];
    for text in cases {
        assert!(
            matches!(parse_err(&text), ManifestError::Syntax(_)),
            "{text}"
        );
    }
}

#[test]
fn rejects_missing_required_keys() {
    let text = MINIMAL.replace("name = \"Tiny\"\n", "");
    assert!(matches!(parse_err(&text), ManifestError::Syntax(_)));
    assert!(matches!(
        parse_err("[dependencies]\n"),
        ManifestError::Syntax(_)
    ));
}

#[test]
fn rejects_bad_ids() {
    for bad in ["", "Upper", "1abc", "has-dash", "has.dot", &"a".repeat(65)] {
        let text = MINIMAL.replace("id = \"tiny\"", &format!("id = \"{bad}\""));
        assert!(
            matches!(parse_err(&text), ManifestError::Syntax(_)),
            "{bad}"
        );
    }
    assert!(Manifest::parse(&MINIMAL.replace("tiny", &"a".repeat(64))).is_ok());
}

#[test]
fn service_ids_allow_dots_mod_ids_do_not() {
    assert!("render.hud_v2".parse::<ServiceId>().is_ok());
    assert!("render.hud".parse::<ModId>().is_err());
    assert!(".hud".parse::<ServiceId>().is_err());
    let err = "Bad".parse::<ModId>().unwrap_err();
    assert!(err.to_string().contains("`Bad`"));
}

#[test]
fn rejects_bad_versions() {
    let text = MINIMAL.replace("1.2.3", "one");
    assert!(matches!(parse_err(&text), ManifestError::Version { .. }));
    let text = MINIMAL.replace("\"1.0\"", "\"1\"");
    assert!(matches!(parse_err(&text), ManifestError::Syntax(_)));
    let text = with_mod_line("[dependencies]\nother = \"not a req\"");
    assert!(matches!(
        parse_err(&text),
        ManifestError::Requirement { .. }
    ));
}

#[test]
fn rejects_empty_name_and_bad_library() {
    let text = MINIMAL.replace("\"Tiny\"", "\"  \"");
    assert!(matches!(parse_err(&text), ManifestError::EmptyName));
    for bad in ["", "../evil", "a/b", "a.b"] {
        let text = MINIMAL.replace(
            "authors = []",
            &format!("authors = []\nlibrary = \"{bad}\""),
        );
        assert!(
            matches!(parse_err(&text), ManifestError::InvalidLibrary(_)),
            "{bad}"
        );
    }
}

#[test]
fn rejects_self_dependency() {
    let text = with_mod_line("[dependencies]\ntiny = \"*\"");
    assert!(matches!(parse_err(&text), ManifestError::SelfDependency(id) if id.as_str() == "tiny"));
}

#[test]
fn rejects_inconsistent_load_lists() {
    let dup = with_mod_line("[load]\nreplaces = [\"a\", \"a\"]");
    assert!(matches!(
        parse_err(&dup),
        ManifestError::DuplicateService {
            list: "replaces",
            ..
        }
    ));
    let dup = with_mod_line("[load]\nwraps = [\"a\", \"b\", \"a\"]");
    assert!(matches!(
        parse_err(&dup),
        ManifestError::DuplicateService { list: "wraps", .. }
    ));
    let both = with_mod_line("[load]\nreplaces = [\"a\"]\nwraps = [\"a\"]");
    assert!(matches!(
        parse_err(&both),
        ManifestError::ReplacedAndWrapped(_)
    ));
}

#[test]
fn error_messages_name_the_problem() {
    let msg = parse_err(&MINIMAL.replace("1.2.3", "one")).to_string();
    assert!(msg.contains("mod.version") && msg.contains("one"));
}

#[test]
fn api_version_parsing() {
    assert_eq!(
        "0.1".parse::<ApiVersion>().unwrap(),
        ApiVersion { major: 0, minor: 1 }
    );
    assert_eq!("12.34".parse::<ApiVersion>().unwrap().to_string(), "12.34");
    for bad in [
        "", "1", "1.", ".1", "1.2.3", "+1.2", "a.b", "-1.0", "65536.0", "1. 2",
    ] {
        assert!(bad.parse::<ApiVersion>().is_err(), "{bad}");
    }
}

#[test]
fn api_version_acceptance_table() {
    let v = |major, minor| ApiVersion { major, minor };
    // (host, required, accepted)
    let table = [
        (v(0, 1), v(0, 1), true),
        (v(0, 2), v(0, 1), false),
        (v(0, 1), v(0, 2), false),
        (v(0, 1), v(1, 1), false),
        (v(1, 0), v(1, 0), true),
        (v(1, 3), v(1, 2), true),
        (v(1, 2), v(1, 3), false),
        (v(2, 0), v(1, 0), false),
        (v(1, 0), v(2, 0), false),
        (v(1, 0), v(0, 0), false),
    ];
    for (host, required, expected) in table {
        assert_eq!(
            host.accepts(required),
            expected,
            "host {host} required {required}"
        );
    }
}

#[test]
fn library_names_and_platform_dirs() {
    assert_eq!(library_file_name("foo", Os::Windows), "foo.dll");
    assert_eq!(library_file_name("foo", Os::Linux), "libfoo.so");
    assert_eq!(library_file_name("foo", Os::Macos), "libfoo.dylib");
    assert_eq!(platform_dir(Os::Linux, Arch::X86_64), "linux-x86_64");
    assert_eq!(platform_dir(Os::Macos, Arch::Aarch64), "macos-aarch64");
    assert_eq!(platform_dir(Os::Windows, Arch::Aarch64), "windows-aarch64");
    if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        assert_eq!(Os::current(), Some(Os::Linux));
        assert_eq!(Arch::current(), Some(Arch::X86_64));
    }
}

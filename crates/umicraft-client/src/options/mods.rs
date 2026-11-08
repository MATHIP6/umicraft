// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `[mods]` section of `options.toml`: the saved state of the built-in mods
//! (ADR-0065).
//!
//! The type lives in `umicraft-builtin-mods`, which sanitizes every value and keeps
//! the tables of mods this version does not know. This module only gives the
//! section its place among the other options and pins its behaviour with tests.

/// The saved state of the built-in mods, one `[mods.<id>]` table per mod.
pub type ModsOptions = umicraft_builtin_mods::ModsConfig;

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use umicraft_builtin_mods::{Anchor, HudPlacement, ModConfig};

    use super::*;
    use crate::options::{Options, FILE_NAME};

    fn temp_file(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("umicraft-mods-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("temp folder");
        folder.join(FILE_NAME)
    }

    fn sample() -> ModsOptions {
        let mut mods = ModsOptions::default();
        mods.0.insert(
            "fps".into(),
            ModConfig {
                enabled: Some(true),
                placement: Some(HudPlacement::at(Anchor::TopRight, -4.0, 4.0)),
                ..ModConfig::default()
            },
        );
        mods
    }

    #[test]
    fn the_mods_survive_a_restart() {
        let path = temp_file("round-trip");
        let options = Options {
            mods: sample(),
            ..Options::default()
        };
        options.save(&path).expect("saves");
        assert_eq!(Options::load(&path).mods, options.mods);
        let _ = std::fs::remove_dir_all(path.parent().expect("folder"));
    }

    #[test]
    fn a_missing_section_gives_no_mod_config() {
        let path = temp_file("missing");
        std::fs::write(&path, "[menu]\ntitle_style = \"classic\"\n").expect("writes");
        assert_eq!(Options::load(&path).mods, ModsOptions::default());
        let _ = std::fs::remove_dir_all(path.parent().expect("folder"));
    }

    #[test]
    fn an_invalid_section_gives_the_defaults_and_spares_the_others() {
        let path = temp_file("invalid");
        std::fs::write(
            &path,
            "[mods]\nfps = 3\n[menu]\ntitle_style = \"classic\"\n",
        )
        .expect("writes");
        let options = Options::load(&path);
        assert_eq!(options.mods, ModsOptions::default());
        assert_eq!(
            options.menu.title_style,
            crate::options::TitleStyleSetting::Classic
        );
        let _ = std::fs::remove_dir_all(path.parent().expect("folder"));
    }

    #[test]
    fn the_tables_of_unknown_mods_are_written_back() {
        let path = temp_file("unknown");
        std::fs::write(&path, "[mods.future]\nenabled = true\n").expect("writes");
        let options = Options::load(&path);
        assert!(options.mods.0.contains_key("future"));
        options.save(&path).expect("saves");
        let text = std::fs::read_to_string(&path).expect("reads");
        assert!(text.contains("[mods.future]"));
        let _ = std::fs::remove_dir_all(path.parent().expect("folder"));
    }
}

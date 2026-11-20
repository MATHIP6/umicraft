// SPDX-License-Identifier: MIT OR Apache-2.0

//! Where the player's choices for each external mod are kept (F3.4).
//!
//! One file per mod, `<options dir>/mods/<id>.toml`, next to `options.toml` so a
//! mod's choices travel with the rest of the player's settings. Mods are optional,
//! so every I/O problem is logged and the client carries on with the defaults.

use std::path::{Path, PathBuf};

use umicraft_modhost::{ExternalModConfig, ModHost, ModId};

/// The folder of the config files, inside the options directory.
const FOLDER: &str = "mods";

/// The folder of the config files for the options file at `options_path`.
pub(crate) fn config_dir(options_path: &Path) -> Option<PathBuf> {
    options_path.parent().map(|dir| dir.join(FOLDER))
}

/// The config file of mod `id` inside `dir`. A mod id is `[a-z0-9_]`, so it is a safe
/// file name.
fn config_path(dir: &Path, id: &ModId) -> PathBuf {
    dir.join(format!("{id}.toml"))
}

/// Applies the saved choices (including the switch) to every loaded mod. A missing
/// file means the first run and changes nothing; a broken one is logged and ignored.
pub(crate) fn load_all(host: &ModHost, dir: &Path) {
    let ids: Vec<ModId> = host.mods().map(|view| view.id.clone()).collect();
    for id in ids {
        let path = config_path(dir, &id);
        match ExternalModConfig::load(&path) {
            Ok(config) => {
                if let Err(error) = host.apply_config(&id, &config) {
                    tracing::warn!(mod_id = %id, %error, "cannot apply the mod's saved choices");
                }
            }
            Err(error) => {
                tracing::warn!(mod_id = %id, path = %path.display(), %error, "cannot read the mod's saved choices");
            }
        }
    }
}

/// Writes the current choices of mod `id`; a failure is logged, never fatal.
pub(crate) fn save(host: &ModHost, dir: &Path, id: &ModId) {
    let Some(config) = host.config(id) else {
        return;
    };
    let path = config_path(dir, id);
    if let Err(error) = config.save(&path) {
        tracing::warn!(mod_id = %id, path = %path.display(), %error, "cannot save the mod's choices");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_folder_is_next_to_the_options_file() {
        assert_eq!(
            config_dir(Path::new("/data/umicraft/options.toml")),
            Some(PathBuf::from("/data/umicraft/mods"))
        );
        assert_eq!(config_dir(Path::new("")), None);
    }

    #[test]
    fn each_mod_has_its_own_toml_file() {
        let id: ModId = "hello_hud".parse().expect("valid id");
        assert_eq!(
            config_path(Path::new("/data/mods"), &id),
            PathBuf::from("/data/mods/hello_hud.toml")
        );
    }
}

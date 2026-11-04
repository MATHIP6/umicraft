// SPDX-License-Identifier: MIT OR Apache-2.0

//! The user's options: video, mouse, chat and the key bindings (M10, UI-3).
//!
//! The options live in one TOML file, `options.toml`, in the Umicraft data
//! folder, as the sections `[video]`, `[controls]`, `[chat]` and `[keys]`
//! (ADR-0017). Loading never fails: a missing file gives the defaults, a
//! section that does not parse is replaced by its defaults without touching
//! the others, and every number is clamped to its range. Saving is atomic
//! (see `ConfigStore::save`). The defaults and ranges follow MCP
//! `GameSettings`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use umicraft_core::{ConfigStore, CoreError};

mod chat;
mod controls;
mod keys;
#[cfg(test)]
mod tests;
mod video;

pub use chat::{ChatOptions, ChatVisibility};
pub use controls::ControlOptions;
pub use keys::{is_reserved, key_from_name, key_label, key_name, Action, KeyBindings};
pub use video::{VideoOptions, FOV, MAX_FPS, MAX_PARTICLE_SETTING, RENDER_DISTANCE, UNLIMITED_FPS};

/// The name of the options file.
pub const FILE_NAME: &str = "options.toml";

/// The path of the options file in the Umicraft data folder, when the
/// platform has one.
#[must_use]
pub fn default_path() -> Option<PathBuf> {
    umicraft_assets::cache::data_dir().map(|folder| folder.join(FILE_NAME))
}

/// All the options.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Options {
    /// The video options.
    pub video: VideoOptions,
    /// The mouse options.
    pub controls: ControlOptions,
    /// The chat options.
    pub chat: ChatOptions,
    /// The key bindings.
    pub keys: KeyBindings,
}

impl Options {
    /// Puts every number back in its range; a value that is not a number
    /// returns to its default.
    pub fn clamp(&mut self) {
        let defaults = Self::default();
        self.video.render_distance = self
            .video
            .render_distance
            .clamp(*RENDER_DISTANCE.start(), *RENDER_DISTANCE.end());
        self.video.max_fps = self.video.max_fps.clamp(*MAX_FPS.start(), *MAX_FPS.end());
        self.video.particles = self.video.particles.min(MAX_PARTICLE_SETTING);
        self.video.fov = if self.video.fov.is_finite() {
            self.video.fov.clamp(*FOV.start(), *FOV.end())
        } else {
            defaults.video.fov
        };
        let defaults_chat = ChatOptions::default();
        for (value, default) in [
            (&mut self.chat.opacity, defaults_chat.opacity),
            (&mut self.chat.scale, defaults_chat.scale),
            (&mut self.chat.height_focused, defaults_chat.height_focused),
            (
                &mut self.chat.height_unfocused,
                defaults_chat.height_unfocused,
            ),
            (&mut self.chat.width, defaults_chat.width),
        ] {
            *value = if value.is_finite() {
                value.clamp(0.0, 1.0)
            } else {
                default
            };
        }
        self.controls.mouse_sensitivity = if self.controls.mouse_sensitivity.is_finite() {
            self.controls.mouse_sensitivity.clamp(0.0, 1.0)
        } else {
            defaults.controls.mouse_sensitivity
        };
    }

    /// Reads the options from `store`, section by section.
    fn from_store(store: &ConfigStore) -> Self {
        fn section<T: serde::de::DeserializeOwned + Default>(store: &ConfigStore, name: &str) -> T {
            match store.section::<T>(name) {
                Ok(value) => value.unwrap_or_default(),
                Err(error) => {
                    tracing::warn!(section = name, %error, "options section ignored");
                    T::default()
                }
            }
        }
        let keys: BTreeMap<String, String> = section(store, "keys");
        let mut options = Self {
            video: section(store, "video"),
            controls: section(store, "controls"),
            chat: section(store, "chat"),
            keys: KeyBindings::from_map(&keys),
        };
        options.clamp();
        options
    }

    /// Loads the options from `path`. A missing or unreadable file gives the
    /// defaults.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        match ConfigStore::from_path(path) {
            Ok(store) => Self::from_store(&store),
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "options file ignored");
                Self::default()
            }
        }
    }

    /// Saves the options to `path`, keeping any section this version does
    /// not know about.
    ///
    /// # Errors
    /// [`CoreError`] when the file cannot be serialized or written.
    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        let mut store = ConfigStore::from_path(path).unwrap_or_default();
        store.set_section("video", &self.video)?;
        store.set_section("controls", &self.controls)?;
        store.set_section("chat", &self.chat)?;
        store.set_section("keys", &self.keys.to_map())?;
        store.save(path)
    }
}

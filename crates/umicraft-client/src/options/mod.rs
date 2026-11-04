// SPDX-License-Identifier: MIT OR Apache-2.0

//! The user's options: video, mouse, chat, language, skin, sound, resource
//! packs and the key bindings (M10, UI-3, ADR-0043).
//!
//! The options live in one TOML file, `options.toml`, in the Umicraft data
//! folder, as the sections `[video]`, `[controls]`, `[chat]`, `[language]`,
//! `[skin]`, `[sound]`, `[resource_packs]` and `[keys]` (ADR-0017). Loading never fails: a missing file gives the defaults, a
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
mod language;
mod packs;
mod skin;
mod sound;
#[cfg(test)]
mod tests;
mod video;

pub use chat::{ChatOptions, ChatVisibility};
pub use controls::ControlOptions;
pub use keys::{is_reserved, key_from_name, key_label, key_name, Action, KeyBindings};
pub use language::LanguageOptions;
pub use packs::{is_pack_name, ResourcePackOptions, PACKS_FOLDER};
pub use skin::{SkinOptions, SkinPart};
pub use sound::{SoundCategory, SoundOptions};
pub use video::{
    VideoOptions, FOV, MAX_CLOUD_SETTING, MAX_FPS, MAX_GUI_SCALE, MAX_PARTICLE_SETTING,
    MAX_SMOOTH_LIGHTING, MIPMAP_LEVELS, RENDER_DISTANCE, UNLIMITED_FPS,
};

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
    /// The language of the texts.
    pub language: LanguageOptions,
    /// The skin layers sent in `Client Settings`.
    pub skin: SkinOptions,
    /// The sound volumes (values only until P11).
    pub sound: SoundOptions,
    /// The enabled resource packs.
    pub resource_packs: ResourcePackOptions,
    /// The key bindings.
    pub keys: KeyBindings,
}

impl Options {
    /// Puts every number back in its range; a value that is not a number
    /// returns to its default.
    pub fn clamp(&mut self) {
        self.language.clamp();
        self.video.clamp();
        self.sound.clamp();
        self.resource_packs.clamp();
        let defaults = Self::default();
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
            language: section(store, "language"),
            skin: section(store, "skin"),
            sound: section(store, "sound"),
            resource_packs: section(store, "resource_packs"),
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
        store.set_section("language", &self.language)?;
        store.set_section("skin", &self.skin)?;
        store.set_section("sound", &self.sound)?;
        store.set_section("resource_packs", &self.resource_packs)?;
        store.set_section("keys", &self.keys.to_map())?;
        store.save(path)
    }
}

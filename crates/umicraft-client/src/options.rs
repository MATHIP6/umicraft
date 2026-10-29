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

use serde::{Deserialize, Serialize};
use umicraft_core::{ConfigStore, CoreError};
use umicraft_render::HudState;
use winit::keyboard::KeyCode;

/// The name of the options file.
pub const FILE_NAME: &str = "options.toml";

/// The path of the options file in the Umicraft data folder, when the
/// platform has one.
#[must_use]
pub fn default_path() -> Option<PathBuf> {
    umicraft_assets::cache::data_dir().map(|folder| folder.join(FILE_NAME))
}

/// The render distance range, in chunks.
pub const RENDER_DISTANCE: std::ops::RangeInclusive<u8> = 2..=16;
/// The field-of-view range, in degrees.
pub const FOV: std::ops::RangeInclusive<f32> = 30.0..=110.0;
/// The frame-rate cap range; the top value means "unlimited".
pub const MAX_FPS: std::ops::RangeInclusive<u32> = 10..=260;
/// The frame-rate cap value that disables the limiter.
pub const UNLIMITED_FPS: u32 = 260;

/// The video options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoOptions {
    /// How many chunks around the player are requested and drawn.
    pub render_distance: u8,
    /// The vertical field of view, in degrees.
    pub fov: f32,
    /// The frame-rate cap; [`UNLIMITED_FPS`] means no cap.
    pub max_fps: u32,
}

impl Default for VideoOptions {
    fn default() -> Self {
        Self {
            render_distance: 8,
            fov: 70.0,
            max_fps: 120,
        }
    }
}

/// The mouse options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ControlOptions {
    /// The look sensitivity, `0.0..=1.0`; `0.5` is the vanilla default.
    pub mouse_sensitivity: f32,
    /// Whether moving the mouse up looks down.
    pub invert_mouse: bool,
}

impl Default for ControlOptions {
    fn default() -> Self {
        Self {
            mouse_sensitivity: 0.5,
            invert_mouse: false,
        }
    }
}

impl ControlOptions {
    /// The multiplier of the base look speed. It is the vanilla formula
    /// `(sensitivity * 0.6 + 0.2)^3 * 8`, which is exactly `1.0` at the
    /// default sensitivity.
    #[must_use]
    pub fn look_factor(&self) -> f32 {
        (self.mouse_sensitivity * 0.6 + 0.2).powi(3) * 8.0
    }
}

/// Which chat messages the server sends (`EnumChatVisibility`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatVisibility {
    /// Every message.
    #[default]
    Full,
    /// Only the replies to the player's commands.
    Commands,
    /// No message.
    Hidden,
}

impl ChatVisibility {
    /// The byte of the `Client Settings` packet.
    #[must_use]
    pub const fn wire(self) -> u8 {
        match self {
            Self::Full => 0,
            Self::Commands => 1,
            Self::Hidden => 2,
        }
    }

    /// The next value when the player clicks the button.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Full => Self::Commands,
            Self::Commands => Self::Hidden,
            Self::Hidden => Self::Full,
        }
    }
}

/// The chat options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatOptions {
    /// Which messages the server sends.
    pub visibility: ChatVisibility,
    /// Whether the colours and styles of the chat are shown.
    pub colors: bool,
    /// `chatLinks`: whether links in the chat are recognised.
    pub links: bool,
    /// `chatLinksPrompt`: whether a click on a link asks first.
    pub links_prompt: bool,
    /// `chatOpacity`, `0.0..=1.0`.
    pub opacity: f32,
    /// `chatScale`, `0.0..=1.0`.
    pub scale: f32,
    /// `chatHeightFocused`, `0.0..=1.0`.
    pub height_focused: f32,
    /// `chatHeightUnfocused`, `0.0..=1.0`.
    pub height_unfocused: f32,
    /// `chatWidth`, `0.0..=1.0`.
    pub width: f32,
    /// `reducedDebugInfo`.
    pub reduced_debug: bool,
}

impl Default for ChatOptions {
    fn default() -> Self {
        Self {
            visibility: ChatVisibility::Full,
            colors: true,
            links: true,
            links_prompt: false,
            opacity: 1.0,
            scale: 1.0,
            height_focused: 1.0,
            height_unfocused: 0.444_444_45,
            width: 1.0,
            reduced_debug: false,
        }
    }
}

impl ChatOptions {
    /// Applies the options to the chat of a HUD snapshot: hidden chat shows
    /// no message, and with the colours off every message is plain white
    /// text (`GameSettings.chatColours`).
    pub fn apply(&self, hud: &mut HudState) {
        hud.chat_style = umicraft_render::ChatStyle {
            opacity: self.opacity,
            scale: self.scale,
            width: self.width,
            height_focused: self.height_focused,
            height_unfocused: self.height_unfocused,
        };
        if self.visibility == ChatVisibility::Hidden {
            hud.chat.clear();
        }
        if !self.colors {
            for run in hud.chat.iter_mut().flat_map(|line| line.runs.iter_mut()) {
                run.color = [255, 255, 255];
                run.bold = false;
                run.italic = false;
                run.underlined = false;
                run.strikethrough = false;
                run.obfuscated = false;
            }
        }
    }
}

/// What a key can be bound to. Only the actions the client acts on are
/// listed; the function keys (`F1` to `F4`) and `Escape` are fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// Walk forward.
    Forward,
    /// Walk backwards.
    Back,
    /// Strafe left.
    Left,
    /// Strafe right.
    Right,
    /// Jump.
    Jump,
    /// Sneak.
    Sneak,
    /// Sprint.
    Sprint,
    /// Open the inventory.
    Inventory,
    /// Open the chat.
    Chat,
    /// Open the chat with a slash already typed.
    Command,
    /// Show the tab list while held.
    PlayerList,
    /// Select hotbar slot 1.
    Hotbar1,
    /// Select hotbar slot 2.
    Hotbar2,
    /// Select hotbar slot 3.
    Hotbar3,
    /// Select hotbar slot 4.
    Hotbar4,
    /// Select hotbar slot 5.
    Hotbar5,
    /// Select hotbar slot 6.
    Hotbar6,
    /// Select hotbar slot 7.
    Hotbar7,
    /// Select hotbar slot 8.
    Hotbar8,
    /// Select hotbar slot 9.
    Hotbar9,
}

impl Action {
    /// Every action, in the order the controls screen lists them.
    pub const ALL: [Self; 20] = [
        Self::Forward,
        Self::Left,
        Self::Back,
        Self::Right,
        Self::Jump,
        Self::Sneak,
        Self::Sprint,
        Self::Inventory,
        Self::Chat,
        Self::Command,
        Self::PlayerList,
        Self::Hotbar1,
        Self::Hotbar2,
        Self::Hotbar3,
        Self::Hotbar4,
        Self::Hotbar5,
        Self::Hotbar6,
        Self::Hotbar7,
        Self::Hotbar8,
        Self::Hotbar9,
    ];

    /// The key of the action in `options.toml`.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Forward => "forward",
            Self::Back => "back",
            Self::Left => "left",
            Self::Right => "right",
            Self::Jump => "jump",
            Self::Sneak => "sneak",
            Self::Sprint => "sprint",
            Self::Inventory => "inventory",
            Self::Chat => "chat",
            Self::Command => "command",
            Self::PlayerList => "player_list",
            Self::Hotbar1 => "hotbar1",
            Self::Hotbar2 => "hotbar2",
            Self::Hotbar3 => "hotbar3",
            Self::Hotbar4 => "hotbar4",
            Self::Hotbar5 => "hotbar5",
            Self::Hotbar6 => "hotbar6",
            Self::Hotbar7 => "hotbar7",
            Self::Hotbar8 => "hotbar8",
            Self::Hotbar9 => "hotbar9",
        }
    }

    /// The name shown on the controls screen.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Forward => "FORWARD",
            Self::Back => "BACK",
            Self::Left => "LEFT",
            Self::Right => "RIGHT",
            Self::Jump => "JUMP",
            Self::Sneak => "SNEAK",
            Self::Sprint => "SPRINT",
            Self::Inventory => "INVENTORY",
            Self::Chat => "CHAT",
            Self::Command => "COMMAND",
            Self::PlayerList => "LIST PLAYERS",
            Self::Hotbar1 => "HOTBAR 1",
            Self::Hotbar2 => "HOTBAR 2",
            Self::Hotbar3 => "HOTBAR 3",
            Self::Hotbar4 => "HOTBAR 4",
            Self::Hotbar5 => "HOTBAR 5",
            Self::Hotbar6 => "HOTBAR 6",
            Self::Hotbar7 => "HOTBAR 7",
            Self::Hotbar8 => "HOTBAR 8",
            Self::Hotbar9 => "HOTBAR 9",
        }
    }

    /// The default key (`GameSettings`: W A S D, space, left shift, left
    /// control, E, T, `/`, Tab and the digits).
    #[must_use]
    pub const fn default_key(self) -> KeyCode {
        match self {
            Self::Forward => KeyCode::KeyW,
            Self::Back => KeyCode::KeyS,
            Self::Left => KeyCode::KeyA,
            Self::Right => KeyCode::KeyD,
            Self::Jump => KeyCode::Space,
            Self::Sneak => KeyCode::ShiftLeft,
            Self::Sprint => KeyCode::ControlLeft,
            Self::Inventory => KeyCode::KeyE,
            Self::Chat => KeyCode::KeyT,
            Self::Command => KeyCode::Slash,
            Self::PlayerList => KeyCode::Tab,
            Self::Hotbar1 => KeyCode::Digit1,
            Self::Hotbar2 => KeyCode::Digit2,
            Self::Hotbar3 => KeyCode::Digit3,
            Self::Hotbar4 => KeyCode::Digit4,
            Self::Hotbar5 => KeyCode::Digit5,
            Self::Hotbar6 => KeyCode::Digit6,
            Self::Hotbar7 => KeyCode::Digit7,
            Self::Hotbar8 => KeyCode::Digit8,
            Self::Hotbar9 => KeyCode::Digit9,
        }
    }

    /// The hotbar slot of a hotbar action, `0..=8`.
    #[must_use]
    pub const fn hotbar_slot(self) -> Option<u8> {
        match self {
            Self::Hotbar1 => Some(0),
            Self::Hotbar2 => Some(1),
            Self::Hotbar3 => Some(2),
            Self::Hotbar4 => Some(3),
            Self::Hotbar5 => Some(4),
            Self::Hotbar6 => Some(5),
            Self::Hotbar7 => Some(6),
            Self::Hotbar8 => Some(7),
            Self::Hotbar9 => Some(8),
            _ => None,
        }
    }

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or(0)
    }
}

/// Lists `(name, key)` pairs for the keys a player can bind; the name is the
/// winit variant name, which is what `options.toml` stores.
macro_rules! bindable_keys {
    ($($key:ident),* $(,)?) => {
        &[$((stringify!($key), KeyCode::$key)),*]
    };
}

const BINDABLE: &[(&str, KeyCode)] = bindable_keys![
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Space,
    Tab,
    Enter,
    Backspace,
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    AltLeft,
    AltRight,
    Slash,
    Backslash,
    Comma,
    Period,
    Semicolon,
    Quote,
    Minus,
    Equal,
    BracketLeft,
    BracketRight,
    Backquote,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    CapsLock,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
];

/// The name `options.toml` stores for a key, or `None` for a key that cannot
/// be bound.
#[must_use]
pub fn key_name(key: KeyCode) -> Option<&'static str> {
    BINDABLE
        .iter()
        .find(|(_, candidate)| *candidate == key)
        .map(|(name, _)| *name)
}

/// The key stored under `name`.
#[must_use]
pub fn key_from_name(name: &str) -> Option<KeyCode> {
    BINDABLE
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, key)| *key)
}

/// Whether the client keeps a key for itself: `Escape` closes screens and
/// `F1` to `F4` toggle the overlay, the debug screen, the `F3` combinations
/// and the resource reload.
#[must_use]
pub fn is_reserved(key: KeyCode) -> bool {
    matches!(
        key,
        KeyCode::Escape | KeyCode::F1 | KeyCode::F2 | KeyCode::F3 | KeyCode::F4
    )
}

/// The text shown for a key on the controls screen.
#[must_use]
pub fn key_label(key: KeyCode) -> String {
    let Some(name) = key_name(key) else {
        return "?".to_string();
    };
    if let Some(letter) = name.strip_prefix("Key") {
        return letter.to_string();
    }
    if let Some(digit) = name.strip_prefix("Digit") {
        return digit.to_string();
    }
    if let Some(digit) = name.strip_prefix("Numpad") {
        return format!("NUM {digit}");
    }
    let label = match name {
        "ShiftLeft" => "LEFT SHIFT",
        "ShiftRight" => "RIGHT SHIFT",
        "ControlLeft" => "LEFT CTRL",
        "ControlRight" => "RIGHT CTRL",
        "AltLeft" => "LEFT ALT",
        "AltRight" => "RIGHT ALT",
        "BracketLeft" => "[",
        "BracketRight" => "]",
        "Backslash" => "\\",
        "Slash" => "/",
        "Comma" => ",",
        "Period" => ".",
        "Semicolon" => ";",
        "Quote" => "'",
        "Minus" => "-",
        "Equal" => "=",
        "Backquote" => "`",
        "ArrowUp" => "UP",
        "ArrowDown" => "DOWN",
        "ArrowLeft" => "LEFT ARROW",
        "ArrowRight" => "RIGHT ARROW",
        "PageUp" => "PAGE UP",
        "PageDown" => "PAGE DOWN",
        "CapsLock" => "CAPS LOCK",
        other => return other.to_uppercase(),
    };
    label.to_string()
}

/// The key bound to every [`Action`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBindings {
    keys: [KeyCode; Action::ALL.len()],
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            keys: Action::ALL.map(Action::default_key),
        }
    }
}

impl KeyBindings {
    /// The key bound to `action`.
    #[must_use]
    pub fn get(&self, action: Action) -> KeyCode {
        self.keys[action.index()]
    }

    /// Binds `key` to `action`. A reserved or unbindable key is refused and
    /// `false` is returned; two actions may share a key, the controls screen
    /// flags it (see [`Self::conflicts`]).
    pub fn set(&mut self, action: Action, key: KeyCode) -> bool {
        if is_reserved(key) || key_name(key).is_none() {
            return false;
        }
        self.keys[action.index()] = key;
        true
    }

    /// Restores every default key.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The other actions that use the same key as `action`.
    #[must_use]
    pub fn conflicts(&self, action: Action) -> Vec<Action> {
        let key = self.get(action);
        Action::ALL
            .iter()
            .copied()
            .filter(|other| *other != action && self.get(*other) == key)
            .collect()
    }

    /// The first action bound to `key`.
    #[must_use]
    pub fn action_for(&self, key: KeyCode) -> Option<Action> {
        Action::ALL
            .iter()
            .copied()
            .find(|action| self.get(*action) == key)
    }

    /// Every action bound to `key`, in listing order.
    pub fn actions_for(&self, key: KeyCode) -> impl Iterator<Item = Action> + '_ {
        Action::ALL
            .iter()
            .copied()
            .filter(move |action| self.get(*action) == key)
    }

    /// The `[keys]` section: action id to key name.
    fn to_map(&self) -> BTreeMap<String, String> {
        Action::ALL
            .iter()
            .filter_map(|action| {
                key_name(self.get(*action)).map(|name| (action.id().to_string(), name.to_string()))
            })
            .collect()
    }

    /// Reads the `[keys]` section. An action that is missing, or whose key is
    /// unknown or reserved, keeps its default.
    fn from_map(map: &BTreeMap<String, String>) -> Self {
        let mut bindings = Self::default();
        for action in Action::ALL {
            if let Some(key) = map.get(action.id()).and_then(|name| key_from_name(name)) {
                bindings.set(action, key);
            }
        }
        bindings
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("umicraft-options-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn the_defaults_match_the_reference_game() {
        let options = Options::default();
        assert_eq!(options.video.fov, 70.0);
        assert_eq!(options.controls.mouse_sensitivity, 0.5);
        assert!((options.controls.look_factor() - 1.0).abs() < 1e-5);
        assert_eq!(options.keys.get(Action::Forward), KeyCode::KeyW);
        assert_eq!(options.keys.get(Action::Command), KeyCode::Slash);
        assert_eq!(options.keys.get(Action::Hotbar9), KeyCode::Digit9);
        assert_eq!(options.chat.visibility, ChatVisibility::Full);
        assert!(options.chat.colors);
    }

    #[test]
    fn no_default_key_is_shared_or_reserved() {
        let bindings = KeyBindings::default();
        for action in Action::ALL {
            assert!(bindings.conflicts(action).is_empty(), "{action:?}");
            assert!(!is_reserved(bindings.get(action)), "{action:?}");
            assert!(key_name(bindings.get(action)).is_some(), "{action:?}");
        }
    }

    #[test]
    fn every_key_name_round_trips_and_has_a_label() {
        for (name, key) in BINDABLE {
            assert_eq!(key_name(*key), Some(*name));
            assert_eq!(key_from_name(name), Some(*key));
            assert!(!key_label(*key).is_empty());
        }
        assert_eq!(key_label(KeyCode::KeyW), "W");
        assert_eq!(key_label(KeyCode::Digit3), "3");
        assert_eq!(key_label(KeyCode::ShiftLeft), "LEFT SHIFT");
        assert_eq!(key_label(KeyCode::Slash), "/");
        assert_eq!(key_label(KeyCode::Escape), "?");
        assert_eq!(key_from_name("NoSuchKey"), None);
    }

    #[test]
    fn a_rebound_key_is_used_and_the_old_one_is_free() {
        let mut bindings = KeyBindings::default();
        assert!(bindings.set(Action::Forward, KeyCode::ArrowUp));
        assert_eq!(bindings.get(Action::Forward), KeyCode::ArrowUp);
        assert_eq!(bindings.action_for(KeyCode::ArrowUp), Some(Action::Forward));
        assert_eq!(bindings.action_for(KeyCode::KeyW), None);
    }

    #[test]
    fn reserved_and_unknown_keys_cannot_be_bound() {
        let mut bindings = KeyBindings::default();
        for key in [
            KeyCode::Escape,
            KeyCode::F1,
            KeyCode::F2,
            KeyCode::F3,
            KeyCode::F4,
            KeyCode::PrintScreen,
        ] {
            assert!(!bindings.set(Action::Jump, key), "{key:?}");
        }
        assert_eq!(bindings.get(Action::Jump), KeyCode::Space);
    }

    #[test]
    fn two_actions_on_one_key_are_reported_as_a_conflict() {
        let mut bindings = KeyBindings::default();
        bindings.set(Action::Back, KeyCode::KeyW);
        assert_eq!(bindings.conflicts(Action::Forward), [Action::Back]);
        assert_eq!(bindings.conflicts(Action::Back), [Action::Forward]);
        assert!(bindings.conflicts(Action::Jump).is_empty());
        let both: Vec<_> = bindings.actions_for(KeyCode::KeyW).collect();
        assert_eq!(both, [Action::Forward, Action::Back]);
    }

    #[test]
    fn reset_restores_every_default() {
        let mut bindings = KeyBindings::default();
        bindings.set(Action::Jump, KeyCode::KeyJ);
        bindings.reset();
        assert_eq!(bindings, KeyBindings::default());
    }

    #[test]
    fn hotbar_actions_map_to_their_slot() {
        let slots: Vec<_> = Action::ALL
            .iter()
            .filter_map(|action| action.hotbar_slot())
            .collect();
        assert_eq!(slots, [0, 1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(Action::Jump.hotbar_slot(), None);
    }

    #[test]
    fn options_survive_a_restart() {
        let folder = temp_folder("restart");
        let path = folder.join(FILE_NAME);
        let mut options = Options::default();
        options.keys.set(Action::Forward, KeyCode::ArrowUp);
        options.keys.set(Action::Chat, KeyCode::KeyC);
        options.video.fov = 95.0;
        options.video.render_distance = 12;
        options.video.max_fps = UNLIMITED_FPS;
        options.controls.invert_mouse = true;
        options.controls.mouse_sensitivity = 0.9;
        options.chat.visibility = ChatVisibility::Commands;
        options.chat.colors = false;
        options.save(&path).unwrap();

        let loaded = Options::load(&path);
        assert_eq!(loaded, options);
        assert_eq!(loaded.keys.get(Action::Forward), KeyCode::ArrowUp);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_missing_file_gives_the_defaults() {
        let folder = temp_folder("missing");
        assert_eq!(Options::load(&folder.join(FILE_NAME)), Options::default());
    }

    #[test]
    fn a_corrupt_file_gives_the_defaults_without_failing() {
        let folder = temp_folder("corrupt");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join(FILE_NAME);
        std::fs::write(&path, "this is [not toml").unwrap();
        assert_eq!(Options::load(&path), Options::default());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn one_broken_section_does_not_reset_the_others() {
        let folder = temp_folder("partial");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join(FILE_NAME);
        std::fs::write(
            &path,
            "[video]\nfov = \"wide\"\n[controls]\ninvert_mouse = true\n[keys]\nforward = \"ArrowUp\"\n",
        )
        .unwrap();
        let loaded = Options::load(&path);
        assert_eq!(loaded.video, VideoOptions::default(), "bad section resets");
        assert!(loaded.controls.invert_mouse, "good sections are kept");
        assert_eq!(loaded.keys.get(Action::Forward), KeyCode::ArrowUp);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn unknown_or_reserved_keys_in_the_file_keep_the_default() {
        let folder = temp_folder("keys");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join(FILE_NAME);
        std::fs::write(
            &path,
            "[keys]\nforward = \"Nonsense\"\njump = \"Escape\"\nsneak = \"KeyZ\"\nghost = \"KeyQ\"\n",
        )
        .unwrap();
        let loaded = Options::load(&path);
        assert_eq!(loaded.keys.get(Action::Forward), KeyCode::KeyW);
        assert_eq!(loaded.keys.get(Action::Jump), KeyCode::Space);
        assert_eq!(loaded.keys.get(Action::Sneak), KeyCode::KeyZ);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn out_of_range_numbers_are_clamped() {
        let folder = temp_folder("clamp");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join(FILE_NAME);
        std::fs::write(
            &path,
            "[video]\nrender_distance = 200\nfov = 500.0\nmax_fps = 1\n[controls]\nmouse_sensitivity = 7.0\n",
        )
        .unwrap();
        let loaded = Options::load(&path);
        assert_eq!(loaded.video.render_distance, 16);
        assert_eq!(loaded.video.fov, 110.0);
        assert_eq!(loaded.video.max_fps, 10);
        assert_eq!(loaded.controls.mouse_sensitivity, 1.0);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_non_finite_number_returns_to_its_default() {
        let mut options = Options::default();
        options.video.fov = f32::NAN;
        options.controls.mouse_sensitivity = f32::INFINITY;
        options.clamp();
        assert_eq!(options.video.fov, 70.0);
        assert_eq!(options.controls.mouse_sensitivity, 0.5);
    }

    #[test]
    fn saving_keeps_the_sections_this_version_does_not_know() {
        let folder = temp_folder("unknown-section");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join(FILE_NAME);
        std::fs::write(&path, "[future]\nanswer = 42\n").unwrap();
        Options::default().save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[future]") && text.contains("answer = 42"));
        assert!(text.contains("[keys]"));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn chat_visibility_cycles_and_maps_to_the_wire_byte() {
        let mut visibility = ChatVisibility::Full;
        let mut wires = Vec::new();
        for _ in 0..4 {
            wires.push(visibility.wire());
            visibility = visibility.next();
        }
        assert_eq!(wires, [0, 1, 2, 0]);
    }

    fn hud_with_a_red_bold_message() -> HudState {
        use std::time::Instant;
        use umicraft_render::{ChatLineView, ChatRunView};
        HudState {
            chat: vec![ChatLineView {
                runs: vec![ChatRunView {
                    text: "hi".to_string(),
                    color: [255, 85, 85],
                    bold: true,
                    italic: true,
                    underlined: true,
                    strikethrough: true,
                    obfuscated: true,
                }],
                received: Instant::now(),
            }],
            ..HudState::default()
        }
    }

    #[test]
    fn hidden_chat_shows_no_message() {
        let mut hud = hud_with_a_red_bold_message();
        let options = ChatOptions {
            visibility: ChatVisibility::Hidden,
            ..ChatOptions::default()
        };
        options.apply(&mut hud);
        assert!(hud.chat.is_empty());
    }

    #[test]
    fn colours_off_makes_every_message_plain_white() {
        let mut hud = hud_with_a_red_bold_message();
        let options = ChatOptions {
            colors: false,
            ..ChatOptions::default()
        };
        options.apply(&mut hud);
        let run = &hud.chat[0].runs[0];
        assert_eq!(run.color, [255, 255, 255]);
        assert!(!run.bold && !run.italic && !run.underlined);
        assert!(!run.strikethrough && !run.obfuscated);
        assert_eq!(run.text, "hi", "the text itself is kept");
    }

    #[test]
    fn the_default_chat_options_change_nothing() {
        let mut hud = hud_with_a_red_bold_message();
        let before = hud.clone();
        ChatOptions::default().apply(&mut hud);
        assert_eq!(hud, before);
    }

    #[test]
    fn the_look_factor_grows_with_the_sensitivity() {
        let low = ControlOptions {
            mouse_sensitivity: 0.0,
            ..ControlOptions::default()
        };
        let high = ControlOptions {
            mouse_sensitivity: 1.0,
            ..ControlOptions::default()
        };
        assert!(low.look_factor() < ControlOptions::default().look_factor());
        assert!(ControlOptions::default().look_factor() < high.look_factor());
        assert!(low.look_factor() > 0.0);
    }
}

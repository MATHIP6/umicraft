// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sound index: which `.ogg` files an event can play.
//!
//! It is the in-memory form of the `sounds.json` of MC 1.8, which MCP loads in
//! `SoundHandler.loadSoundRegistry` / `loadSoundResource` into one
//! `SoundEventAccessorComposite` per event. [`SoundIndex::parse`] reads the
//! file, [`SoundIndex::pick`] chooses an entry by weight.
//!
//! Event names carry no `minecraft:` domain: Umicraft only knows that domain.

mod parse;
mod pick;

use std::collections::HashMap;

pub use pick::{SeededRng, SoundRng};

use crate::category::SoundCategory;

/// One line of the `sounds` array of an event (`SoundList.SoundEntry`).
#[derive(Debug, Clone, PartialEq)]
pub enum SoundEntry {
    /// An `.ogg` file (`type: "file"`, the default).
    File {
        /// The path below `sounds/` without the extension, for example `dig/stone1`.
        path: String,
        /// The entry volume, multiplied into the request volume.
        volume: f32,
        /// The entry pitch, multiplied into the request pitch.
        pitch: f32,
        /// The weight of the entry in the random pick, at least 1.
        weight: u32,
        /// Whether the file is streamed instead of loaded whole (`stream: true`).
        stream: bool,
    },
    /// Another event (`type: "event"`); its weight is the total weight of that
    /// event, as `SoundHandler` computes it.
    Event {
        /// The name of the referenced event.
        event: String,
    },
}

/// The sounds of one event (`SoundEventAccessorComposite`).
#[derive(Debug, Clone, PartialEq)]
pub struct EventSounds {
    /// The volume category of the event.
    pub category: SoundCategory,
    /// The entries to pick from.
    pub entries: Vec<SoundEntry>,
    /// Whether this definition replaces an earlier one when merged (`replace`).
    pub replace: bool,
}

/// The chosen sound of an event, ready to be decoded and played.
#[derive(Debug, Clone, PartialEq)]
pub struct PickedSound {
    /// The path below `sounds/` without the extension.
    pub path: String,
    /// The entry volume.
    pub volume: f32,
    /// The entry pitch.
    pub pitch: f32,
    /// Whether the file should be streamed.
    pub stream: bool,
    /// The category of the event the pick started from.
    pub category: SoundCategory,
}

impl PickedSound {
    /// The resource path of the file: `sounds/<path>.ogg`.
    #[must_use]
    pub fn resource_path(&self) -> String {
        format!("sounds/{}.ogg", self.path)
    }
}

/// All the events of the loaded resource packs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SoundIndex {
    events: HashMap<String, EventSounds>,
}

impl SoundIndex {
    /// An index with no event.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The sounds of `event`, if it exists.
    #[must_use]
    pub fn event(&self, event: &str) -> Option<&EventSounds> {
        self.events.get(event)
    }

    /// The number of events.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether the index has no event.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Merges a later `sounds.json` (a resource pack on top) into this index,
    /// as `SoundHandler.loadSoundResource` does: an event with `replace: true`
    /// (or a new one) replaces the old definition, any other appends its
    /// entries to the old event and keeps the old category.
    pub fn merge(&mut self, later: SoundIndex) {
        for (name, incoming) in later.events {
            match self.events.get_mut(&name) {
                Some(existing) if !incoming.replace => existing.entries.extend(incoming.entries),
                _ => {
                    self.events.insert(name, incoming);
                }
            }
        }
    }
}

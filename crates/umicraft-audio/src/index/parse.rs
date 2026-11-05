// SPDX-License-Identifier: MIT OR Apache-2.0

//! Reading `sounds.json` (`SoundListSerializer.deserialize`).
//!
//! Validation follows MCP: a volume, pitch or weight that is not positive, an
//! unknown category or an unknown type invalidates the whole file, which
//! `SoundHandler` then skips with a warning.

use std::collections::HashMap;

use serde::Deserialize;

use super::{EventSounds, SoundEntry, SoundIndex};
use crate::category::SoundCategory;
use crate::error::AudioError;

const DEFAULT_DOMAIN: &str = "minecraft:";

#[derive(Deserialize)]
struct RawEvent {
    #[serde(default)]
    replace: bool,
    category: Option<String>,
    #[serde(default)]
    sounds: Vec<RawEntry>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawEntry {
    Name(String),
    Full(RawFull),
}

#[derive(Deserialize)]
struct RawFull {
    name: String,
    #[serde(rename = "type")]
    kind: Option<String>,
    volume: Option<f32>,
    pitch: Option<f32>,
    weight: Option<i64>,
    stream: Option<bool>,
}

impl SoundIndex {
    /// Parses the text of a `sounds.json`.
    ///
    /// # Errors
    ///
    /// [`AudioError::Index`] when the text is not the expected JSON, and
    /// [`AudioError::InvalidEntry`] when an entry breaks a rule of
    /// `SoundListSerializer` (see the module docs).
    pub fn parse(json: &str) -> Result<Self, AudioError> {
        let raw: HashMap<String, RawEvent> = serde_json::from_str(json)?;
        let mut events = HashMap::with_capacity(raw.len());
        for (name, event) in raw {
            let converted = convert_event(&name, event)?;
            events.insert(strip_domain(&name).to_owned(), converted);
        }
        Ok(Self { events })
    }
}

fn convert_event(name: &str, raw: RawEvent) -> Result<EventSounds, AudioError> {
    let invalid = |reason| AudioError::InvalidEntry {
        event: name.to_owned(),
        reason,
    };
    // The category defaults to `master` (`SoundCategory.MASTER.getCategoryName()`).
    let category = match raw.category {
        None => SoundCategory::Master,
        Some(text) => text.parse().map_err(|_| invalid("invalid category"))?,
    };
    let entries = raw
        .sounds
        .into_iter()
        .map(|entry| convert_entry(entry).map_err(invalid))
        .collect::<Result<_, _>>()?;
    Ok(EventSounds {
        category,
        entries,
        replace: raw.replace,
    })
}

fn convert_entry(raw: RawEntry) -> Result<SoundEntry, &'static str> {
    let full = match raw {
        RawEntry::Name(path) => return Ok(file(&path, 1.0, 1.0, 1, false)),
        RawEntry::Full(full) => full,
    };
    let volume = positive_f32(full.volume, "invalid volume")?;
    let pitch = positive_f32(full.pitch, "invalid pitch")?;
    let weight = match full.weight {
        None => 1,
        Some(w) => u32::try_from(w)
            .ok()
            .filter(|w| *w > 0)
            .ok_or("invalid weight")?,
    };
    match full.kind.as_deref() {
        None | Some("file") => Ok(file(
            &full.name,
            volume,
            pitch,
            weight,
            full.stream.unwrap_or(false),
        )),
        Some("event") => Ok(SoundEntry::Event {
            event: strip_domain(&full.name).to_owned(),
        }),
        Some(_) => Err("invalid type"),
    }
}

fn positive_f32(value: Option<f32>, reason: &'static str) -> Result<f32, &'static str> {
    match value {
        None => Ok(1.0),
        Some(v) if v > 0.0 => Ok(v),
        Some(_) => Err(reason),
    }
}

fn file(name: &str, volume: f32, pitch: f32, weight: u32, stream: bool) -> SoundEntry {
    SoundEntry::File {
        path: strip_domain(name).to_owned(),
        volume,
        pitch,
        weight,
        stream,
    }
}

fn strip_domain(name: &str) -> &str {
    name.strip_prefix(DEFAULT_DOMAIN).unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "test.step": {
            "category": "block",
            "sounds": [
                "step/a",
                { "name": "minecraft:step/b", "volume": 0.5, "pitch": 1.2, "weight": 3, "stream": true },
                { "name": "test.other", "type": "event" }
            ]
        },
        "test.other": { "replace": true, "category": "player", "sounds": ["x"] }
    }"#;

    #[test]
    fn parses_events_and_entries() {
        let index = SoundIndex::parse(SAMPLE).expect("sample is valid");
        assert_eq!(index.len(), 2);
        let step = index.event("test.step").expect("event exists");
        assert_eq!(step.category, SoundCategory::Blocks);
        assert_eq!(step.entries.len(), 3);
        assert_eq!(step.entries[0], file("step/a", 1.0, 1.0, 1, false));
        assert_eq!(step.entries[1], file("step/b", 0.5, 1.2, 3, true));
        assert_eq!(
            step.entries[2],
            SoundEntry::Event {
                event: "test.other".into()
            }
        );
        assert!(index.event("test.other").expect("exists").replace);
    }

    #[test]
    fn category_defaults_to_master() {
        let index = SoundIndex::parse(r#"{"a": {"sounds": ["x"]}}"#).expect("valid");
        assert_eq!(
            index.event("a").expect("exists").category,
            SoundCategory::Master
        );
    }

    #[test]
    fn rejects_what_the_serializer_rejects() {
        for bad in [
            r#"{"a": {"category": "nope", "sounds": []}}"#,
            r#"{"a": {"sounds": [{"name": "x", "volume": 0}]}}"#,
            r#"{"a": {"sounds": [{"name": "x", "pitch": -1}]}}"#,
            r#"{"a": {"sounds": [{"name": "x", "weight": 0}]}}"#,
            r#"{"a": {"sounds": [{"name": "x", "type": "bogus"}]}}"#,
        ] {
            assert!(
                matches!(SoundIndex::parse(bad), Err(AudioError::InvalidEntry { .. })),
                "{bad}"
            );
        }
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(matches!(
            SoundIndex::parse("[1]"),
            Err(AudioError::Index(_))
        ));
    }

    #[test]
    fn merge_appends_or_replaces() {
        let mut base =
            SoundIndex::parse(r#"{"a": {"category": "block", "sounds": ["one"]}}"#).expect("valid");
        let append = SoundIndex::parse(r#"{"a": {"sounds": ["two"]}}"#).expect("valid");
        base.merge(append);
        let a = base.event("a").expect("exists");
        assert_eq!(a.entries.len(), 2);
        assert_eq!(a.category, SoundCategory::Blocks);

        let replace =
            SoundIndex::parse(r#"{"a": {"replace": true, "sounds": ["three"]}}"#).expect("valid");
        base.merge(replace);
        assert_eq!(base.event("a").expect("exists").entries.len(), 1);
    }
}

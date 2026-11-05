// SPDX-License-Identifier: MIT OR Apache-2.0

//! Parsing of the status-response payload (`server list ping` JSON).
//!
//! Real 1.8 servers vary their shape generously (description may be a plain
//! string, a nested component object, or an array of components), so optional
//! fields and tolerant MOTD text extraction rule here.

use serde::Deserialize;

/// Parsed server list payload.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerStatus {
    /// Advertised version block (name, protocol).
    #[serde(default)]
    pub version: Option<VersionBlock>,
    /// Advertised player counts.
    #[serde(default)]
    pub players: Option<PlayersBlock>,
    /// Raw MOTD JSON: string, component object, or component array.
    #[serde(default)]
    pub description: Option<serde_json::Value>,
    /// Base64 PNG, optional.
    #[serde(default)]
    pub favicon: Option<String>,
}

/// The server list's `version` block.
#[derive(Debug, Clone, Deserialize)]
pub struct VersionBlock {
    /// Display name, e.g. `Paper 1.8.8`.
    #[serde(default)]
    pub name: Option<String>,
    /// Advertised protocol version (often 47 or a compatibility value).
    #[serde(default)]
    pub protocol: Option<i32>,
}

/// The server list's `players` block.
#[derive(Debug, Clone, Deserialize)]
pub struct PlayersBlock {
    /// Players currently online (may be absent on proxies).
    #[serde(default)]
    pub online: Option<i64>,
    /// Player cap.
    #[serde(default)]
    pub max: Option<i64>,
    /// A few online player names (the hover list), server-chosen and optional.
    #[serde(default)]
    pub sample: Option<Vec<PlayerSample>>,
}

/// One entry of the `players.sample` list.
#[derive(Debug, Clone, Deserialize)]
pub struct PlayerSample {
    /// Player name; servers use this field for arbitrary text (plugins show
    /// extra hover lines), so it may contain `§` codes.
    #[serde(default)]
    pub name: Option<String>,
}

impl ServerStatus {
    /// Flattens `description` into text with legacy `§` codes: the JSON
    /// `color`, `bold`, `italic`, `underlined`, `strikethrough` and
    /// `obfuscated` of every component become the codes the server list
    /// draws, as `IChatComponent.getFormattedText` does (MCP 1.8). Codes
    /// already written in the text are kept.
    ///
    /// Returns an empty string when there is no description at all.
    #[must_use]
    pub fn motd_text(&self) -> String {
        self.description
            .as_ref()
            .map(json_to_legacy_text)
            .unwrap_or_default()
    }
}

/// Recursively converts a chat-JSON value to concatenated plain text.
///
/// Handles the 1.8 shapes: a bare string, a `{"text": ..., "extra": [...]}` /
/// `{"translate": ...}` component object, and arrays of components; unknown
/// shapes contribute nothing rather than erroring.
#[must_use]
pub fn json_to_plain_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(items) => items.iter().map(json_to_plain_text).collect(),
        serde_json::Value::Object(map) => {
            let mut out = String::new();
            if let Some(text) = map.get("text").and_then(serde_json::Value::as_str) {
                out.push_str(text);
            }
            if let Some(extra) = map.get("extra") {
                out.push_str(&json_to_plain_text(extra));
            }
            out
        }
        _ => String::new(),
    }
}

/// The legacy code letter of each JSON colour name, in `§0`..`§f` order.
const COLOR_NAMES: [&str; 16] = [
    "black",
    "dark_blue",
    "dark_green",
    "dark_aqua",
    "dark_red",
    "dark_purple",
    "gold",
    "gray",
    "dark_gray",
    "blue",
    "green",
    "aqua",
    "red",
    "light_purple",
    "yellow",
    "white",
];
/// The JSON style flags and their legacy code letters.
const FLAGS: [(&str, char); 5] = [
    ("obfuscated", 'k'),
    ("bold", 'l'),
    ("strikethrough", 'm'),
    ("underlined", 'n'),
    ("italic", 'o'),
];
/// The deepest `extra` nesting followed; a hostile server must not overflow
/// the stack.
const MAX_DEPTH: usize = 16;

/// The style of a component with what it inherits from its parent.
#[derive(Debug, Clone, Default, PartialEq)]
struct LegacyStyle {
    color: Option<char>,
    flags: [bool; 5],
}

impl LegacyStyle {
    /// This style with the fields `value` sets on top of it.
    fn with(&self, value: &serde_json::Value) -> Self {
        let color = value
            .get("color")
            .and_then(serde_json::Value::as_str)
            .and_then(|name| {
                COLOR_NAMES
                    .iter()
                    .position(|candidate| candidate.eq_ignore_ascii_case(name))
            })
            .and_then(|index| char::from_digit(index as u32, 16))
            .or(self.color);
        let mut flags = self.flags;
        for (flag, (name, _)) in flags.iter_mut().zip(FLAGS) {
            if let Some(set) = value.get(name).and_then(serde_json::Value::as_bool) {
                *flag = set;
            }
        }
        Self { color, flags }
    }

    /// The codes that switch to this style from a reset.
    fn codes(&self) -> String {
        let mut codes = String::new();
        if let Some(color) = self.color {
            codes.extend(['§', color]);
        }
        for (on, (_, letter)) in self.flags.iter().zip(FLAGS) {
            if *on {
                codes.extend(['§', letter]);
            }
        }
        codes
    }
}

/// Converts a chat-JSON value to text with legacy `§` codes (see
/// [`ServerStatus::motd_text`]). Unknown shapes contribute nothing.
#[must_use]
pub fn json_to_legacy_text(value: &serde_json::Value) -> String {
    let mut out = String::new();
    let mut styled = false;
    walk_legacy(value, &LegacyStyle::default(), 0, &mut styled, &mut out);
    out
}

/// Appends the text of `value` and its `extra` list, each piece preceded by
/// the codes of its own style (a reset first once anything was styled).
fn walk_legacy(
    value: &serde_json::Value,
    parent: &LegacyStyle,
    depth: usize,
    styled: &mut bool,
    out: &mut String,
) {
    if depth > MAX_DEPTH {
        return;
    }
    match value {
        serde_json::Value::String(text) => push_styled(text, parent, styled, out),
        serde_json::Value::Array(items) => {
            for item in items {
                walk_legacy(item, parent, depth + 1, styled, out);
            }
        }
        serde_json::Value::Object(map) => {
            let style = parent.with(value);
            if let Some(text) = map.get("text").and_then(serde_json::Value::as_str) {
                push_styled(text, &style, styled, out);
            }
            if let Some(extra) = map.get("extra") {
                walk_legacy(extra, &style, depth + 1, styled, out);
            }
        }
        _ => {}
    }
}

fn push_styled(text: &str, style: &LegacyStyle, styled: &mut bool, out: &mut String) {
    if text.is_empty() {
        return;
    }
    let codes = style.codes();
    if !codes.is_empty() || *styled {
        out.push_str("§r");
        out.push_str(&codes);
        *styled = !codes.is_empty();
    }
    out.push_str(text);
}

/// Parses a raw login/play `Disconnect` reason (chat JSON) to plain text,
/// falling back to the raw body when it is not parseable JSON.
#[must_use]
pub fn disconnect_reason(raw: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(value) => json_to_plain_text(&value),
        Err(_) => raw.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_status_shape() {
        let status: ServerStatus =
            serde_json::from_str(r#"{"version":{"name":"Paper 1.8.8","protocol":47},"players":{"online":0,"max":20},"description":{"text":"hello §aworld"}}"#).unwrap();
        assert_eq!(status.motd_text(), "hello §aworld");
        assert_eq!(status.players.as_ref().unwrap().max, Some(20));
    }

    #[test]
    fn motd_accepts_string_component_and_array() {
        let string_only: ServerStatus = serde_json::from_str(r#"{"description":"plain"}"#).unwrap();
        assert_eq!(string_only.motd_text(), "plain");

        let arrayed: ServerStatus =
            serde_json::from_str(r#"{"description":[{"text":"a"},{"text":"b"}]}"#).unwrap();
        assert_eq!(arrayed.motd_text(), "ab");

        let nested: ServerStatus =
            serde_json::from_str(r#"{"description":{"text":"base","extra":[{"text":" plus"}]}}"#)
                .unwrap();
        assert_eq!(nested.motd_text(), "base plus");
    }

    #[test]
    fn motd_keeps_json_colours_and_styles_as_legacy_codes() {
        let status: ServerStatus = serde_json::from_str(
            r##"{"description":{"text":"Prisme","color":"light_purple","bold":true,
            "extra":[{"text":" - Practice","color":"white","bold":false},{"text":"!"}]}}"##,
        )
        .unwrap();
        assert_eq!(status.motd_text(), "§r§d§lPrisme§r§f - Practice§r§d§l!");
    }

    #[test]
    fn motd_resets_after_a_styled_piece_and_ignores_unknown_colours() {
        let status: ServerStatus = serde_json::from_str(
            r#"{"description":[{"text":"a","color":"red"},{"text":"b"},{"text":"c","color":"nope"}]}"#,
        )
        .unwrap();
        assert_eq!(status.motd_text(), "§r§ca§rbc");
    }

    #[test]
    fn motd_nesting_is_capped() {
        let mut json = serde_json::json!({"text": "x"});
        for _ in 0..100 {
            json = serde_json::json!({"text": "y", "extra": [json]});
        }
        assert!(!json_to_legacy_text(&json).contains('x'));
    }

    #[test]
    fn parses_the_player_sample() {
        let status: ServerStatus = serde_json::from_str(
            r#"{"players":{"online":1,"max":2,"sample":[{"name":"Ann","id":"x"},{}]}}"#,
        )
        .unwrap();
        let sample = status.players.unwrap().sample.unwrap();
        assert_eq!(sample[0].name.as_deref(), Some("Ann"));
        assert_eq!(sample[1].name, None);
    }

    #[test]
    fn disconnect_reason_falls_back_to_raw() {
        assert_eq!(disconnect_reason(r#"{"text":"bye"}"#), "bye");
        assert_eq!(disconnect_reason("not json"), "not json");
    }
}

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
}

impl ServerStatus {
    /// Flattens `description` into readable text (components concatenated in
    /// order; `"§"` color codes left as-is for M1).
    ///
    /// Returns an empty string when there is no description at all.
    #[must_use]
    pub fn motd_text(&self) -> String {
        self.description
            .as_ref()
            .map(json_to_plain_text)
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
    fn disconnect_reason_falls_back_to_raw() {
        assert_eq!(disconnect_reason(r#"{"text":"bye"}"#), "bye");
        assert_eq!(disconnect_reason("not json"), "not json");
    }
}

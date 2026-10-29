// SPDX-License-Identifier: MIT OR Apache-2.0

//! The chat: the plain text of a 1.8 chat JSON component.
//!
//! The server sends the chat as a JSON component (MCP
//! `ChatComponentText`), not as plain text. The HUD draws plain strings, so
//! the client flattens the component into the line a player would read.
//!
//! Only the parts of the format the vanilla server uses for its system
//! messages are handled: the `text` and `translate` fields, the `with`
//! arguments and the `extra` list. A component that cannot be parsed falls
//! back to the raw JSON, so no message is ever lost.

use std::sync::OnceLock;

use serde_json::Value;
use umicraft_game::chat_component::{self, Translations};

/// The system messages of the vanilla server that the client can flatten.
///
/// The keys are the `translate` values of the 1.8 language file; the values
/// are the English texts of MCP `net/minecraft/util/ChatComponentTranslation`
/// and the 1.8 `en_US.lang`.
const TRANSLATIONS: [(&str, &str); 8] = [
    ("multiplayer.player.joined", "%s joined the game"),
    ("multiplayer.player.left", "%s left the game"),
    ("chat.type.text", "%s: %s"),
    ("chat.type.announcement", "[%s] %s"),
    ("multiplayer.disconnect.generic", "Disconnected"),
    ("multiplayer.disconnect.kicked", "Kicked by an operator: %s"),
    (
        "commands.generic.notFound",
        "Unknown command. Type /help for help.",
    ),
    ("death.attack.player", "%s was slain by %s"),
];

/// The plain text of a chat JSON component.
///
/// The function never fails: a malformed component is returned unchanged, so
/// the player still sees something.
#[must_use]
pub fn plain_text(json: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return json.to_string();
    };
    let text = component_text(&value);
    if text.is_empty() {
        json.to_string()
    } else {
        text
    }
}

/// The plain text of a chat JSON component, empty when the component is.
///
/// [`plain_text`] falls back to the raw JSON for an empty result so that no
/// chat message is lost. The tab-list header, the title and the action bar
/// are legitimately empty (`{"text":""}` clears them), so they use this
/// variant. A malformed component is still returned unchanged.
#[must_use]
pub fn flatten(json: &str) -> String {
    static TRANSLATIONS: OnceLock<Translations> = OnceLock::new();
    let translations = TRANSLATIONS.get_or_init(Translations::builtin);
    chat_component::plain(&chat_component::parse(json, translations))
}

/// The text of one component and of its children.
fn component_text(value: &Value) -> String {
    // An argument of a translation may be a plain string.
    if let Value::String(direct) = value {
        return direct.clone();
    }
    let mut text = String::new();
    if let Some(Value::String(direct)) = value.get("text") {
        text.push_str(direct);
    }
    if let Some(Value::String(key)) = value.get("translate") {
        let arguments = arguments(value);
        text.push_str(&translate(key, &arguments));
    }
    if let Some(Value::Array(extra)) = value.get("extra") {
        for child in extra {
            text.push_str(&component_text(child));
        }
    }
    text
}

/// The `with` arguments of a component, as plain strings.
fn arguments(value: &Value) -> Vec<String> {
    match value.get("with") {
        Some(Value::Array(items)) => items.iter().map(component_text).collect(),
        _ => Vec::new(),
    }
}

/// The text of a translated key.
///
/// An unknown key keeps its own name, which is what the vanilla client shows
/// when the language file misses an entry.
fn translate(key: &str, arguments: &[String]) -> String {
    let template = TRANSLATIONS
        .iter()
        .find(|(name, _)| *name == key)
        .map_or("", |(_, text)| *text);
    if template.is_empty() {
        return key.to_string();
    }
    let mut out = String::new();
    let mut rest = template;
    let mut index = 0;
    while let Some(position) = rest.find("%s") {
        out.push_str(&rest[..position]);
        out.push_str(arguments.get(index).map_or("", String::as_str));
        rest = &rest[position + 2..];
        index += 1;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod flatten_tests {
    use super::{flatten, plain_text};

    #[test]
    fn an_empty_component_stays_empty_for_the_overlays() {
        assert_eq!(flatten(r#"{"text":""}"#), "");
        // The chat keeps the raw JSON so no message is lost.
        assert_eq!(plain_text(r#"{"text":""}"#), r#"{"text":""}"#);
    }

    #[test]
    fn flatten_joins_the_text_and_the_extra_children() {
        assert_eq!(
            flatten(r#"{"text":"a","extra":[{"text":"b"},{"text":"c"}]}"#),
            "abc"
        );
    }

    #[test]
    fn a_malformed_component_is_returned_unchanged() {
        assert_eq!(flatten("not json"), "not json");
    }

    #[test]
    fn flatten_drops_the_legacy_colour_codes_the_font_cannot_draw() {
        assert_eq!(flatten("{\"text\":\"\u{a7}cRed \u{a7}lBold\"}"), "Red Bold");
    }

    #[test]
    fn flatten_translates_the_known_system_messages() {
        assert_eq!(
            flatten(r#"{"translate":"multiplayer.player.joined","with":["Alex"]}"#),
            "Alex joined the game"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_text_component_is_used_as_it_is() {
        assert_eq!(plain_text("{\"text\":\"hello\"}"), "hello");
    }

    #[test]
    fn the_join_message_is_flattened() {
        let json = "{\"color\":\"yellow\",\"translate\":\"multiplayer.player.joined\",\"with\":[\"Steve\"]}";
        assert_eq!(plain_text(json), "Steve joined the game");
    }

    #[test]
    fn a_chat_line_keeps_the_player_and_the_message() {
        let json =
            "{\"translate\":\"chat.type.text\",\"with\":[{\"text\":\"Steve\"},{\"text\":\"hi\"}]}";
        assert_eq!(plain_text(json), "Steve: hi");
    }

    #[test]
    fn the_extra_children_are_appended() {
        let json = "{\"text\":\"a\",\"extra\":[{\"text\":\"b\"}]}";
        assert_eq!(plain_text(json), "ab");
    }

    #[test]
    fn a_malformed_component_falls_back_to_the_raw_json() {
        assert_eq!(plain_text("not json"), "not json");
        assert_eq!(plain_text("{\"unknown\":1}"), "{\"unknown\":1}");
    }

    #[test]
    fn an_unknown_translation_keeps_its_key() {
        assert_eq!(plain_text("{\"translate\":\"some.key\"}"), "some.key");
    }
}

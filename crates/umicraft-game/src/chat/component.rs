// SPDX-License-Identifier: MIT OR Apache-2.0

//! Chat components: the JSON a 1.8 server sends for chat, titles, the tab
//! list and the sidebar, flattened into styled runs of text (M10, GAM-6).
//!
//! The format follows MCP `IChatComponent`, `ChatStyle` and its JSON
//! serializer (`ChatComponentText`, `ChatComponentTranslation`,
//! `ChatComponentScore`, the `extra` list, the style fields and their
//! inheritance from the parent) and the legacy `§` codes of
//! `FontRenderer.renderStringAtPos`.
//!
//! The parser never fails on server data. A malformed component becomes one
//! run of raw text, the recursion depth and the total text are capped, and a
//! `translate` key the language table does not know is shown as the key.

use serde_json::Value;

use super::events::{ClickAction, ClickEvent, HoverEvent};
use super::lang::{expand_format, LangTable, Piece};
use super::style::{Color, Style, SECTION};

/// The deepest `extra` / `with` nesting the parser follows. A hostile server
/// could otherwise nest a component until the stack overflows.
pub const MAX_DEPTH: usize = 16;
/// The most characters one parsed component may hold.
pub const MAX_CHARS: usize = 32_767;
/// The most runs one parsed component may hold.
pub const MAX_RUNS: usize = 4096;

/// A piece of text with one style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// The text.
    pub text: String,
    /// The style.
    pub style: Style,
    /// The click event of the component the run came from.
    pub click: Option<ClickEvent>,
    /// The hover event of the component the run came from.
    pub hover: Option<HoverEvent>,
    /// The text a Shift-click appends to the input line.
    pub insertion: Option<String>,
}

impl Run {
    /// An unstyled run without events.
    #[must_use]
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: Style::default(),
            click: None,
            hover: None,
            insertion: None,
        }
    }

    /// The same run with other text, as wrapping splits a run.
    #[must_use]
    pub fn with_text(&self, text: String) -> Self {
        Self {
            text,
            ..self.clone()
        }
    }
}

/// The style fields a component sets itself; `None` inherits the parent's.
#[derive(Debug, Clone, Default)]
struct Inherited {
    color: Option<Color>,
    bold: Option<bool>,
    italic: Option<bool>,
    underlined: Option<bool>,
    strikethrough: Option<bool>,
    obfuscated: Option<bool>,
    click: Option<ClickEvent>,
    hover: Option<HoverEvent>,
    insertion: Option<String>,
}

impl Inherited {
    /// This style with the fields `value` sets on top.
    fn with(&self, value: &Value, lang: &LangTable) -> Self {
        let flag =
            |name: &str, parent: Option<bool>| value.get(name).and_then(Value::as_bool).or(parent);
        Self {
            color: value
                .get("color")
                .and_then(Value::as_str)
                .and_then(Color::from_name)
                .or(self.color),
            bold: flag("bold", self.bold),
            italic: flag("italic", self.italic),
            underlined: flag("underlined", self.underlined),
            strikethrough: flag("strikethrough", self.strikethrough),
            obfuscated: flag("obfuscated", self.obfuscated),
            click: value
                .get("clickEvent")
                .and_then(click_event)
                .or_else(|| self.click.clone()),
            hover: value
                .get("hoverEvent")
                .and_then(|event| hover_event(event, lang))
                .or_else(|| self.hover.clone()),
            insertion: value
                .get("insertion")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| self.insertion.clone()),
        }
    }

    fn style(&self) -> Style {
        Style {
            color: self.color,
            bold: self.bold.unwrap_or(false),
            italic: self.italic.unwrap_or(false),
            underlined: self.underlined.unwrap_or(false),
            strikethrough: self.strikethrough.unwrap_or(false),
            obfuscated: self.obfuscated.unwrap_or(false),
        }
    }
}

fn click_event(value: &Value) -> Option<ClickEvent> {
    Some(ClickEvent {
        action: ClickAction::from_name(value.get("action")?.as_str()?)?,
        value: value.get("value")?.as_str()?.to_string(),
    })
}

/// A `hoverEvent` object. Its `value` is itself a component: shown as text
/// for `show_text`, and read as NBT text for the other actions.
fn hover_event(value: &Value, lang: &LangTable) -> Option<HoverEvent> {
    let action = value.get("action")?.as_str()?.to_ascii_lowercase();
    let content = value.get("value")?;
    if action == "show_text" {
        return Some(HoverEvent::ShowText(parse_value(content, lang)));
    }
    let text = plain(&parse_value(content, lang));
    match action.as_str() {
        "show_item" => Some(HoverEvent::ShowItem(text)),
        "show_entity" => Some(HoverEvent::ShowEntity(text)),
        "show_achievement" => Some(HoverEvent::ShowAchievement(text)),
        _ => None,
    }
}

/// What the parser has produced so far, with its budgets.
struct Output {
    runs: Vec<Run>,
    chars: usize,
}

impl Output {
    fn push(&mut self, text: &str, style: &Inherited) {
        if text.is_empty() || self.runs.len() >= MAX_RUNS || self.chars >= MAX_CHARS {
            return;
        }
        let room = MAX_CHARS - self.chars;
        let text: String = text.chars().take(room).collect();
        self.chars += text.chars().count();
        self.runs.push(Run {
            text,
            style: style.style(),
            click: style.click.clone(),
            hover: style.hover.clone(),
            insertion: style.insertion.clone(),
        });
    }
}

/// Parses a chat JSON component into runs, expanding the legacy `§` codes.
///
/// Malformed JSON becomes a single run holding the raw text, so no message
/// is lost. A component with no text yields no runs.
#[must_use]
pub fn parse(json: &str, lang: &LangTable) -> Vec<Run> {
    match serde_json::from_str::<Value>(json) {
        Ok(value) => parse_value(&value, lang),
        Err(_) => expand_legacy(vec![Run::plain(
            json.chars().take(MAX_CHARS).collect::<String>(),
        )]),
    }
}

/// The runs of a component the client writes itself. The text is never
/// mistaken for JSON (a player named `null` or `123`), but its `§` codes
/// still apply.
#[must_use]
pub fn local(text: &str) -> Vec<Run> {
    expand_legacy(vec![Run::plain(
        text.chars().take(MAX_CHARS).collect::<String>(),
    )])
}

/// The text of a component with the JSON styles dropped but the legacy `§`
/// codes inside the text kept (`IChatComponent.getUnformattedText`).
///
/// The action bar is drawn from this text: 1.8 loses the colour of the
/// component but the font renderer still obeys a code written in the text.
#[must_use]
pub fn unformatted_text(json: &str, lang: &LangTable) -> String {
    match serde_json::from_str::<Value>(json) {
        Ok(value) => plain(&collect(&value, lang)),
        Err(_) => json.chars().take(MAX_CHARS).collect(),
    }
}

fn parse_value(value: &Value, lang: &LangTable) -> Vec<Run> {
    expand_legacy(collect(value, lang))
}

fn collect(value: &Value, lang: &LangTable) -> Vec<Run> {
    let mut output = Output {
        runs: Vec::new(),
        chars: 0,
    };
    walk(value, &Inherited::default(), lang, 0, &mut output);
    output.runs
}

/// The text of the runs, without any style.
#[must_use]
pub fn plain(runs: &[Run]) -> String {
    runs.iter().map(|run| run.text.as_str()).collect()
}

fn walk(value: &Value, parent: &Inherited, lang: &LangTable, depth: usize, output: &mut Output) {
    if depth > MAX_DEPTH {
        return;
    }
    match value {
        Value::String(text) => output.push(text, parent),
        Value::Number(number) => output.push(&number.to_string(), parent),
        Value::Bool(flag) => output.push(&flag.to_string(), parent),
        Value::Array(items) => {
            // The first element is the component; the others are appended as
            // its siblings, so they inherit its style.
            let mut items = items.iter();
            let Some(first) = items.next() else {
                return;
            };
            walk(first, parent, lang, depth + 1, output);
            let sibling_parent = match first {
                Value::Object(_) => parent.with(first, lang),
                _ => parent.clone(),
            };
            for sibling in items {
                walk(sibling, &sibling_parent, lang, depth + 1, output);
            }
        }
        Value::Object(_) => {
            let style = parent.with(value, lang);
            if let Some(Value::String(text)) = value.get("text") {
                output.push(text, &style);
            }
            if let Some(score) = value.get("score") {
                let shown = score.get("value").and_then(Value::as_str).unwrap_or("");
                output.push(shown, &style);
            }
            if let Some(Value::String(key)) = value.get("translate") {
                let arguments = match value.get("with") {
                    Some(Value::Array(items)) => items.as_slice(),
                    _ => &[],
                };
                translate(key, arguments, &style, lang, depth, output);
            }
            if let Some(Value::Array(extra)) = value.get("extra") {
                for child in extra {
                    walk(child, &style, lang, depth + 1, output);
                }
            }
        }
        Value::Null => {}
    }
}

/// Expands a `translate` component: `%s`, `%1$s` and `%%` in the format.
///
/// A format the chat cannot expand shows its key, as a missing entry does.
fn translate(
    key: &str,
    arguments: &[Value],
    style: &Inherited,
    lang: &LangTable,
    depth: usize,
    output: &mut Output,
) {
    let format = lang.get(key).unwrap_or(key);
    let Ok(pieces) = expand_format(format) else {
        output.push(key, style);
        return;
    };
    for piece in pieces {
        match piece {
            Piece::Text(text) => output.push(&text, style),
            Piece::Argument(index) => {
                if let Some(argument) = arguments.get(index) {
                    walk(argument, style, lang, depth + 1, output);
                }
            }
        }
    }
}

/// Splits runs at the legacy `§` codes and applies them.
///
/// A colour code resets the other styles; `k l m n o` switch one on; `r`
/// resets everything; an unknown code is treated as white, like the
/// reference font renderer (see [`Style::apply_code`]).
fn expand_legacy(runs: Vec<Run>) -> Vec<Run> {
    if runs.iter().all(|run| !run.text.contains(SECTION)) {
        return runs;
    }
    let mut expanded = Vec::with_capacity(runs.len());
    for run in runs {
        let mut style = run.style;
        let mut buffer = String::new();
        let mut chars = run.text.chars();
        while let Some(character) = chars.next() {
            if character != SECTION {
                buffer.push(character);
                continue;
            }
            let Some(code) = chars.next() else {
                buffer.push(character);
                break;
            };
            if !buffer.is_empty() {
                expanded.push(Run {
                    style,
                    ..run.with_text(std::mem::take(&mut buffer))
                });
            }
            style.apply_code(code);
        }
        if !buffer.is_empty() {
            expanded.push(Run {
                style,
                ..run.with_text(buffer)
            });
        }
    }
    expanded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runs(json: &str) -> Vec<Run> {
        parse(json, &LangTable::builtin())
    }

    fn red() -> Option<Color> {
        Color::from_name("red")
    }

    #[test]
    fn a_plain_component_is_one_unstyled_run() {
        let parsed = runs(r#"{"text":"hello"}"#);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].text, "hello");
        assert_eq!(parsed[0].style, Style::default());
    }

    #[test]
    fn a_child_inherits_the_style_of_its_parent_and_adds_its_own() {
        let parsed = runs(r#"{"text":"A","color":"red","extra":[{"text":"B","bold":true}]}"#);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].style.color, red());
        assert!(!parsed[0].style.bold);
        assert_eq!(parsed[1].style.color, red(), "colour is inherited");
        assert!(parsed[1].style.bold);
    }

    #[test]
    fn a_child_can_override_the_inherited_style() {
        let parsed = runs(
            r#"{"text":"A","color":"red","bold":true,"extra":[{"text":"B","color":"blue","bold":false}]}"#,
        );
        assert_eq!(parsed[1].style.color, Color::from_name("blue"));
        assert!(!parsed[1].style.bold);
    }

    #[test]
    fn italic_is_a_style_flag_that_is_inherited() {
        let parsed = runs(r#"{"text":"A","italic":true,"extra":[{"text":"B"}]}"#);
        assert!(parsed[0].style.italic && parsed[1].style.italic);
    }

    #[test]
    fn a_bare_string_and_an_array_are_components_too() {
        assert_eq!(plain(&runs(r#""hi""#)), "hi");
        let parsed = runs(r#"[{"text":"a","color":"gold"},{"text":"b"},"c"]"#);
        assert_eq!(plain(&parsed), "abc");
        assert_eq!(
            parsed[1].style.color,
            Color::from_name("gold"),
            "siblings inherit the first component's style"
        );
    }

    #[test]
    fn translate_fills_the_arguments_in_order() {
        let parsed = runs(r#"{"translate":"chat.type.text","with":["Steve","hi"]}"#);
        assert_eq!(plain(&parsed), "<Steve> hi");
        let kicked = runs(r#"{"translate":"multiplayer.disconnect.kicked","with":["bye"]}"#);
        assert_eq!(plain(&kicked), "Kicked by an operator: bye");
    }

    #[test]
    fn translate_arguments_keep_their_own_style_on_top_of_the_parent() {
        let parsed = runs(
            r#"{"translate":"multiplayer.player.joined","color":"yellow","with":[{"text":"Alex","bold":true}]}"#,
        );
        assert_eq!(plain(&parsed), "Alex joined the game");
        let alex = &parsed[0];
        assert_eq!(alex.text, "Alex");
        assert!(alex.style.bold);
        assert_eq!(alex.style.color, Color::from_name("yellow"));
    }

    #[test]
    fn numbered_arguments_and_escaped_percent_signs_work() {
        let mut table = LangTable::default();
        table.extend(LangTable::from_lang("k=%2$s then %1$s, 100%% sure\n"));
        let parsed = parse(r#"{"translate":"k","with":["a","b"]}"#, &table);
        assert_eq!(plain(&parsed), "b then a, 100% sure");
    }

    #[test]
    fn a_lang_file_entry_with_a_number_conversion_takes_a_string_argument() {
        let table = LangTable::from_lang("k=%s has %d points\n");
        let parsed = parse(r#"{"translate":"k","with":["Alex",12]}"#, &table);
        assert_eq!(plain(&parsed), "Alex has 12 points");
    }

    #[test]
    fn an_unknown_key_is_shown_as_the_key() {
        assert_eq!(
            plain(&runs(r#"{"translate":"some.unknown.key"}"#)),
            "some.unknown.key"
        );
    }

    #[test]
    fn a_format_the_chat_cannot_expand_shows_the_key() {
        let table = LangTable::from_lang("bad=value %x\n");
        assert_eq!(plain(&parse(r#"{"translate":"bad"}"#, &table)), "bad");
    }

    #[test]
    fn a_score_component_shows_its_value() {
        let parsed = runs(r#"{"score":{"name":"Steve","objective":"kills","value":"7"}}"#);
        assert_eq!(plain(&parsed), "7");
    }

    #[test]
    fn legacy_codes_split_the_text_and_apply_the_styles() {
        let parsed = runs("{\"text\":\"a\u{a7}cb\u{a7}lc\u{a7}rd\"}");
        let texts: Vec<_> = parsed.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(texts, ["a", "b", "c", "d"]);
        assert_eq!(parsed[0].style.color, None);
        assert_eq!(parsed[1].style.color, red());
        assert!(!parsed[1].style.bold);
        assert!(parsed[2].style.bold);
        assert_eq!(parsed[2].style.color, red(), "bold keeps the colour");
        assert_eq!(parsed[3].style, Style::default(), "r resets everything");
    }

    #[test]
    fn a_legacy_code_extends_the_json_style_it_follows() {
        let parsed = runs("{\"text\":\"\u{a7}ox\",\"color\":\"red\",\"bold\":true}");
        assert!(parsed[0].style.italic && parsed[0].style.bold);
        assert_eq!(parsed[0].style.color, red());
    }

    #[test]
    fn a_colour_code_clears_the_other_styles() {
        let parsed = runs("{\"text\":\"\u{a7}l\u{a7}nx\u{a7}ay\"}");
        assert!(parsed[0].style.bold && parsed[0].style.underlined);
        assert!(!parsed[1].style.bold && !parsed[1].style.underlined);
        assert_eq!(parsed[1].style.color, Color::from_code('a'));
    }

    #[test]
    fn an_unknown_code_is_white_and_a_lone_section_sign_is_text() {
        let parsed = runs("{\"text\":\"\u{a7}zx\"}");
        assert_eq!(parsed[0].style.color, Some(Color::WHITE));
        let lone = runs("{\"text\":\"end\u{a7}\"}");
        assert_eq!(plain(&lone), "end\u{a7}");
    }

    #[test]
    fn malformed_json_is_kept_as_raw_text() {
        let parsed = runs("not json at all");
        assert_eq!(plain(&parsed), "not json at all");
    }

    #[test]
    fn raw_text_still_honours_legacy_codes() {
        let parsed = parse("\u{a7}chello", &LangTable::default());
        assert_eq!(parsed[0].style.color, red());
    }

    #[test]
    fn local_text_is_never_json_but_keeps_its_codes() {
        assert_eq!(plain(&local("null")), "null");
        assert_eq!(local("\u{a7}ahi")[0].style.color, Color::from_code('a'));
    }

    #[test]
    fn an_empty_component_has_no_runs() {
        assert!(runs(r#"{"text":""}"#).is_empty());
        assert!(runs("null").is_empty());
        assert!(runs("[]").is_empty());
    }

    #[test]
    fn extreme_nesting_returns_instead_of_overflowing_the_stack() {
        let mut json = String::from(r#"{"text":"x"}"#);
        for _ in 0..5000 {
            json = format!(r#"{{"text":"y","extra":[{json}]}}"#);
        }
        // serde_json rejects the nesting itself (recursion limit), which
        // takes the raw-text path; either way the call returns.
        let parsed = runs(&json);
        assert!(!parsed.is_empty());
    }

    #[test]
    fn nesting_within_the_serde_limit_is_cut_at_the_depth_cap() {
        let mut json = String::from(r#"{"text":"leaf"}"#);
        for _ in 0..40 {
            json = format!(r#"{{"text":"n","extra":[{json}]}}"#);
        }
        let parsed = runs(&json);
        assert!(!plain(&parsed).contains("leaf"), "the leaf is too deep");
        assert!(plain(&parsed).len() <= MAX_DEPTH + 1);
    }

    #[test]
    fn the_total_text_is_capped() {
        let big = "a".repeat(MAX_CHARS);
        let json = format!(r#"{{"text":"{big}","extra":[{{"text":"more"}}]}}"#);
        let parsed = runs(&json);
        assert_eq!(plain(&parsed).chars().count(), MAX_CHARS);
    }

    #[test]
    fn click_hover_and_insertion_are_kept_and_inherited() {
        let parsed = runs(
            r#"{"text":"a","clickEvent":{"action":"suggest_command","value":"/home"},
                "hoverEvent":{"action":"show_text","value":{"text":"tip"}},
                "insertion":"Steve",
                "extra":[{"text":"b"}]}"#,
        );
        for run in &parsed {
            let click = run.click.as_ref().expect("click");
            assert_eq!(click.action, ClickAction::SuggestCommand);
            assert_eq!(click.value, "/home");
            assert_eq!(
                run.hover,
                Some(HoverEvent::ShowText(vec![Run::plain("tip")]))
            );
            assert_eq!(run.insertion.as_deref(), Some("Steve"));
        }
    }

    #[test]
    fn a_hover_text_may_be_styled() {
        let parsed = runs(
            r#"{"text":"a","hoverEvent":{"action":"show_text","value":{"text":"t","color":"red"}}}"#,
        );
        let Some(HoverEvent::ShowText(lines)) = &parsed[0].hover else {
            panic!("show_text expected");
        };
        assert_eq!(lines[0].style.color, red());
    }

    #[test]
    fn item_and_entity_hovers_keep_their_nbt_text() {
        let item = runs(r#"{"text":"a","hoverEvent":{"action":"show_item","value":"{id:1}"}}"#);
        assert_eq!(item[0].hover, Some(HoverEvent::ShowItem("{id:1}".into())));
        let entity =
            runs(r#"{"text":"a","hoverEvent":{"action":"show_entity","value":"{id:\"x\"}"}}"#);
        assert!(matches!(entity[0].hover, Some(HoverEvent::ShowEntity(_))));
    }

    #[test]
    fn an_unknown_hover_or_click_action_is_dropped() {
        let parsed = runs(
            r#"{"text":"a","hoverEvent":{"action":"show_cat","value":"x"},
                "clickEvent":{"action":"explode","value":"x"}}"#,
        );
        assert_eq!(parsed[0].hover, None);
        assert_eq!(parsed[0].click, None);
    }
}

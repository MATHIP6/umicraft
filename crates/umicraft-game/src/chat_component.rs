// SPDX-License-Identifier: MIT OR Apache-2.0

//! Chat components: the JSON a 1.8 server sends for chat, titles, the tab
//! list and the sidebar, flattened into styled runs of text (M10, GAM-6).
//!
//! The format follows MCP `IChatComponent`, `ChatStyle` and its JSON
//! serializer (`ChatComponentText`, `ChatComponentTranslation`, the `extra`
//! list, the style fields and their inheritance from the parent) and the
//! legacy `§` colour codes of `FontRenderer.renderStringAtPos`.
//!
//! The parser never fails on server data. A malformed component becomes one
//! run of raw text, the recursion depth and the total text are capped, and a
//! `translate` key the client does not know is shown as the key.

use std::collections::HashMap;

use serde_json::Value;

/// The deepest `extra` / `with` nesting the parser follows. A hostile server
/// could otherwise nest a component until the stack overflows.
pub const MAX_DEPTH: usize = 16;
/// The most characters one parsed component may hold.
pub const MAX_CHARS: usize = 32_767;
/// The most runs one parsed component may hold.
pub const MAX_RUNS: usize = 4096;
/// The section sign that starts a legacy formatting code.
const SECTION: char = '\u{a7}';

/// One of the sixteen chat colours, by its formatting index (`0` black to
/// `f` white).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(u8);

impl Color {
    /// White, the default colour of chat text.
    pub const WHITE: Self = Self(15);

    /// The colour of a legacy code character such as `'c'`.
    #[must_use]
    pub fn from_code(code: char) -> Option<Self> {
        code.to_digit(16)
            .and_then(|digit| u8::try_from(digit).ok())
            .map(Self)
    }

    /// The colour with this JSON name, such as `"dark_red"`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        const NAMES: [&str; 16] = [
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
        NAMES
            .iter()
            .position(|candidate| candidate.eq_ignore_ascii_case(name))
            .map(|index| Self(index as u8))
    }

    /// The formatting index, `0..=15`.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }

    /// The RGB value of the colour (`FontRenderer.colorCode`, without the
    /// darker shadow variants).
    #[must_use]
    pub const fn rgb(self) -> [u8; 3] {
        const TABLE: [[u8; 3]; 16] = [
            [0x00, 0x00, 0x00],
            [0x00, 0x00, 0xAA],
            [0x00, 0xAA, 0x00],
            [0x00, 0xAA, 0xAA],
            [0xAA, 0x00, 0x00],
            [0xAA, 0x00, 0xAA],
            [0xFF, 0xAA, 0x00],
            [0xAA, 0xAA, 0xAA],
            [0x55, 0x55, 0x55],
            [0x55, 0x55, 0xFF],
            [0x55, 0xFF, 0x55],
            [0x55, 0xFF, 0xFF],
            [0xFF, 0x55, 0x55],
            [0xFF, 0x55, 0xFF],
            [0xFF, 0xFF, 0x55],
            [0xFF, 0xFF, 0xFF],
        ];
        TABLE[(self.0 & 15) as usize]
    }
}

/// The resolved style of a run of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    /// The text colour, white when `None`.
    pub color: Option<Color>,
    /// Bold text.
    pub bold: bool,
    /// Italic text.
    pub italic: bool,
    /// Underlined text.
    pub underlined: bool,
    /// Struck-through text.
    pub strikethrough: bool,
    /// Text drawn as random glyphs of the same width.
    pub obfuscated: bool,
}

/// What clicking a run does, as the server described it. The client decides
/// which actions it follows (ADR-0017: never `open_url`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClickEvent {
    /// `open_url`, `run_command`, `suggest_command`, ….
    pub action: String,
    /// The URL or the command.
    pub value: String,
}

/// A piece of text with one style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// The text.
    pub text: String,
    /// The style.
    pub style: Style,
    /// The click action of the component the run came from.
    pub click: Option<ClickEvent>,
    /// The `show_text` hover of the component the run came from.
    pub hover: Option<String>,
}

/// The translation table for `translate` components.
#[derive(Debug, Clone, Default)]
pub struct Translations {
    entries: HashMap<String, String>,
}

impl Translations {
    /// A table with the system messages the vanilla server sends most, so the
    /// chat is readable before the language file is installed.
    #[must_use]
    pub fn builtin() -> Self {
        const ENTRIES: [(&str, &str); 8] = [
            ("multiplayer.player.joined", "%s joined the game"),
            ("multiplayer.player.left", "%s left the game"),
            ("chat.type.text", "<%s> %s"),
            ("chat.type.announcement", "[%s] %s"),
            ("multiplayer.disconnect.generic", "Disconnected"),
            ("multiplayer.disconnect.kicked", "Kicked by an operator: %s"),
            (
                "commands.generic.notFound",
                "Unknown command. Type /help for help.",
            ),
            ("death.attack.player", "%s was slain by %s"),
        ];
        Self {
            entries: ENTRIES
                .iter()
                .map(|(key, text)| ((*key).to_string(), (*text).to_string()))
                .collect(),
        }
    }

    /// Parses the text of a `.lang` file: `key=value` lines, `#` comments.
    #[must_use]
    pub fn from_lang(text: &str) -> Self {
        let entries = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key.trim().to_string(), value.to_string()))
            .collect();
        Self { entries }
    }

    /// Adds the entries of `other`, replacing the ones with the same key.
    pub fn extend(&mut self, other: Self) {
        self.entries.extend(other.entries);
    }

    /// The text for a key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
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
    hover: Option<String>,
}

impl Inherited {
    /// This style with the fields `value` sets on top.
    fn with(&self, value: &Value) -> Self {
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
                .and_then(hover_text)
                .or_else(|| self.hover.clone()),
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
        action: value.get("action")?.as_str()?.to_string(),
        value: value.get("value")?.as_str()?.to_string(),
    })
}

/// The text of a `show_text` hover event. Item and entity hovers are not
/// followed (ADR-0017).
fn hover_text(value: &Value) -> Option<String> {
    if value.get("action")?.as_str()? != "show_text" {
        return None;
    }
    let runs = parse_value(value.get("value")?, &Translations::default());
    Some(plain(&runs))
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
        });
    }
}

/// Parses a chat JSON component into runs, expanding the legacy `§` codes.
///
/// Malformed JSON becomes a single run holding the raw text, so no message
/// is lost. A component with no text yields no runs.
#[must_use]
pub fn parse(json: &str, translations: &Translations) -> Vec<Run> {
    match serde_json::from_str::<Value>(json) {
        Ok(value) => parse_value(&value, translations),
        Err(_) => expand_legacy(vec![Run {
            text: json.chars().take(MAX_CHARS).collect(),
            style: Style::default(),
            click: None,
            hover: None,
        }]),
    }
}

fn parse_value(value: &Value, translations: &Translations) -> Vec<Run> {
    let mut output = Output {
        runs: Vec::new(),
        chars: 0,
    };
    walk(value, &Inherited::default(), translations, 0, &mut output);
    expand_legacy(output.runs)
}

/// The text of the runs, without any style.
#[must_use]
pub fn plain(runs: &[Run]) -> String {
    runs.iter().map(|run| run.text.as_str()).collect()
}

fn walk(
    value: &Value,
    parent: &Inherited,
    translations: &Translations,
    depth: usize,
    output: &mut Output,
) {
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
            walk(first, parent, translations, depth + 1, output);
            let sibling_parent = match first {
                Value::Object(_) => parent.with(first),
                _ => parent.clone(),
            };
            for sibling in items {
                walk(sibling, &sibling_parent, translations, depth + 1, output);
            }
        }
        Value::Object(_) => {
            let style = parent.with(value);
            if let Some(Value::String(text)) = value.get("text") {
                output.push(text, &style);
            }
            if let Some(Value::String(key)) = value.get("translate") {
                let arguments = match value.get("with") {
                    Some(Value::Array(items)) => items.as_slice(),
                    _ => &[],
                };
                translate(key, arguments, &style, translations, depth, output);
            }
            if let Some(Value::Array(extra)) = value.get("extra") {
                for child in extra {
                    walk(child, &style, translations, depth + 1, output);
                }
            }
        }
        Value::Null => {}
    }
}

/// Expands a `translate` component: `%s`, `%1$s` and `%%` in the format.
fn translate(
    key: &str,
    arguments: &[Value],
    style: &Inherited,
    translations: &Translations,
    depth: usize,
    output: &mut Output,
) {
    let format = translations.get(key).unwrap_or(key);
    let mut literal = String::new();
    let mut next_argument = 0;
    let mut chars = format.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '%' {
            literal.push(character);
            continue;
        }
        match chars.peek().copied() {
            Some('%') => {
                chars.next();
                literal.push('%');
            }
            Some('s') => {
                chars.next();
                output.push(&std::mem::take(&mut literal), style);
                if let Some(argument) = arguments.get(next_argument) {
                    walk(argument, style, translations, depth + 1, output);
                }
                next_argument += 1;
            }
            Some(digit) if digit.is_ascii_digit() => {
                // `%n$s`: a numbered argument.
                let mut lookahead = chars.clone();
                let mut number = String::new();
                while let Some(&next) = lookahead.peek() {
                    if next.is_ascii_digit() {
                        number.push(next);
                        lookahead.next();
                    } else {
                        break;
                    }
                }
                if lookahead.next() == Some('$') && lookahead.next() == Some('s') {
                    chars = lookahead;
                    output.push(&std::mem::take(&mut literal), style);
                    let index = number.parse::<usize>().ok().and_then(|n| n.checked_sub(1));
                    if let Some(argument) = index.and_then(|index| arguments.get(index)) {
                        walk(argument, style, translations, depth + 1, output);
                    }
                } else {
                    literal.push('%');
                }
            }
            _ => literal.push('%'),
        }
    }
    output.push(&literal, style);
}

/// Splits runs at the legacy `§` codes and applies them.
///
/// A colour code resets the other styles; `k l m n o` switch one on; `r`
/// resets everything; an unknown code is treated as white, like the
/// reference font renderer.
fn expand_legacy(runs: Vec<Run>) -> Vec<Run> {
    if runs.iter().all(|run| !run.text.contains(SECTION)) {
        return runs;
    }
    let mut expanded = Vec::with_capacity(runs.len());
    for run in runs {
        let mut style = run.style;
        let mut buffer = String::new();
        let mut chars = run.text.chars();
        let flush = |buffer: &mut String, style: Style, expanded: &mut Vec<Run>| {
            if !buffer.is_empty() {
                expanded.push(Run {
                    text: std::mem::take(buffer),
                    style,
                    click: run.click.clone(),
                    hover: run.hover.clone(),
                });
            }
        };
        while let Some(character) = chars.next() {
            if character != SECTION {
                buffer.push(character);
                continue;
            }
            let Some(code) = chars.next() else {
                buffer.push(character);
                break;
            };
            flush(&mut buffer, style, &mut expanded);
            match code.to_ascii_lowercase() {
                'k' => style.obfuscated = true,
                'l' => style.bold = true,
                'm' => style.strikethrough = true,
                'n' => style.underlined = true,
                'o' => style.italic = true,
                'r' => style = Style::default(),
                other => {
                    style = Style {
                        color: Some(Color::from_code(other).unwrap_or(Color::WHITE)),
                        ..Style::default()
                    };
                }
            }
        }
        flush(&mut buffer, style, &mut expanded);
    }
    expanded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runs(json: &str) -> Vec<Run> {
        parse(json, &Translations::builtin())
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
        let mut table = Translations::default();
        table.extend(Translations::from_lang("k=%2$s then %1$s, 100%% sure\n"));
        let parsed = parse(r#"{"translate":"k","with":["a","b"]}"#, &table);
        assert_eq!(plain(&parsed), "b then a, 100% sure");
    }

    #[test]
    fn an_unknown_key_is_shown_as_the_key() {
        assert_eq!(
            plain(&runs(r#"{"translate":"some.unknown.key"}"#)),
            "some.unknown.key"
        );
    }

    #[test]
    fn a_lang_file_is_parsed_into_entries() {
        let table = Translations::from_lang("# comment\n\na.b=Hello %s\nc=d=e\n");
        assert_eq!(table.get("a.b"), Some("Hello %s"));
        assert_eq!(table.get("c"), Some("d=e"));
        assert_eq!(table.get("# comment"), None);
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
        let parsed = parse("\u{a7}chello", &Translations::default());
        assert_eq!(parsed[0].style.color, red());
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
    fn click_and_hover_events_are_kept_and_inherited() {
        let parsed = runs(
            r#"{"text":"a","clickEvent":{"action":"suggest_command","value":"/home"},
                "hoverEvent":{"action":"show_text","value":{"text":"tip"}},
                "extra":[{"text":"b"}]}"#,
        );
        for run in &parsed {
            assert_eq!(run.click.as_ref().unwrap().action, "suggest_command");
            assert_eq!(run.click.as_ref().unwrap().value, "/home");
            assert_eq!(run.hover.as_deref(), Some("tip"));
        }
    }

    #[test]
    fn a_hover_that_is_not_show_text_is_dropped() {
        let parsed = runs(r#"{"text":"a","hoverEvent":{"action":"show_item","value":"{id:1}"}}"#);
        assert_eq!(parsed[0].hover, None);
    }

    #[test]
    fn colours_map_to_the_font_table() {
        assert_eq!(Color::from_name("red").unwrap().rgb(), [0xFF, 0x55, 0x55]);
        assert_eq!(Color::from_code('0').unwrap().rgb(), [0, 0, 0]);
        assert_eq!(Color::from_name("RED"), Color::from_name("red"));
        assert_eq!(Color::from_name("purple"), None);
        assert_eq!(Color::from_code('g'), None);
    }
}

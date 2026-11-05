// SPDX-License-Identifier: MIT OR Apache-2.0

//! The language system: `.lang` parsing, the format expansion and the
//! [`Lang`] handle every UI text goes through (ADR-0039).
//!
//! No Minecraft UI text is hard-coded: code holds translation *keys* and asks
//! [`Lang::tr`] / [`Lang::tr_args`] for the text. The texts come from the
//! `.lang` files of the first-run download (`umicraft-assets`, asset index
//! entry `minecraft/lang/<code>.lang`); they are never stored in the
//! repository (spec §3). [`LangTable::builtin`] is a tiny offline / test
//! fallback only.
//!
//! Sources: MCP `Locale.loadLocaleData` (`key=value` lines, `#` comments,
//! split at the first `=`, `%d` / `%1$.2f` rewritten to `%s`),
//! `I18n.format` / `Locale.formatMessage` (unknown key gives the key) and
//! `ChatComponentTranslation.initializeFromFormat` (the placeholder pattern
//! `%(?:(\d+)\$)?([A-Za-z%]|$)`, `%%` is a percent sign, only `s` is a
//! supported conversion). The vanilla fallback to `en_US` is
//! `Locale.loadLocaleDataFiles` (the `en_US` file is always loaded first).

use std::collections::HashMap;

use thiserror::Error;

/// A translation format the chat cannot expand.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FormatError {
    /// The format holds a conversion other than `%s` and `%%`
    /// (`ChatComponentTranslationFormatException`).
    #[error("unsupported format {0:?}")]
    Unsupported(String),
}

/// One piece of an expanded translation format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    /// Literal text, with `%%` already turned into `%`.
    Text(String),
    /// The zero-based index of the argument to insert.
    Argument(usize),
}

/// The translation table for `translate` components.
#[derive(Debug, Clone, Default)]
pub struct LangTable {
    entries: HashMap<String, String>,
}

impl LangTable {
    /// The offline / test fallback: the few texts the client needs before
    /// the language files are installed (chat system messages, the pause and
    /// death screens). It is **not** a translation source; the downloaded
    /// `.lang` files override every entry (ADR-0039).
    #[must_use]
    pub fn builtin() -> Self {
        Self {
            entries: BUILTIN
                .iter()
                .map(|(key, text)| ((*key).to_string(), (*text).to_string()))
                .collect(),
        }
    }

    /// Parses the text of a `.lang` file (`Locale.loadLocaleData`).
    ///
    /// Blank lines and lines starting with `#` are skipped, a line is split
    /// at its first `=`, and numeric conversions (`%d`, `%1$.2f`) become `%s`
    /// because the chat only substitutes strings.
    #[must_use]
    pub fn from_lang(text: &str) -> Self {
        let entries = text
            .lines()
            .map(|line| line.trim_end_matches('\r'))
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key.to_string(), numeric_to_string(value)))
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

    /// The number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table has no entry.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Rewrites `%d`, `%f` and their numbered / width forms to `%s`
/// (`Locale.pattern = %(\d+\$)?[\d\.]*[df]`).
fn numeric_to_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        let digits = tail.bytes().take_while(u8::is_ascii_digit).count();
        let (number, after) = match tail[digits..].strip_prefix('$') {
            Some(after) if digits > 0 => (&tail[..=digits], after),
            _ => ("", tail),
        };
        let width = after
            .bytes()
            .take_while(|byte| byte.is_ascii_digit() || *byte == b'.')
            .count();
        match after.as_bytes().get(width) {
            Some(b'd' | b'f') => {
                out.push('%');
                out.push_str(number);
                out.push('s');
                rest = &after[width + 1..];
            }
            _ => {
                out.push('%');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Expands a translation format into text and argument pieces.
///
/// Placeholders follow `%(?:(\d+)\$)?([A-Za-z%]|$)`: `%s` takes the next
/// argument, `%2$s` the second one, `%%` is a literal percent sign. Any other
/// conversion is an error.
///
/// # Errors
///
/// Returns [`FormatError::Unsupported`] for a conversion other than `s`.
pub fn expand_format(format: &str) -> Result<Vec<Piece>, FormatError> {
    let chars: Vec<char> = format.chars().collect();
    let mut pieces = Vec::new();
    let mut literal = String::new();
    let mut next_argument = 0;
    let mut at = 0;
    while at < chars.len() {
        if chars[at] != '%' {
            literal.push(chars[at]);
            at += 1;
            continue;
        }
        let digits = chars[at + 1..]
            .iter()
            .take_while(|character| character.is_ascii_digit())
            .count();
        let numbered = digits > 0 && chars.get(at + 1 + digits) == Some(&'$');
        let conversion_at = at + 1 + if numbered { digits + 1 } else { 0 };
        match chars.get(conversion_at).copied() {
            Some('%') if !numbered => literal.push('%'),
            Some('s') => {
                if !literal.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut literal)));
                }
                let index = if numbered {
                    let number: String = chars[at + 1..at + 1 + digits].iter().collect();
                    number.parse::<usize>().ok().and_then(|n| n.checked_sub(1))
                } else {
                    next_argument += 1;
                    Some(next_argument - 1)
                };
                let index = index.ok_or_else(|| FormatError::Unsupported("%0$s".to_string()))?;
                pieces.push(Piece::Argument(index));
            }
            Some(other) if other.is_ascii_alphabetic() || other == '%' => {
                return Err(FormatError::Unsupported(format!("%{other}")));
            }
            // A lone `%` (end of text, or before something the pattern does
            // not match) stays literal.
            _ => {
                literal.push('%');
                at += 1;
                continue;
            }
        }
        at = conversion_at + 1;
    }
    if !literal.is_empty() {
        pieces.push(Piece::Text(literal));
    }
    Ok(pieces)
}

/// The texts a UI language handle serves.
///
/// One merged table, built in the order of the fallback chain (lowest
/// priority first): the [`LangTable::builtin`] offline fallback, the `en_US`
/// file, then the selected language. A key found nowhere gives the key itself
/// (`I18n.format` behaviour), which makes a missing translation visible
/// instead of blank.
#[derive(Debug, Clone)]
pub struct Lang {
    code: String,
    table: LangTable,
}

impl Lang {
    /// The code of the language the client starts with and falls back to.
    pub const DEFAULT_CODE: &'static str = "en_US";

    /// A handle that only knows the [`LangTable::builtin`] fallback. Used by
    /// tests and before the language files are downloaded.
    #[must_use]
    pub fn builtin() -> Self {
        Self {
            code: Self::DEFAULT_CODE.to_string(),
            table: LangTable::builtin(),
        }
    }

    /// Builds the handle for language `code` from the text of its `.lang`
    /// file and, when `code` is not `en_US`, the text of the `en_US` file.
    /// Either text may be absent (not downloaded).
    #[must_use]
    pub fn from_texts(code: &str, english: Option<&str>, selected: Option<&str>) -> Self {
        let mut table = LangTable::builtin();
        for text in [english, selected].into_iter().flatten() {
            table.extend(LangTable::from_lang(text));
        }
        Self {
            code: code.to_string(),
            table,
        }
    }

    /// The code of the selected language (`en_US`, `de_DE`, ...).
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// The merged table, for the chat's `translate` components.
    #[must_use]
    pub fn table(&self) -> &LangTable {
        &self.table
    }

    /// The text of `key` when some table in the chain has it.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.table.get(key)
    }

    /// The text of `key`, or the key itself when no table has it.
    #[must_use]
    pub fn tr<'a>(&'a self, key: &'a str) -> &'a str {
        self.get(key).unwrap_or(key)
    }

    /// The text of `key` with its placeholders filled from `args`
    /// (`%s`, `%1$s`, `%%`; `%d` was rewritten to `%s` at load time).
    ///
    /// A missing argument leaves its placeholder empty; a format that cannot
    /// be expanded is returned as written, as `I18n.format` does when the
    /// `String.format` call throws.
    ///
    /// ```
    /// use umicraft_core::lang::Lang;
    /// let lang = Lang::from_texts("en_US", Some("k=%2$s then %1$s\n"), None);
    /// assert_eq!(lang.tr_args("k", &["a", "b"]), "b then a");
    /// ```
    #[must_use]
    pub fn tr_args(&self, key: &str, args: &[&str]) -> String {
        let format = self.tr(key);
        let Ok(pieces) = expand_format(format) else {
            return format.to_string();
        };
        pieces
            .iter()
            .map(|piece| match piece {
                Piece::Text(text) => text.as_str(),
                Piece::Argument(index) => args.get(*index).copied().unwrap_or(""),
            })
            .collect()
    }
}

impl Default for Lang {
    fn default() -> Self {
        Self::builtin()
    }
}

/// The offline fallback texts (`en_US` wording), see [`LangTable::builtin`].
///
/// Only keys the client needs to be usable without downloaded assets belong
/// here (the pause and death screens, the chat system messages, and the texts
/// vanilla hard-codes); every other key shows as itself until the language
/// files are installed. The guard test `no_hardcoded_ui_text` keeps UI code from carrying
/// its own copies.
const BUILTIN: &[(&str, &str)] = &[
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
    ("menu.game", "Game menu"),
    ("menu.returnToGame", "Back to Game"),
    ("menu.options", "Options..."),
    ("menu.shareToLan", "Open to LAN"),
    ("menu.disconnect", "Disconnect"),
    ("menu.returnToMenu", "Save and Quit to Title"),
    // The title screen is the first screen of the client, so its texts and
    // those of the server list work before the language files are installed.
    ("menu.singleplayer", "Singleplayer"),
    ("menu.multiplayer", "Multiplayer"),
    ("menu.quit", "Quit Game"),
    ("multiplayer.title", "Play Multiplayer"),
    ("selectServer.select", "Join Server"),
    ("selectServer.direct", "Direct Connect"),
    ("selectServer.add", "Add server"),
    ("selectServer.defaultName", "Minecraft Server"),
    ("selectServer.edit", "Edit"),
    ("selectServer.delete", "Delete"),
    ("selectServer.refresh", "Refresh"),
    (
        "selectServer.deleteQuestion",
        "Are you sure you want to remove this server?",
    ),
    (
        "selectServer.deleteWarning",
        "will be lost forever! (A long time!)",
    ),
    ("selectServer.deleteButton", "Delete"),
    ("addServer.title", "Edit Server Info"),
    ("addServer.enterName", "Server Name"),
    ("addServer.enterIp", "Server Address"),
    ("addServer.add", "Done"),
    ("addServer.resourcePack", "Server Resource Packs"),
    ("addServer.resourcePack.enabled", "Enabled"),
    ("addServer.resourcePack.disabled", "Disabled"),
    ("addServer.resourcePack.prompt", "Prompt"),
    ("connect.connecting", "Connecting to the server..."),
    ("connect.authorizing", "Logging in..."),
    ("disconnect.lost", "Connection Lost"),
    ("gui.toMenu", "Back to server list"),
    ("gui.achievements", "Achievements"),
    ("gui.stats", "Statistics"),
    ("gui.done", "Done"),
    ("gui.cancel", "Cancel"),
    ("key.mouseButton", "Button %s"),
    ("deathScreen.title", "You died!"),
    ("deathScreen.score", "Score"),
    ("deathScreen.respawn", "Respawn"),
    ("deathScreen.titleScreen", "Title screen"),
    // Texts vanilla hard-codes in its classes, so no `.lang` file has them:
    // `GuiOptions` ("Super Secret Settings..."), `GuiScreen.handleComponentHover`
    // ("Invalid Item!"), `ItemStack.getTooltip` ("Durability: x / y") and the attribute line (`+%d %s`, whose key does
    // exist in the files and only guards the offline case).
    ("umicraft.options.shaders", "Super Secret Settings..."),
    // `ResourcePackListEntryDefault` hard-codes its name and description.
    ("umicraft.resourcePack.default", "Default"),
    (
        "umicraft.resourcePack.defaultDescription",
        "The default look of Minecraft",
    ),
    (
        "umicraft.resourcePack.missingMetadata",
        "Missing pack.mcmeta :(",
    ),
    ("umicraft.hover.invalidItem", "Invalid Item!"),
    ("umicraft.hover.invalidEntity", "Invalid Entity!"),
    ("item.durability", "Durability: %s / %s"),
    // Menu texts vanilla hard-codes (`ServerListEntryNormal`) or that are
    // Umicraft's own (title screen credit); the first group has no key in the files.
    ("umicraft.server.pinging", "Pinging..."),
    ("umicraft.server.noConnection", "(no connection)"),
    ("umicraft.server.clientOutdated", "Client out of date!"),
    ("umicraft.server.serverOutdated", "Server out of date!"),
    ("umicraft.server.andMore", "... and %s more ..."),
    ("umicraft.server.unreachable", "Can't connect to server."),
    (
        "umicraft.title.credit",
        "Not affiliated with Mojang or Microsoft",
    ),
    ("attribute.modifier.plus.0", "+%s %s"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_title_and_server_list_texts_work_offline() {
        let lang = Lang::builtin();
        assert_eq!(lang.tr("menu.multiplayer"), "Multiplayer");
        assert_eq!(lang.tr("selectServer.direct"), "Direct Connect");
        assert_eq!(lang.tr("disconnect.lost"), "Connection Lost");
        assert_eq!(lang.tr("gui.toMenu"), "Back to server list");
    }

    fn text(value: &str) -> Piece {
        Piece::Text(value.to_string())
    }

    #[test]
    fn a_lang_file_is_parsed_into_entries() {
        let table = LangTable::from_lang("# comment\n\na.b=Hello %s\nc=d=e\r\n #x=y\n");
        assert_eq!(table.get("a.b"), Some("Hello %s"));
        assert_eq!(table.get("c"), Some("d=e"));
        assert_eq!(table.get("# comment"), None);
        assert_eq!(table.len(), 3, "a space before # makes it a key");
    }

    #[test]
    fn numeric_conversions_become_string_conversions() {
        let table = LangTable::from_lang("k=%d of %2$d, %.2f and %1$05.1f, 100%% and 5%\n");
        assert_eq!(
            table.get("k"),
            Some("%s of %2$s, %s and %1$s, 100%% and 5%")
        );
    }

    #[test]
    fn later_tables_override_earlier_entries() {
        let mut table = LangTable::builtin();
        table.extend(LangTable::from_lang("chat.type.text=%s says %s\n"));
        assert_eq!(table.get("chat.type.text"), Some("%s says %s"));
        assert!(table.get("multiplayer.player.joined").is_some());
    }

    #[test]
    fn sequential_and_numbered_placeholders_expand() {
        assert_eq!(
            expand_format("a %s b %s"),
            Ok(vec![
                text("a "),
                Piece::Argument(0),
                text(" b "),
                Piece::Argument(1)
            ])
        );
        assert_eq!(
            expand_format("%2$s then %1$s"),
            Ok(vec![Piece::Argument(1), text(" then "), Piece::Argument(0)])
        );
    }

    #[test]
    fn a_doubled_percent_is_one_percent_sign() {
        assert_eq!(expand_format("100%% sure"), Ok(vec![text("100% sure")]));
    }

    #[test]
    fn a_trailing_or_lone_percent_stays_literal() {
        assert_eq!(expand_format("50%"), Ok(vec![text("50%")]));
        assert_eq!(expand_format("5% off"), Ok(vec![text("5% off")]));
    }

    #[test]
    fn an_unsupported_conversion_is_an_error() {
        assert_eq!(
            expand_format("%x"),
            Err(FormatError::Unsupported("%x".to_string()))
        );
    }

    #[test]
    fn a_key_found_nowhere_is_returned_as_is() {
        let lang = Lang::builtin();
        assert_eq!(lang.tr("no.such.key"), "no.such.key");
        assert_eq!(lang.tr_args("no.such.key", &["x"]), "no.such.key");
    }

    #[test]
    fn the_selected_language_beats_english_which_beats_the_builtin() {
        let english = "a=en a\nb=en b\nchat.type.text=en chat\n";
        let german = "a=de a\n";
        let lang = Lang::from_texts("de_DE", Some(english), Some(german));
        assert_eq!(lang.code(), "de_DE");
        assert_eq!(lang.tr("a"), "de a");
        assert_eq!(lang.tr("b"), "en b");
        assert_eq!(lang.tr("chat.type.text"), "en chat");
        assert_eq!(lang.tr("multiplayer.player.joined"), "%s joined the game");
    }

    #[test]
    fn arguments_fill_sequential_numbered_and_numeric_placeholders() {
        let lang = Lang::from_texts(
            "en_US",
            Some("seq=%s and %s\nnum=%2$s before %1$s\ndec=%d / %d\npct=100%%\n"),
            None,
        );
        assert_eq!(lang.tr_args("seq", &["x", "y"]), "x and y");
        assert_eq!(lang.tr_args("num", &["x", "y"]), "y before x");
        assert_eq!(lang.tr_args("dec", &["1", "2"]), "1 / 2");
        assert_eq!(lang.tr_args("pct", &[]), "100%");
        assert_eq!(lang.tr_args("seq", &["x"]), "x and ");
    }

    #[test]
    fn an_unexpandable_format_is_returned_as_written() {
        let lang = Lang::from_texts("en_US", Some("bad=value %x\n"), None);
        assert_eq!(lang.tr_args("bad", &["a"]), "value %x");
    }

    #[test]
    fn a_missing_language_file_leaves_the_fallbacks() {
        let lang = Lang::from_texts("fr_FR", None, None);
        assert_eq!(lang.tr("chat.type.text"), "<%s> %s");
    }
}

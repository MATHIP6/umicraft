// SPDX-License-Identifier: MIT OR Apache-2.0

//! Click and hover events of chat components, and what the client does with
//! them.
//!
//! Sources: MCP `ClickEvent.Action` / `HoverEvent.Action` (the action names),
//! `GuiScreen.handleComponentClick` (what a click does) and
//! `GuiScreen.handleComponentHover` (what a hover shows).
//!
//! Policy (ADR-0017): a click acts only when the player makes it. `open_url`,
//! `open_file` and `twitch_user_info` are never followed, `change_page` has no
//! book to turn, so only `run_command` and `suggest_command` have an effect.

use super::commands;
use super::component::Run;

/// What a click on a component asks for (`ClickEvent.Action`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickAction {
    /// `open_url`: never followed.
    OpenUrl,
    /// `open_file`: never followed.
    OpenFile,
    /// `run_command`: the value is sent as a chat message.
    RunCommand,
    /// `twitch_user_info`: Twitch is gone from 1.8 clients we support.
    TwitchUserInfo,
    /// `suggest_command`: the value replaces the input line.
    SuggestCommand,
    /// `change_page`: only meaningful in a book.
    ChangePage,
}

impl ClickAction {
    /// The action with this JSON name; `None` for an unknown name, in which
    /// case the reference deserializer drops the whole event.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name.to_ascii_lowercase().as_str() {
            "open_url" => Self::OpenUrl,
            "open_file" => Self::OpenFile,
            "run_command" => Self::RunCommand,
            "twitch_user_info" => Self::TwitchUserInfo,
            "suggest_command" => Self::SuggestCommand,
            "change_page" => Self::ChangePage,
            _ => return None,
        })
    }
}

/// A click event as the server described it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClickEvent {
    /// What the click asks for.
    pub action: ClickAction,
    /// The URL, the command or the page.
    pub value: String,
}

/// A hover event as the server described it (`HoverEvent.Action`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoverEvent {
    /// `show_text`: the runs of the text component to show.
    ShowText(Vec<Run>),
    /// `show_item`: the item as JSON-flavoured NBT text.
    ShowItem(String),
    /// `show_entity`: the entity as JSON-flavoured NBT text.
    ShowEntity(String),
    /// `show_achievement`: the statistic id.
    ShowAchievement(String),
}

/// What the client does after the player clicked a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickOutcome {
    /// Send this text as a chat message, without adding it to the history
    /// (`sendChatMessage(value, false)`).
    Send(String),
    /// Replace the input line with this text (`setText(value, true)`).
    Suggest(String),
    /// Append this text to the input line (`setText(insertion, false)`).
    Insert(String),
    /// The click is handled and has no effect.
    Nothing,
}

/// The effect of a player's left click on a run with this `click` event and
/// `insertion` text (`GuiScreen.handleComponentClick`).
///
/// With Shift held the insertion text is appended to the input line;
/// otherwise the click event runs.
#[must_use]
pub fn on_click(click: Option<&ClickEvent>, insertion: Option<&str>, shift: bool) -> ClickOutcome {
    if shift {
        return insertion.map_or(ClickOutcome::Nothing, |text| {
            ClickOutcome::Insert(text.to_string())
        });
    }
    let Some(click) = click else {
        return ClickOutcome::Nothing;
    };
    match click.action {
        ClickAction::RunCommand => {
            commands::outgoing(&click.value).map_or(ClickOutcome::Nothing, ClickOutcome::Send)
        }
        ClickAction::SuggestCommand => ClickOutcome::Suggest(click.value.clone()),
        ClickAction::OpenUrl
        | ClickAction::OpenFile
        | ClickAction::TwitchUserInfo
        | ClickAction::ChangePage => ClickOutcome::Nothing,
    }
}

/// The item a `show_item` hover names: the registry id, count and damage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverItem {
    /// The numeric item id.
    pub id: i32,
    /// The damage value, 0 when absent.
    pub damage: i16,
}

/// Reads `id` and `Damage` from the NBT text of a `show_item` event
/// (`JsonToNBT.getTagFromJson` then `ItemStack.loadItemStackFromNBT`).
///
/// Only the top-level compound is read, which is all the item tooltip needs.
/// `None` when the text is not a compound or names no known item; the caller
/// then shows "Invalid Item!" like the reference does.
#[must_use]
pub fn parse_hover_item(snbt: &str) -> Option<HoverItem> {
    let id = nbt_field(snbt, "id")?;
    let id = match id.trim_matches(['"', '\'']).parse::<i32>() {
        Ok(number) => number,
        Err(_) => {
            let name = id.trim_matches(['"', '\'']);
            let name = name.strip_prefix("minecraft:").unwrap_or(name);
            i32::from(
                crate::items::all()
                    .iter()
                    .find(|item| item.name == name)?
                    .id,
            )
        }
    };
    crate::items::lookup(id)?;
    let damage = nbt_field(snbt, "Damage")
        .and_then(|text| {
            text.trim_end_matches(['s', 'S', 'b', 'B', 'l', 'L'])
                .parse()
                .ok()
        })
        .unwrap_or(0);
    Some(HoverItem { id, damage })
}

/// The lines `show_entity` shows when advanced tooltips are on: the name,
/// the type and the id (`GuiScreen.handleComponentHover`). `None` when the
/// text is not a compound (the reference shows "Invalid Entity!").
///
/// The numeric entity id the reference appends to the type line
/// (`EntityList.getIDFromString`) is not available, so it is left out.
#[must_use]
pub fn hover_entity_lines(snbt: &str) -> Option<Vec<String>> {
    let text = |key| nbt_field(snbt, key).map(|value| value.trim_matches(['"', '\'']).to_string());
    nbt_field(snbt, "id")?;
    let mut lines = vec![text("name").unwrap_or_default()];
    if let Some(kind) = text("type") {
        lines.push(format!("Type: {kind}"));
    }
    lines.push(text("id").unwrap_or_default());
    Some(lines)
}

/// The raw text of a top-level field of NBT text such as `{id:"x",Damage:3s}`.
fn nbt_field<'a>(snbt: &'a str, key: &str) -> Option<&'a str> {
    let body = snbt.trim().strip_prefix('{')?.strip_suffix('}')?;
    let mut depth = 0_u32;
    let mut quote: Option<char> = None;
    let mut start = 0;
    let mut fields = Vec::new();
    for (index, character) in body.char_indices() {
        match (quote, character) {
            (Some(open), c) if c == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(character),
            (None, '{' | '[') => depth += 1,
            (None, '}' | ']') => depth = depth.saturating_sub(1),
            (None, ',') if depth == 0 => {
                fields.push(&body[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    fields.push(&body[start..]);
    fields.into_iter().find_map(|field| {
        let (name, value) = field.split_once(':')?;
        (name.trim().trim_matches(['"', '\'']) == key).then_some(value.trim())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click(action: ClickAction, value: &str) -> ClickEvent {
        ClickEvent {
            action,
            value: value.to_string(),
        }
    }

    #[test]
    fn run_command_sends_and_suggest_command_fills_the_line() {
        let command = click(ClickAction::RunCommand, "/home");
        assert_eq!(
            on_click(Some(&command), None, false),
            ClickOutcome::Send("/home".into())
        );
        let suggest = click(ClickAction::SuggestCommand, "/msg ");
        assert_eq!(
            on_click(Some(&suggest), None, false),
            ClickOutcome::Suggest("/msg ".into())
        );
    }

    #[test]
    fn links_files_twitch_and_pages_are_never_followed() {
        for action in [
            ClickAction::OpenUrl,
            ClickAction::OpenFile,
            ClickAction::TwitchUserInfo,
            ClickAction::ChangePage,
        ] {
            let link = click(action, "https://example.org");
            assert_eq!(on_click(Some(&link), None, false), ClickOutcome::Nothing);
        }
    }

    #[test]
    fn a_command_over_the_length_limit_is_not_sent() {
        let long = "a".repeat(101);
        let command = click(ClickAction::RunCommand, &long);
        assert_eq!(on_click(Some(&command), None, false), ClickOutcome::Nothing);
    }

    #[test]
    fn shift_click_inserts_the_insertion_text_and_skips_the_click_event() {
        let command = click(ClickAction::RunCommand, "/home");
        assert_eq!(
            on_click(Some(&command), Some("Steve"), true),
            ClickOutcome::Insert("Steve".into())
        );
        assert_eq!(on_click(Some(&command), None, true), ClickOutcome::Nothing);
    }

    #[test]
    fn a_run_without_events_does_nothing() {
        assert_eq!(on_click(None, None, false), ClickOutcome::Nothing);
    }

    #[test]
    fn an_unknown_action_name_is_dropped() {
        assert_eq!(
            ClickAction::from_name("RUN_COMMAND"),
            Some(ClickAction::RunCommand)
        );
        assert_eq!(ClickAction::from_name("explode"), None);
    }

    #[test]
    fn a_hover_entity_lists_name_type_and_id() {
        let lines = hover_entity_lines(r#"{id:"abc-1",type:"Pig",name:"Wilbur"}"#);
        assert_eq!(
            lines,
            Some(vec!["Wilbur".into(), "Type: Pig".into(), "abc-1".into()])
        );
        assert_eq!(hover_entity_lines("nope"), None);
    }

    #[test]
    fn a_hover_item_is_read_from_its_nbt_text() {
        let item = parse_hover_item(
            r#"{id:"minecraft:diamond_sword",Count:1b,Damage:5s,tag:{a:1,b:"x,y"}}"#,
        );
        assert_eq!(item, Some(HoverItem { id: 276, damage: 5 }));
        assert_eq!(parse_hover_item("{id:1}").map(|item| item.id), Some(1));
        assert_eq!(parse_hover_item(r#"{id:"minecraft:nope"}"#), None);
        assert_eq!(parse_hover_item("not nbt"), None);
    }
}

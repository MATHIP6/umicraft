// SPDX-License-Identifier: MIT OR Apache-2.0

//! The client side of the chat: the conversion of parsed components to HUD views
//! and the text of a hover tooltip.
//!
//! The server sends the chat as a JSON component (MCP `IChatComponent`); the
//! parsing, styles, wrapping and click rules live in
//! [`umicraft_game::chat`]. The language comes from [`crate::language`].

use std::sync::Arc;
use std::time::Instant;

use umicraft_core::lang::Lang;
use umicraft_game::chat::events::{hover_entity_lines, parse_hover_item};
use umicraft_game::chat::{self, Color, HoverEvent, Run};
use umicraft_game::inventory::tooltip::tooltip_advanced;
use umicraft_protocol::packets::play::ItemStack;
use umicraft_render::{ChatLineView, ChatRunView};
use umicraft_ui::widgets::tooltip::TooltipRow;

use crate::tooltip_draw;

/// The colour of the "Invalid Item!" line (`EnumChatFormatting.RED`).
const RED: u32 = 0x00FF_5555;
/// The colour of plain tooltip text.
const WHITE: u32 = 0x00FF_FFFF;

/// The language the chat components are translated in: the one the client
/// installed at startup (ADR-0039).
pub(crate) fn lang() -> Arc<Lang> {
    crate::language::current()
}

/// The plain text of a chat JSON component, empty when the component is.
///
/// The tab-list header, the title and the action bar are legitimately empty
/// (`{"text":""}` clears them). A malformed component is returned unchanged.
#[must_use]
pub fn flatten(json: &str) -> String {
    chat::plain(&chat::parse(json, lang().table()))
}

/// The HUD view of parsed runs, stamped with the arrival time.
pub(crate) fn line_from_runs(runs: &[Run], received: Instant) -> ChatLineView {
    ChatLineView {
        runs: run_views(runs),
        received,
    }
}

/// The HUD runs of parsed chat runs; text without a colour is white.
pub(crate) fn run_views(runs: &[Run]) -> Vec<ChatRunView> {
    runs.iter().map(run_view).collect()
}

fn run_view(run: &Run) -> ChatRunView {
    ChatRunView {
        text: run.text.clone(),
        color: run.style.color.map_or(Color::WHITE.rgb(), Color::rgb),
        bold: run.style.bold,
        italic: run.style.italic,
        underlined: run.style.underlined,
        strikethrough: run.style.strikethrough,
        obfuscated: run.style.obfuscated,
        click: run.click.clone(),
        hover: run.hover.clone(),
        insertion: run.insertion.clone(),
    }
}

/// The rows of the tooltip a hover event shows
/// (`GuiScreen.handleComponentHover`).
///
/// `show_text` is one row per line of the text; `show_item` is the item
/// tooltip, or a red `umicraft.hover.invalidItem`; `show_entity` shows only with advanced
/// tooltips on. `show_achievement` needs the statistics list, which the
/// client does not have, so it shows nothing.
pub(crate) fn hover_rows(hover: &HoverEvent, advanced: bool) -> Vec<TooltipRow> {
    match hover {
        HoverEvent::ShowText(runs) => text_rows(runs),
        HoverEvent::ShowItem(snbt) => item_rows(snbt, advanced),
        HoverEvent::ShowEntity(snbt) if advanced => hover_entity_lines(snbt).map_or_else(
            || vec![row(lang().tr("umicraft.hover.invalidEntity"), RED)],
            |lines| lines.into_iter().map(|line| row(&line, WHITE)).collect(),
        ),
        HoverEvent::ShowEntity(_) | HoverEvent::ShowAchievement(_) => Vec::new(),
    }
}

fn row(text: &str, rgb: u32) -> TooltipRow {
    TooltipRow {
        text: text.to_string(),
        rgb,
    }
}

/// One row per line of the text; a row takes the colour of its first run.
fn text_rows(runs: &[Run]) -> Vec<TooltipRow> {
    let mut rows = vec![TooltipRow {
        text: String::new(),
        rgb: WHITE,
    }];
    for run in runs {
        for (index, piece) in run.text.split('\n').enumerate() {
            if index > 0 {
                rows.push(row("", WHITE));
            }
            let Some(current) = rows.last_mut() else {
                continue;
            };
            if current.text.is_empty() && !piece.is_empty() {
                let [r, g, b] = run.style.color.map_or(Color::WHITE.rgb(), Color::rgb);
                current.rgb = u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b);
            }
            current.text.push_str(piece);
        }
    }
    rows
}

fn item_rows(snbt: &str, advanced: bool) -> Vec<TooltipRow> {
    let Some(item) = parse_hover_item(snbt).and_then(|item| {
        Some(ItemStack {
            item_id: i16::try_from(item.id).ok()?,
            stack_size: 1,
            damage: item.damage,
            nbt: None,
        })
    }) else {
        return vec![row(lang().tr("umicraft.hover.invalidItem"), RED)];
    };
    tooltip_advanced(&item, advanced)
        .iter()
        .map(|line| row(&tooltip_draw::resolve(&line.text), line.color.rgb()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::chat::LangTable;

    #[test]
    fn an_empty_component_stays_empty_for_the_overlays() {
        assert_eq!(flatten(r#"{"text":""}"#), "");
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

    #[test]
    fn the_view_keeps_the_style_flags_and_the_events() {
        let runs = chat::parse(
            r#"{"text":"x","italic":true,"insertion":"i",
                "clickEvent":{"action":"run_command","value":"/a"}}"#,
            &LangTable::builtin(),
        );
        let line = line_from_runs(&runs, Instant::now());
        let view = &line.runs[0];
        assert!(view.italic);
        assert_eq!(view.insertion.as_deref(), Some("i"));
        assert!(view.click.is_some());
    }

    #[test]
    fn show_text_is_one_row_per_line_in_the_colour_of_the_line() {
        let runs = chat::parse(r#"{"text":"a\nb","color":"red"}"#, &LangTable::builtin());
        let rows = text_rows(&runs);
        let texts: Vec<_> = rows.iter().map(|row| row.text.as_str()).collect();
        assert_eq!(texts, ["a", "b"]);
        assert_eq!(rows[0].rgb, 0xFF5555);
    }

    #[test]
    fn show_item_is_the_item_tooltip_and_a_broken_item_is_invalid() {
        let rows = item_rows(r#"{id:"minecraft:diamond_sword",Damage:0s}"#, false);
        // No language file in the test: the legacy key reads as its words.
        assert_eq!(rows[0].text, "Sword Diamond");
        assert_eq!(item_rows("garbage", false), vec![row("Invalid Item!", RED)]);
    }

    #[test]
    fn show_entity_needs_advanced_tooltips() {
        let hover = HoverEvent::ShowEntity(r#"{id:"x",name:"Pig"}"#.to_string());
        assert!(hover_rows(&hover, false).is_empty());
        assert_eq!(hover_rows(&hover, true)[0].text, "Pig");
    }
}

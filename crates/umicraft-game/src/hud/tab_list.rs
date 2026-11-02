// SPDX-License-Identifier: MIT OR Apache-2.0

//! The tab list as styled rows.
//!
//! Sources: MCP `GuiPlayerTabOverlay` (`getPlayerName`, the `PlayerComparator`
//! order, the list-slot objective) and `NetHandlerPlayClient.
//! handlePlayerListHeaderFooter` (an empty header or footer is absent). The
//! geometry (columns, widths, ping bars, hearts) needs the font and the
//! screen width and is computed by the renderer.

use crate::chat_component::{parse, parse_legacy, plain, Run, Translations};
use crate::player_list::{PlayerEntry, PlayerList, Uuid, SPECTATOR};
use crate::scoreboard::{Scoreboard, SLOT_LIST};

/// How the list-slot objective draws its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueStyle {
    /// A yellow number right-aligned in the column.
    Integer,
    /// Hearts, or `"x.yhp"` when the column is too narrow.
    Hearts,
}

/// One player of the tab list.
#[derive(Debug, Clone, PartialEq)]
pub struct TabRow {
    /// The player UUID.
    pub uuid: Uuid,
    /// The formatted name: the server display name, else the team prefix,
    /// the player name and the team suffix.
    pub name: Vec<Run>,
    /// The latency in milliseconds; negative means unknown.
    pub ping: i32,
    /// Spectators are drawn in italics at 56 % opacity and show no score.
    pub spectator: bool,
    /// The score of the list objective, when one is displayed.
    pub score: Option<i32>,
    /// The base64 `textures` property, which carries the skin of the head.
    pub textures: Option<String>,
}

/// The tab list to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct TabListFrame {
    /// Every player in display order. The renderer shows the first
    /// [`crate::player_list::MAX_VISIBLE`] but measures the columns over all
    /// of them, as the reference does.
    pub rows: Vec<TabRow>,
    /// The style of the score column; `None` when no objective is in the list
    /// slot.
    pub value_style: Option<ValueStyle>,
    /// The header, absent when its text is empty.
    pub header: Option<Vec<Run>>,
    /// The footer, absent when its text is empty.
    pub footer: Option<Vec<Run>>,
}

/// The tab list the player sees, or `None` when nobody is listed.
#[must_use]
pub fn tab_list_frame(
    list: &PlayerList,
    scoreboard: &Scoreboard,
    translations: &Translations,
) -> Option<TabListFrame> {
    if list.is_empty() {
        return None;
    }
    let objective = scoreboard.display_slot(SLOT_LIST);
    let value_style = objective.map(|objective| {
        if objective.render_type == "hearts" {
            ValueStyle::Hearts
        } else {
            ValueStyle::Integer
        }
    });
    let rows = list
        .sorted_all(scoreboard)
        .into_iter()
        .map(|entry| TabRow {
            uuid: entry.uuid,
            name: player_name(entry, scoreboard, translations),
            ping: entry.ping,
            spectator: entry.game_mode == SPECTATOR,
            score: objective.map(|objective| scoreboard.score(&objective.name, &entry.name)),
            textures: entry.textures.clone(),
        })
        .collect();
    Some(TabListFrame {
        rows,
        value_style,
        header: non_empty(list.header(), translations),
        footer: non_empty(list.footer(), translations),
    })
}

/// `GuiPlayerTabOverlay.getPlayerName`: the display name the server set, else
/// the name with its team decoration.
fn player_name(
    entry: &PlayerEntry,
    scoreboard: &Scoreboard,
    translations: &Translations,
) -> Vec<Run> {
    match &entry.display_name {
        Some(json) => parse(json, translations),
        None => parse_legacy(&scoreboard.format_name(&entry.name)),
    }
}

fn non_empty(json: Option<&str>, translations: &Translations) -> Option<Vec<Run>> {
    let runs = parse(json?, translations);
    (!plain(&runs).is_empty()).then_some(runs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_protocol::packets::play::clientbound::{
        AddPlayerEntry, DisplayScoreboard, ObjectiveRenderType, PlayerListHeaderFooter,
        PlayerListItem, ScoreboardObjective, TeamInfo, Teams, UpdateScore,
    };

    fn add(seed: u8, name: &str, game_mode: i32) -> AddPlayerEntry {
        AddPlayerEntry {
            uuid: u128::from_be_bytes([seed; 16]),
            name: name.to_owned(),
            properties: vec![],
            game_mode,
            ping: 20,
            display_name: None,
        }
    }

    fn list(adds: Vec<AddPlayerEntry>) -> PlayerList {
        let mut list = PlayerList::new();
        list.apply(&PlayerListItem::AddPlayer(adds));
        list
    }

    fn frame(list: &PlayerList, board: &Scoreboard) -> TabListFrame {
        tab_list_frame(list, board, &Translations::builtin()).expect("listed")
    }

    fn names(frame: &TabListFrame) -> Vec<String> {
        frame.rows.iter().map(|row| plain(&row.name)).collect()
    }

    #[test]
    fn an_empty_list_has_no_frame() {
        let none = tab_list_frame(
            &PlayerList::new(),
            &Scoreboard::new(),
            &Translations::default(),
        );
        assert_eq!(none, None);
    }

    #[test]
    fn spectators_come_last_then_teams_then_names() {
        let mut board = Scoreboard::new();
        board.apply_teams(&Teams::Create {
            name: "a".to_owned(),
            info: TeamInfo {
                display_name: "A".to_owned(),
                prefix: "[A] ".to_owned(),
                suffix: String::new(),
                friendly_flags: 0,
                name_tag_visibility: "always".to_owned(),
                color: -1,
            },
            players: vec!["Zed".to_owned()],
        });
        let players = list(vec![
            add(1, "Spec", 3),
            add(2, "Zed", 0),
            add(3, "Bob", 0),
            add(4, "Ann", 0),
        ]);
        assert_eq!(
            names(&frame(&players, &board)),
            ["Ann", "Bob", "[A] Zed", "Spec"],
            "the teamless sort before team a"
        );
    }

    #[test]
    fn a_server_display_name_replaces_the_team_decorated_name() {
        let mut entry = add(1, "Steve", 0);
        entry.display_name = Some(r#"{"text":"Hero","color":"red"}"#.to_owned());
        let players = list(vec![entry]);
        let row = &frame(&players, &Scoreboard::new()).rows[0];
        assert_eq!(plain(&row.name), "Hero");
        assert_eq!(row.name[0].style.color.map(|c| c.index()), Some(12));
    }

    #[test]
    fn the_spectator_flag_and_ping_pass_through() {
        let players = list(vec![add(1, "Spec", 3), add(2, "Bob", 1)]);
        let rows = frame(&players, &Scoreboard::new()).rows;
        assert!(!rows[0].spectator && rows[1].spectator);
        assert_eq!(rows[0].ping, 20);
    }

    #[test]
    fn the_list_objective_gives_each_row_a_score() {
        let mut board = Scoreboard::new();
        board.apply_objective(&ScoreboardObjective::Create {
            name: "hp".to_owned(),
            value: "Health".to_owned(),
            render_type: ObjectiveRenderType::Hearts,
        });
        board.apply_score(&UpdateScore::Change {
            name: "Bob".to_owned(),
            objective: "hp".to_owned(),
            value: 14,
        });
        board.apply_display(&DisplayScoreboard {
            position: 0,
            score_name: "hp".to_owned(),
        });
        let players = list(vec![add(1, "Bob", 0), add(2, "Ann", 0)]);
        let frame = frame(&players, &board);
        assert_eq!(frame.value_style, Some(ValueStyle::Hearts));
        assert_eq!(frame.rows[0].score, Some(0), "Ann has no score");
        assert_eq!(frame.rows[1].score, Some(14));
    }

    #[test]
    fn without_a_list_objective_there_is_no_score_column() {
        let players = list(vec![add(1, "Bob", 0)]);
        let frame = frame(&players, &Scoreboard::new());
        assert_eq!(frame.value_style, None);
        assert_eq!(frame.rows[0].score, None);
    }

    #[test]
    fn an_empty_header_or_footer_is_absent() {
        let mut players = list(vec![add(1, "Bob", 0)]);
        players.set_header_footer(&PlayerListHeaderFooter {
            header: r#"{"text":"Welcome","color":"gold"}"#.to_owned(),
            footer: r#"{"text":""}"#.to_owned(),
        });
        let frame = frame(&players, &Scoreboard::new());
        assert_eq!(
            frame.header.as_deref().map(plain).as_deref(),
            Some("Welcome")
        );
        assert_eq!(frame.footer, None);
    }
}

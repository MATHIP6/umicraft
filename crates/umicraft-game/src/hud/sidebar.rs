// SPDX-License-Identifier: MIT OR Apache-2.0

//! The scoreboard sidebar as styled text.
//!
//! The line selection (team colour slot first, at most 15 lines, highest
//! score first, `#` holders hidden) is [`Scoreboard::sidebar`], from MCP
//! `GuiIngame.renderScoreboard`; this module gives the title and the holder
//! names (`ScorePlayerTeam.formatPlayerName`) their `§` styles.

use crate::chat::{local, Run};
use crate::scoreboard::Scoreboard;

/// One sidebar row.
#[derive(Debug, Clone, PartialEq)]
pub struct SidebarLineFrame {
    /// The holder name with its team prefix and suffix.
    pub name: Vec<Run>,
    /// The score, drawn in red at the right edge.
    pub score: i32,
}

/// The sidebar to draw, top row first.
#[derive(Debug, Clone, PartialEq)]
pub struct SidebarFrame {
    /// The objective display name, centred over the rows.
    pub title: Vec<Run>,
    /// The rows.
    pub lines: Vec<SidebarLineFrame>,
}

/// The sidebar `own_name` sees, or `None` when no objective is displayed.
///
/// An objective without scores still has a frame; the renderer draws no
/// title for it, as `renderScoreboard` only draws the title with the last
/// row.
#[must_use]
pub fn sidebar_frame(scoreboard: &Scoreboard, own_name: &str) -> Option<SidebarFrame> {
    let sidebar = scoreboard.sidebar(own_name)?;
    Some(SidebarFrame {
        title: local(&sidebar.title),
        lines: sidebar
            .lines
            .iter()
            .map(|line| SidebarLineFrame {
                name: local(&line.name),
                score: line.value,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::plain;
    use umicraft_protocol::packets::play::clientbound::{
        DisplayScoreboard, ObjectiveRenderType, ScoreboardObjective, TeamInfo, Teams, UpdateScore,
    };

    fn board() -> Scoreboard {
        let mut board = Scoreboard::new();
        board.apply_objective(&ScoreboardObjective::Create {
            name: "k".to_owned(),
            value: "§6Kills".to_owned(),
            render_type: ObjectiveRenderType::Integer,
        });
        for (holder, value) in [("Alex", 4), ("Bob", 9)] {
            board.apply_score(&UpdateScore::Change {
                name: holder.to_owned(),
                objective: "k".to_owned(),
                value,
            });
        }
        board.apply_display(&DisplayScoreboard {
            position: 1,
            score_name: "k".to_owned(),
        });
        board
    }

    #[test]
    fn no_objective_means_no_sidebar() {
        assert_eq!(sidebar_frame(&Scoreboard::new(), "me"), None);
    }

    #[test]
    fn the_title_is_styled_and_rows_run_from_the_highest_score() {
        let frame = sidebar_frame(&board(), "me").expect("shown");
        assert_eq!(plain(&frame.title), "Kills");
        assert_eq!(frame.title[0].style.color.map(|c| c.index()), Some(6));
        let rows: Vec<_> = frame
            .lines
            .iter()
            .map(|l| (plain(&l.name), l.score))
            .collect();
        assert_eq!(rows, [("Bob".to_owned(), 9), ("Alex".to_owned(), 4)]);
    }

    #[test]
    fn a_holder_name_wears_its_team_decoration() {
        let mut board = board();
        board.apply_teams(&Teams::Create {
            name: "red".to_owned(),
            info: TeamInfo {
                display_name: "Red".to_owned(),
                prefix: "§c[R] ".to_owned(),
                suffix: "§r!".to_owned(),
                friendly_flags: 0,
                name_tag_visibility: "always".to_owned(),
                color: -1,
            },
            players: vec!["Bob".to_owned()],
        });
        let frame = sidebar_frame(&board, "me").expect("shown");
        assert_eq!(plain(&frame.lines[0].name), "[R] Bob!");
        assert_eq!(
            frame.lines[0].name[0].style.color.map(|c| c.index()),
            Some(12)
        );
    }
}

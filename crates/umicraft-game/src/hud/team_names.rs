// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the scoreboard adds around a player's name tag: the objective in the
//! below-name slot and the team friendly flags.
//!
//! Sources: MCP `RenderPlayer.renderOffsetLivingLabel` (the score line, 10
//! blocks, never while sneaking), `RendererLivingEntity.renderName` and
//! `Entity.isInvisibleToPlayer` (see also [`crate::entity::nametag`] for the
//! visibility rules and the prefix and suffix).

use crate::entity::nametag::player_tag;
use crate::scoreboard::{Scoreboard, SLOT_BELOW_NAME};

/// The score line is drawn only closer than this, squared
/// (`renderOffsetLivingLabel`: `p_177069_10_ < 100.0D`).
pub const SCORE_LABEL_RANGE_SQ: f64 = 100.0;

/// The objective line under a player's name, `"<score> <display name>"`.
///
/// `None` when no objective is assigned to the below-name slot.
#[must_use]
pub fn below_name_line(scoreboard: &Scoreboard, player: &str) -> Option<String> {
    let objective = scoreboard.display_slot(SLOT_BELOW_NAME)?;
    Some(format!(
        "{} {}",
        scoreboard.score(&objective.name, player),
        objective.display_name
    ))
}

/// The whole label of a remote player: the name tag, then `'\n'` and the
/// below-name line when an objective is shown.
///
/// The renderer splits the two lines, drops the score when the player is
/// crouching or [`SCORE_LABEL_RANGE_SQ`] away, and lifts the name by 1.15
/// text lines over it. `None` when the team rules hide the tag; the score
/// goes with it because both are drawn by `renderName`.
#[must_use]
pub fn player_label(
    scoreboard: &Scoreboard,
    local: &str,
    player: &str,
    invisible: bool,
) -> Option<String> {
    let tag = player_tag(scoreboard, local, player, invisible)?;
    Some(match below_name_line(scoreboard, player) {
        Some(score) => format!("{tag}\n{score}"),
        None => tag,
    })
}

/// Whether the local player sees an invisible `player` as a ghost: both are
/// in the same team and the team sees invisible team mates (friendly flag
/// bit 1, `Entity.isInvisibleToPlayer`).
#[must_use]
pub fn sees_team_ghost(scoreboard: &Scoreboard, local: &str, player: &str) -> bool {
    let (Some(team), Some(own)) = (scoreboard.team_of(player), scoreboard.team_of(local)) else {
        return false;
    };
    team.name == own.name && team.sees_friendly_invisibles()
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_protocol::packets::play::clientbound::{
        DisplayScoreboard, ObjectiveRenderType, ScoreboardObjective, TeamInfo, Teams, UpdateScore,
    };

    fn board_with_team(flags: i8, members: &[&str]) -> Scoreboard {
        let mut board = Scoreboard::new();
        board.apply_teams(&Teams::Create {
            name: "red".to_owned(),
            info: TeamInfo {
                display_name: "Red".to_owned(),
                prefix: "§c".to_owned(),
                suffix: String::new(),
                friendly_flags: flags,
                name_tag_visibility: "always".to_owned(),
                color: 12,
            },
            players: members.iter().map(|m| (*m).to_owned()).collect(),
        });
        board
    }

    fn show_below_name(board: &mut Scoreboard, score: i32) {
        board.apply_objective(&ScoreboardObjective::Create {
            name: "hp".to_owned(),
            value: "§4♥".to_owned(),
            render_type: ObjectiveRenderType::Integer,
        });
        board.apply_score(&UpdateScore::Change {
            name: "Bob".to_owned(),
            objective: "hp".to_owned(),
            value: score,
        });
        board.apply_display(&DisplayScoreboard {
            position: 2,
            score_name: "hp".to_owned(),
        });
    }

    #[test]
    fn the_below_name_objective_adds_a_score_line() {
        let mut board = Scoreboard::new();
        assert_eq!(
            player_label(&board, "me", "Bob", false).as_deref(),
            Some("Bob")
        );
        show_below_name(&mut board, 17);
        assert_eq!(
            player_label(&board, "me", "Bob", false).as_deref(),
            Some("Bob\n17 §4♥")
        );
        assert_eq!(
            below_name_line(&board, "Ann").as_deref(),
            Some("0 §4♥"),
            "a holder without a score has zero"
        );
    }

    #[test]
    fn a_hidden_tag_hides_the_score_too() {
        let mut board = Scoreboard::new();
        show_below_name(&mut board, 3);
        assert_eq!(player_label(&board, "me", "Bob", true), None);
    }

    #[test]
    fn the_label_carries_the_team_decoration() {
        let mut board = board_with_team(0, &["Bob"]);
        show_below_name(&mut board, 1);
        assert_eq!(
            player_label(&board, "me", "Bob", false).as_deref(),
            Some("§cBob\n1 §4♥")
        );
    }

    #[test]
    fn only_team_mates_of_a_team_with_the_flag_are_ghosts() {
        let seeing = board_with_team(0b10, &["Bob", "Ann"]);
        assert!(sees_team_ghost(&seeing, "Ann", "Bob"));
        assert!(!sees_team_ghost(&seeing, "me", "Bob"), "not in the team");
        let blind = board_with_team(0b01, &["Bob", "Ann"]);
        assert!(
            !sees_team_ghost(&blind, "Ann", "Bob"),
            "flag 1 is friendly fire"
        );
    }

    #[test]
    fn the_friendly_flags_are_read_bit_by_bit() {
        let board = board_with_team(0b01, &["Bob"]);
        let team = board.team_of("Bob").expect("member");
        assert!(team.allows_friendly_fire() && !team.sees_friendly_invisibles());
        let board = board_with_team(0b10, &["Bob"]);
        let team = board.team_of("Bob").expect("member");
        assert!(!team.allows_friendly_fire() && team.sees_friendly_invisibles());
    }
}

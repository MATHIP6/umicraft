// SPDX-License-Identifier: MIT OR Apache-2.0

//! Name tags: who gets one and what it says.
//!
//! Source: MCP `RendererLivingEntity.canRenderName` and `renderName`,
//! `RenderLiving.canRenderName`, `EntityPlayer.getDisplayName`,
//! `ScorePlayerTeam.formatPlayerName` and `Entity.isInvisibleToPlayer`.
//! A team's own rule decides even for an invisible member, as in vanilla.
//! Whether the tag is close enough to draw (64 blocks, 32 when crouching)
//! is decided where the camera is known.

use crate::scoreboard::{Scoreboard, Team};

/// The text above a remote player, or `None` when the rules hide it.
///
/// `local` is the name of the local player, `player` the name of the tagged
/// one. The text is the team prefix, the name and the team suffix, with the
/// formatting codes the server wrote.
#[must_use]
pub fn player_tag(
    scoreboard: &Scoreboard,
    local: &str,
    player: &str,
    invisible: bool,
) -> Option<String> {
    let team = scoreboard.team_of(player);
    let local_team = scoreboard.team_of(local);
    let shown = match team {
        Some(team) => team_shows_tag(team, local_team),
        // `Entity.isInvisibleToPlayer`: an invisible player without a team is
        // always invisible to the others.
        None => !invisible,
    };
    shown.then(|| match team {
        Some(team) => format!("{}{player}{}", team.prefix, team.suffix),
        None => player.to_owned(),
    })
}

/// The tag of a named mob: only a custom name that is always shown has one
/// (the name of a mob under the crosshair needs the ray cast of P6).
#[must_use]
pub fn mob_tag(custom_name: Option<&str>, always_visible: bool) -> Option<String> {
    custom_name.filter(|_| always_visible).map(str::to_owned)
}

/// The team rule of `RendererLivingEntity.canRenderName`.
fn team_shows_tag(team: &Team, local_team: Option<&Team>) -> bool {
    match team.name_tag_visibility.as_str() {
        "never" => false,
        "hideForOtherTeams" => local_team.is_none_or(|local| local.name == team.name),
        "hideForOwnTeam" => local_team.is_none_or(|local| local.name != team.name),
        // `always`, and any rule this version does not know.
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_protocol::packets::play::clientbound::teams::{TeamInfo, Teams};

    fn info(prefix: &str, suffix: &str, visibility: &str, flags: i8) -> TeamInfo {
        TeamInfo {
            display_name: "Team".to_owned(),
            prefix: prefix.to_owned(),
            suffix: suffix.to_owned(),
            friendly_flags: flags,
            name_tag_visibility: visibility.to_owned(),
            color: -1,
        }
    }

    fn board(teams: &[(&str, TeamInfo, &[&str])]) -> Scoreboard {
        let mut board = Scoreboard::new();
        for (name, team_info, members) in teams {
            board.apply_teams(&Teams::Create {
                name: (*name).to_owned(),
                info: team_info.clone(),
                players: members.iter().map(|m| (*m).to_owned()).collect(),
            });
        }
        board
    }

    #[test]
    fn a_player_without_a_team_shows_the_plain_name() {
        let board = Scoreboard::new();
        assert_eq!(
            player_tag(&board, "me", "Bob", false).as_deref(),
            Some("Bob")
        );
    }

    #[test]
    fn the_team_prefix_and_suffix_surround_the_name() {
        let board = board(&[("red", info("§c[R] ", "§r!", "always", 0), &["Bob"])]);
        assert_eq!(
            player_tag(&board, "me", "Bob", false).as_deref(),
            Some("§c[R] Bob§r!")
        );
    }

    #[test]
    fn never_hides_and_always_shows_even_an_invisible_teammate() {
        let hidden = board(&[("t", info("", "", "never", 0), &["Bob"])]);
        assert_eq!(player_tag(&hidden, "me", "Bob", false), None);
        let shown = board(&[("t", info("", "", "always", 0), &["Bob"])]);
        assert!(player_tag(&shown, "me", "Bob", true).is_some());
    }

    #[test]
    fn hide_for_other_teams_shows_teammates_and_everyone_when_teamless() {
        let board = board(&[
            ("a", info("", "", "hideForOtherTeams", 0), &["Bob", "Ann"]),
            ("b", info("", "", "always", 0), &["Cid"]),
        ]);
        assert!(
            player_tag(&board, "Ann", "Bob", false).is_some(),
            "same team"
        );
        assert!(
            player_tag(&board, "Cid", "Bob", false).is_none(),
            "other team"
        );
        assert!(
            player_tag(&board, "me", "Bob", false).is_some(),
            "no own team"
        );
    }

    #[test]
    fn hide_for_own_team_hides_teammates() {
        let board = board(&[("a", info("", "", "hideForOwnTeam", 0), &["Bob", "Ann"])]);
        assert!(player_tag(&board, "Ann", "Bob", false).is_none());
        assert!(player_tag(&board, "me", "Bob", false).is_some());
    }

    #[test]
    fn an_unknown_visibility_rule_shows_the_tag() {
        let board = board(&[("a", info("", "", "sometimes", 0), &["Bob"])]);
        assert!(player_tag(&board, "me", "Bob", false).is_some());
    }

    #[test]
    fn an_invisible_player_without_a_team_has_no_tag() {
        let board = Scoreboard::new();
        assert_eq!(player_tag(&board, "me", "Bob", true), None);
    }

    #[test]
    fn only_named_mobs_that_always_show_their_name_have_a_tag() {
        assert_eq!(mob_tag(Some("Bessie"), true).as_deref(), Some("Bessie"));
        assert_eq!(mob_tag(Some("Bessie"), false), None);
        assert_eq!(mob_tag(None, true), None);
    }
}

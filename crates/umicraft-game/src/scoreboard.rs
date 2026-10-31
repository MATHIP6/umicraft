// SPDX-License-Identifier: MIT OR Apache-2.0

//! The scoreboard: objectives, scores, display slots and teams (M10, GAM-7).
//!
//! The model is I/O-free: the game thread feeds it the four scoreboard
//! packets and the renderer reads views of it. Behaviour follows MCP
//! `Scoreboard`, `ScorePlayerTeam` and `NetHandlerPlayClient` (handlers for
//! `S3B`, `S3C`, `S3D`, `S3E`) and `GuiIngame.renderScoreboard`.
//!
//! A server can send a packet that refers to an objective or a team the
//! client does not know. The reference client throws in some of those cases
//! (a duplicate create, for example); here they are ignored or replaced, so a
//! misbehaving plugin cannot stop the session.

use std::collections::BTreeMap;

use umicraft_protocol::packets::play::clientbound::{
    teams, DisplayScoreboard, ObjectiveRenderType, ScoreboardObjective, Teams, UpdateScore,
};

/// The number of display slots: list, sidebar, below name and 16 team
/// sidebars.
pub const DISPLAY_SLOTS: usize = 19;
/// The display slot of the tab list.
pub const SLOT_LIST: i8 = 0;
/// The display slot of the sidebar.
pub const SLOT_SIDEBAR: i8 = 1;
/// The first display slot of the per-team-colour sidebars (`3 + colour`).
pub const SLOT_TEAM_SIDEBAR_BASE: i8 = 3;
/// The sidebar shows at most this many lines.
pub const SIDEBAR_MAX_LINES: usize = 15;

/// A scoreboard objective.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Objective {
    /// The internal name.
    pub name: String,
    /// The name shown to the player (legacy `§` codes allowed).
    pub display_name: String,
    /// How the value is drawn: `"integer"` or `"hearts"`.
    pub render_type: String,
}

/// A scoreboard team.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Team {
    /// The internal name; the tab list sorts by it.
    pub name: String,
    /// The display name.
    pub display_name: String,
    /// Text drawn before a member name.
    pub prefix: String,
    /// Text drawn after a member name.
    pub suffix: String,
    /// Bit 0: friendly fire; bit 1: see invisible team mates.
    pub friendly_flags: i8,
    /// When name tags are visible.
    pub name_tag_visibility: String,
    /// The team colour as a chat formatting index, `-1` for none.
    pub color: i8,
}

/// One line of the sidebar, already formatted with the team of its holder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarLine {
    /// The holder name with its team prefix and suffix.
    pub name: String,
    /// The score.
    pub value: i32,
}

/// The sidebar as the player sees it, top line first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sidebar {
    /// The objective display name.
    pub title: String,
    /// The visible lines, highest score first.
    pub lines: Vec<SidebarLine>,
}

/// The scoreboard of the world the player is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scoreboard {
    objectives: BTreeMap<String, Objective>,
    /// Holder name to objective name to value.
    scores: BTreeMap<String, BTreeMap<String, i32>>,
    teams: BTreeMap<String, Team>,
    /// Member name to team name.
    membership: BTreeMap<String, String>,
    /// Objective name per display slot.
    slots: [Option<String>; DISPLAY_SLOTS],
}

impl Default for Scoreboard {
    fn default() -> Self {
        Self {
            objectives: BTreeMap::new(),
            scores: BTreeMap::new(),
            teams: BTreeMap::new(),
            membership: BTreeMap::new(),
            slots: std::array::from_fn(|_| None),
        }
    }
}

impl Scoreboard {
    /// Creates an empty scoreboard.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies an objective packet: create, remove or update.
    pub fn apply_objective(&mut self, packet: &ScoreboardObjective) {
        match packet {
            ScoreboardObjective::Create {
                name,
                value,
                render_type,
            }
            | ScoreboardObjective::Update {
                name,
                value,
                render_type,
            } => {
                let render_type = match render_type {
                    ObjectiveRenderType::Integer => "integer",
                    ObjectiveRenderType::Hearts => "hearts",
                };
                self.objectives.insert(
                    name.clone(),
                    Objective {
                        name: name.clone(),
                        display_name: value.clone(),
                        render_type: render_type.to_string(),
                    },
                );
            }
            ScoreboardObjective::Remove { name } => {
                self.objectives.remove(name);
                for scores in self.scores.values_mut() {
                    scores.remove(name);
                }
                self.scores.retain(|_, scores| !scores.is_empty());
                for slot in &mut self.slots {
                    if slot.as_deref() == Some(name.as_str()) {
                        *slot = None;
                    }
                }
            }
        }
    }

    /// Applies a score packet: set or remove one line.
    ///
    /// A score for an objective that does not exist is dropped, like the
    /// reference client, which looks the objective up by name.
    pub fn apply_score(&mut self, packet: &UpdateScore) {
        match packet {
            UpdateScore::Change {
                name,
                objective,
                value,
            } => {
                if !self.objectives.contains_key(objective) {
                    return;
                }
                self.scores
                    .entry(name.clone())
                    .or_default()
                    .insert(objective.clone(), *value);
            }
            UpdateScore::Remove { name, objective } => {
                if !self.objectives.contains_key(objective) {
                    return;
                }
                if let Some(scores) = self.scores.get_mut(name) {
                    scores.remove(objective);
                    if scores.is_empty() {
                        self.scores.remove(name);
                    }
                }
            }
        }
    }

    /// Applies a display packet. An empty or unknown objective clears the
    /// slot; a slot outside `0..=18` is ignored.
    pub fn apply_display(&mut self, packet: &DisplayScoreboard) {
        let Ok(slot) = usize::try_from(packet.position) else {
            return;
        };
        let Some(entry) = self.slots.get_mut(slot) else {
            return;
        };
        *entry = self
            .objectives
            .contains_key(&packet.score_name)
            .then(|| packet.score_name.clone());
    }

    /// Applies a team packet: create, remove, update, add or remove members.
    pub fn apply_teams(&mut self, packet: &Teams) {
        match packet {
            Teams::Create {
                name,
                info,
                players,
            } => {
                self.set_team(name, info);
                self.add_members(name, players);
            }
            Teams::Remove { name } => {
                self.teams.remove(name);
                self.membership.retain(|_, team| team != name);
            }
            Teams::Update { name, info } => {
                if self.teams.contains_key(name) {
                    self.set_team(name, info);
                }
            }
            Teams::AddPlayers { name, players } => self.add_members(name, players),
            Teams::RemovePlayers { name, players } => {
                for player in players {
                    if self.membership.get(player) == Some(name) {
                        self.membership.remove(player);
                    }
                }
            }
        }
    }

    fn set_team(&mut self, name: &str, info: &teams::TeamInfo) {
        self.teams.insert(
            name.to_string(),
            Team {
                name: name.to_string(),
                display_name: info.display_name.clone(),
                prefix: info.prefix.clone(),
                suffix: info.suffix.clone(),
                friendly_flags: info.friendly_flags,
                name_tag_visibility: info.name_tag_visibility.clone(),
                color: info.color,
            },
        );
    }

    /// Adds members to a known team; a member leaves its previous team.
    fn add_members(&mut self, team: &str, players: &[String]) {
        if !self.teams.contains_key(team) {
            return;
        }
        for player in players {
            self.membership.insert(player.clone(), team.to_string());
        }
    }

    /// The objective with this name.
    #[must_use]
    pub fn objective(&self, name: &str) -> Option<&Objective> {
        self.objectives.get(name)
    }

    /// The objective shown in a display slot.
    #[must_use]
    pub fn display_slot(&self, slot: i8) -> Option<&Objective> {
        let name = self.slots.get(usize::try_from(slot).ok()?)?.as_deref()?;
        self.objectives.get(name)
    }

    /// The team a player belongs to.
    #[must_use]
    pub fn team_of(&self, player: &str) -> Option<&Team> {
        self.teams.get(self.membership.get(player)?)
    }

    /// The name of a player with its team prefix and suffix
    /// (`ScorePlayerTeam.formatPlayerName`).
    #[must_use]
    pub fn format_name(&self, player: &str) -> String {
        match self.team_of(player) {
            Some(team) => format!("{}{player}{}", team.prefix, team.suffix),
            None => player.to_string(),
        }
    }

    /// The score of `holder` in `objective`, `0` when it has none
    /// (`Scoreboard.getValueFromObjective` creates the score).
    #[must_use]
    pub fn score(&self, objective: &str, holder: &str) -> i32 {
        self.scores
            .get(holder)
            .and_then(|by_objective| by_objective.get(objective))
            .copied()
            .unwrap_or(0)
    }

    /// The scores of an objective in the order of MCP `Score.scoreComparator`:
    /// ascending value, equal values by descending name (ignoring case).
    #[must_use]
    pub fn sorted_scores(&self, objective: &str) -> Vec<(&str, i32)> {
        let mut scores: Vec<(&str, i32)> = self
            .scores
            .iter()
            .filter_map(|(holder, by_objective)| {
                by_objective
                    .get(objective)
                    .map(|value| (holder.as_str(), *value))
            })
            .collect();
        scores.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| b.0.to_lowercase().cmp(&a.0.to_lowercase()))
        });
        scores
    }

    /// The sidebar shown to `own_name`.
    ///
    /// A player on a team with a colour sees the objective assigned to the
    /// slot of that colour first, then the plain sidebar slot
    /// (`GuiIngame.renderScoreboard`). Holders whose name starts with `#` are
    /// hidden and at most [`SIDEBAR_MAX_LINES`] lines are kept, the highest.
    #[must_use]
    pub fn sidebar(&self, own_name: &str) -> Option<Sidebar> {
        let team_slot = self
            .team_of(own_name)
            .filter(|team| (0..=15).contains(&team.color))
            .and_then(|team| self.display_slot(SLOT_TEAM_SIDEBAR_BASE + team.color));
        let objective = team_slot.or_else(|| self.display_slot(SLOT_SIDEBAR))?;

        let mut visible: Vec<(&str, i32)> = self
            .sorted_scores(&objective.name)
            .into_iter()
            .filter(|(holder, _)| !holder.starts_with('#'))
            .collect();
        if visible.len() > SIDEBAR_MAX_LINES {
            visible.drain(..visible.len() - SIDEBAR_MAX_LINES);
        }
        let lines = visible
            .into_iter()
            .rev()
            .map(|(holder, value)| SidebarLine {
                name: self.format_name(holder),
                value,
            })
            .collect();
        Some(Sidebar {
            title: objective.display_name.clone(),
            lines,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_protocol::packets::play::clientbound::TeamInfo;

    fn create_objective(board: &mut Scoreboard, name: &str, title: &str) {
        board.apply_objective(&ScoreboardObjective::Create {
            name: name.to_string(),
            value: title.to_string(),
            render_type: ObjectiveRenderType::Integer,
        });
    }

    fn set_score(board: &mut Scoreboard, holder: &str, objective: &str, value: Option<i32>) {
        board.apply_score(&match value {
            Some(value) => UpdateScore::Change {
                name: holder.to_string(),
                objective: objective.to_string(),
                value,
            },
            None => UpdateScore::Remove {
                name: holder.to_string(),
                objective: objective.to_string(),
            },
        });
    }

    fn display(board: &mut Scoreboard, position: i8, name: &str) {
        board.apply_display(&DisplayScoreboard {
            position,
            score_name: name.to_string(),
        });
    }

    fn team_info(prefix: &str, suffix: &str, color: i8) -> TeamInfo {
        TeamInfo {
            display_name: "t".to_string(),
            prefix: prefix.to_string(),
            suffix: suffix.to_string(),
            friendly_flags: 0,
            name_tag_visibility: "always".to_string(),
            color,
        }
    }

    fn create_team(board: &mut Scoreboard, name: &str, info: TeamInfo, players: &[&str]) {
        board.apply_teams(&Teams::Create {
            name: name.to_string(),
            info,
            players: players.iter().map(|p| (*p).to_string()).collect(),
        });
    }

    #[test]
    fn a_holder_without_a_score_has_zero() {
        let mut board = Scoreboard::new();
        create_objective(&mut board, "kills", "Kills");
        set_score(&mut board, "Alex", "kills", Some(3));
        assert_eq!(board.score("kills", "Alex"), 3);
        assert_eq!(board.score("kills", "Bob"), 0);
        assert_eq!(board.score("deaths", "Alex"), 0);
    }

    #[test]
    fn the_sidebar_lists_the_highest_scores_first() {
        let mut board = Scoreboard::new();
        create_objective(&mut board, "kills", "Kills");
        display(&mut board, SLOT_SIDEBAR, "kills");
        set_score(&mut board, "Alex", "kills", Some(3));
        set_score(&mut board, "Steve", "kills", Some(9));
        set_score(&mut board, "Bob", "kills", Some(5));

        let sidebar = board.sidebar("Alex").expect("a sidebar");
        assert_eq!(sidebar.title, "Kills");
        let names: Vec<_> = sidebar.lines.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Steve", "Bob", "Alex"]);
    }

    #[test]
    fn equal_scores_show_the_smallest_name_on_top() {
        let mut board = Scoreboard::new();
        create_objective(&mut board, "o", "O");
        display(&mut board, SLOT_SIDEBAR, "o");
        for holder in ["a", "b", "c"] {
            set_score(&mut board, holder, "o", Some(1));
        }
        // `Score.scoreComparator` sorts equal values by descending name, and
        // the sidebar reverses that list, so the top line is the smallest name.
        let sidebar = board.sidebar("x").unwrap();
        let names: Vec<_> = sidebar.lines.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c"]);
    }

    #[test]
    fn the_sidebar_keeps_fifteen_lines_and_hides_hash_holders() {
        let mut board = Scoreboard::new();
        create_objective(&mut board, "o", "O");
        display(&mut board, SLOT_SIDEBAR, "o");
        for value in 0..20 {
            set_score(&mut board, &format!("p{value:02}"), "o", Some(value));
        }
        set_score(&mut board, "#total", "o", Some(1000));

        let sidebar = board.sidebar("x").unwrap();
        assert_eq!(sidebar.lines.len(), SIDEBAR_MAX_LINES);
        assert_eq!(sidebar.lines[0].value, 19);
        assert_eq!(sidebar.lines.last().unwrap().value, 5);
        assert!(sidebar.lines.iter().all(|l| !l.name.starts_with('#')));
    }

    #[test]
    fn a_removed_score_disappears() {
        let mut board = Scoreboard::new();
        create_objective(&mut board, "o", "O");
        display(&mut board, SLOT_SIDEBAR, "o");
        set_score(&mut board, "Steve", "o", Some(1));
        set_score(&mut board, "Steve", "o", None);
        assert!(board.sidebar("x").unwrap().lines.is_empty());
    }

    #[test]
    fn a_score_for_an_unknown_objective_is_dropped() {
        let mut board = Scoreboard::new();
        set_score(&mut board, "Steve", "missing", Some(1));
        create_objective(&mut board, "missing", "M");
        assert!(board.sorted_scores("missing").is_empty());
    }

    #[test]
    fn removing_an_objective_clears_its_scores_and_its_slots() {
        let mut board = Scoreboard::new();
        create_objective(&mut board, "o", "O");
        display(&mut board, SLOT_SIDEBAR, "o");
        set_score(&mut board, "Steve", "o", Some(1));
        board.apply_objective(&ScoreboardObjective::Remove {
            name: "o".to_string(),
        });
        assert!(board.sidebar("x").is_none());
        assert!(board.objective("o").is_none());
        create_objective(&mut board, "o", "O");
        assert!(board.sorted_scores("o").is_empty());
    }

    #[test]
    fn updating_an_objective_changes_its_title() {
        let mut board = Scoreboard::new();
        create_objective(&mut board, "o", "Old");
        board.apply_objective(&ScoreboardObjective::Update {
            name: "o".to_string(),
            value: "New".to_string(),
            render_type: ObjectiveRenderType::Hearts,
        });
        assert_eq!(board.objective("o").unwrap().display_name, "New");
    }

    #[test]
    fn a_display_slot_for_an_unknown_objective_or_bad_index_is_ignored() {
        let mut board = Scoreboard::new();
        display(&mut board, SLOT_SIDEBAR, "missing");
        display(&mut board, 40, "missing");
        display(&mut board, -1, "missing");
        assert!(board.display_slot(SLOT_SIDEBAR).is_none());
    }

    #[test]
    fn a_team_colour_selects_its_own_sidebar_slot() {
        let mut board = Scoreboard::new();
        create_objective(&mut board, "plain", "Plain");
        create_objective(&mut board, "red", "Red team");
        display(&mut board, SLOT_SIDEBAR, "plain");
        display(&mut board, SLOT_TEAM_SIDEBAR_BASE + 12, "red");
        create_team(&mut board, "t", team_info("", "", 12), &["Steve"]);

        assert_eq!(board.sidebar("Steve").unwrap().title, "Red team");
        assert_eq!(board.sidebar("Alex").unwrap().title, "Plain");
    }

    #[test]
    fn team_prefix_and_suffix_decorate_the_holder_name() {
        let mut board = Scoreboard::new();
        create_team(&mut board, "t", team_info("[R] ", " *", -1), &["Steve"]);
        assert_eq!(board.format_name("Steve"), "[R] Steve *");
        assert_eq!(board.format_name("Alex"), "Alex");
    }

    #[test]
    fn a_player_moves_between_teams_and_leaves_them() {
        let mut board = Scoreboard::new();
        create_team(&mut board, "a", team_info("", "", -1), &["Steve"]);
        create_team(&mut board, "b", team_info("", "", -1), &[]);
        board.apply_teams(&Teams::AddPlayers {
            name: "b".to_string(),
            players: vec!["Steve".to_string()],
        });
        assert_eq!(board.team_of("Steve").unwrap().name, "b");

        // Removing him from the team he is not in changes nothing.
        board.apply_teams(&Teams::RemovePlayers {
            name: "a".to_string(),
            players: vec!["Steve".to_string()],
        });
        assert_eq!(board.team_of("Steve").unwrap().name, "b");

        board.apply_teams(&Teams::RemovePlayers {
            name: "b".to_string(),
            players: vec!["Steve".to_string()],
        });
        assert!(board.team_of("Steve").is_none());
    }

    #[test]
    fn removing_a_team_frees_its_members() {
        let mut board = Scoreboard::new();
        create_team(&mut board, "a", team_info("", "", -1), &["Steve"]);
        board.apply_teams(&Teams::Remove {
            name: "a".to_string(),
        });
        assert!(board.team_of("Steve").is_none());
    }

    #[test]
    fn members_cannot_join_a_team_that_does_not_exist() {
        let mut board = Scoreboard::new();
        board.apply_teams(&Teams::AddPlayers {
            name: "ghost".to_string(),
            players: vec!["Steve".to_string()],
        });
        assert!(board.team_of("Steve").is_none());
    }
}

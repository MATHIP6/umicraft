// SPDX-License-Identifier: MIT OR Apache-2.0

//! The client copy of the scoreboard: objectives, scores, display slots and
//! teams.
//!
//! Source: MCP `Scoreboard` (`addScoreObjective`, `removeObjective`,
//! `setObjectiveInSlot`, `addPlayerToTeam`, `removeTeam`) and the
//! `NetHandlerPlayClient` handlers for packets 0x3B-0x3E. The reference
//! client throws on inconsistent packets (a duplicate objective, a score for
//! an unknown objective); this model ignores them instead, because a server
//! must never be able to crash the client.

use std::collections::{BTreeMap, BTreeSet};

use umicraft_protocol::packets::play::clientbound::display_scoreboard::DisplayScoreboard;
use umicraft_protocol::packets::play::clientbound::scoreboard_objective::{
    ObjectiveRenderType, ScoreboardObjective,
};
use umicraft_protocol::packets::play::clientbound::teams::{TeamInfo, Teams};
use umicraft_protocol::packets::play::clientbound::update_score::UpdateScore;

/// Number of display slots: 0 list, 1 sidebar, 2 below name, 3-18 the
/// sidebars of the 16 team colours (MCP `Scoreboard.getObjectiveDisplayName`
/// and `getObjectiveInDisplaySlot`).
pub const DISPLAY_SLOT_COUNT: usize = 19;
/// The tab-list slot.
pub const SLOT_LIST: usize = 0;
/// The sidebar slot.
pub const SLOT_SIDEBAR: usize = 1;
/// The below-name slot.
pub const SLOT_BELOW_NAME: usize = 2;

/// A scoreboard objective.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Objective {
    /// The unique internal name.
    pub name: String,
    /// The display name shown in the sidebar title.
    pub display_name: String,
    /// How the value is drawn.
    pub render_type: ObjectiveRenderType,
}

/// A team: its look and its members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Team {
    /// The unique internal name.
    pub name: String,
    /// Display name, prefix, suffix, flags, name-tag visibility and colour.
    pub info: TeamInfo,
    members: BTreeSet<String>,
}

impl Team {
    /// The member names (players or entity UUID strings), sorted.
    pub fn members(&self) -> impl Iterator<Item = &str> {
        self.members.iter().map(String::as_str)
    }
}

/// The scoreboard of the current server session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scoreboard {
    objectives: BTreeMap<String, Objective>,
    scores: BTreeMap<(String, String), i32>,
    display: [Option<String>; DISPLAY_SLOT_COUNT],
    teams: BTreeMap<String, Team>,
    team_of: BTreeMap<String, String>,
}

impl Scoreboard {
    /// Applies an objective packet (0x3B). Creating an existing objective and
    /// updating or removing an unknown one are ignored.
    pub fn apply_objective(&mut self, packet: &ScoreboardObjective) {
        match packet {
            ScoreboardObjective::Create {
                name,
                value,
                render_type,
            } => {
                self.objectives
                    .entry(name.clone())
                    .or_insert_with(|| Objective {
                        name: name.clone(),
                        display_name: value.clone(),
                        render_type: *render_type,
                    });
            }
            ScoreboardObjective::Update {
                name,
                value,
                render_type,
            } => {
                if let Some(objective) = self.objectives.get_mut(name) {
                    objective.display_name.clone_from(value);
                    objective.render_type = *render_type;
                }
            }
            ScoreboardObjective::Remove { name } => self.remove_objective(name),
        }
    }

    /// `Scoreboard.removeObjective`: the scores and display slots of the
    /// objective go with it.
    fn remove_objective(&mut self, name: &str) {
        if self.objectives.remove(name).is_none() {
            return;
        }
        self.scores.retain(|(_, objective), _| objective != name);
        for slot in &mut self.display {
            if slot.as_deref() == Some(name) {
                *slot = None;
            }
        }
    }

    /// Applies a score packet (0x3C). A score for an unknown objective is
    /// ignored.
    pub fn apply_score(&mut self, packet: &UpdateScore) {
        match packet {
            UpdateScore::Change {
                name,
                objective,
                value,
            } => {
                if self.objectives.contains_key(objective) {
                    self.scores
                        .insert((name.clone(), objective.clone()), *value);
                }
            }
            UpdateScore::Remove { name, objective } => {
                self.scores.remove(&(name.clone(), objective.clone()));
            }
        }
    }

    /// Applies a display packet (0x3D): an empty or unknown objective name
    /// clears the slot, a slot outside `0..=18` is ignored (MCP
    /// `handleDisplayScoreboard`, `Scoreboard.setObjectiveInSlot`).
    pub fn apply_display(&mut self, packet: &DisplayScoreboard) {
        let Ok(slot) = usize::try_from(packet.position) else {
            return;
        };
        let known = self.objectives.contains_key(&packet.score_name);
        if let Some(entry) = self.display.get_mut(slot) {
            *entry = known.then(|| packet.score_name.clone());
        }
    }

    /// Applies a team packet (0x3E).
    pub fn apply_team(&mut self, packet: &Teams) {
        match packet {
            Teams::Create {
                name,
                info,
                players,
            } => {
                self.remove_team(name);
                self.teams.insert(
                    name.clone(),
                    Team {
                        name: name.clone(),
                        info: info.clone(),
                        members: BTreeSet::new(),
                    },
                );
                self.add_players(name, players);
            }
            Teams::Remove { name } => self.remove_team(name),
            Teams::Update { name, info } => {
                if let Some(team) = self.teams.get_mut(name) {
                    team.info = info.clone();
                }
            }
            Teams::AddPlayers { name, players } => self.add_players(name, players),
            Teams::RemovePlayers { name, players } => self.remove_players(name, players),
        }
    }

    /// `Scoreboard.addPlayerToTeam`: a player is in one team at a time, so
    /// the previous membership is dropped. An unknown team is ignored.
    fn add_players(&mut self, team: &str, players: &[String]) {
        if !self.teams.contains_key(team) {
            return;
        }
        for player in players {
            if let Some(previous) = self.team_of.insert(player.clone(), team.to_owned()) {
                if let Some(old) = self.teams.get_mut(&previous) {
                    old.members.remove(player);
                }
            }
            if let Some(target) = self.teams.get_mut(team) {
                target.members.insert(player.clone());
            }
        }
    }

    /// `Scoreboard.removePlayerFromTeam`: only members of that team leave.
    fn remove_players(&mut self, team: &str, players: &[String]) {
        let Some(target) = self.teams.get_mut(team) else {
            return;
        };
        for player in players {
            if target.members.remove(player) {
                self.team_of.remove(player);
            }
        }
    }

    /// `Scoreboard.removeTeam`: the members become team-less.
    fn remove_team(&mut self, name: &str) {
        if let Some(team) = self.teams.remove(name) {
            for member in &team.members {
                self.team_of.remove(member);
            }
        }
    }

    /// The objective with this name.
    pub fn objective(&self, name: &str) -> Option<&Objective> {
        self.objectives.get(name)
    }

    /// All objectives, sorted by name.
    pub fn objectives(&self) -> impl Iterator<Item = &Objective> {
        self.objectives.values()
    }

    /// The score of `owner` in `objective`.
    pub fn score(&self, owner: &str, objective: &str) -> Option<i32> {
        self.scores
            .get(&(owner.to_owned(), objective.to_owned()))
            .copied()
    }

    /// The `(owner, value)` pairs of an objective, sorted by owner.
    pub fn scores_of<'a>(&'a self, objective: &'a str) -> impl Iterator<Item = (&'a str, i32)> {
        self.scores
            .iter()
            .filter(move |((_, name), _)| name == objective)
            .map(|((owner, _), value)| (owner.as_str(), *value))
    }

    /// The objective shown in a display slot (`0..=18`).
    pub fn display_slot(&self, slot: usize) -> Option<&Objective> {
        let name = self.display.get(slot)?.as_deref()?;
        self.objectives.get(name)
    }

    /// The team with this name.
    pub fn team(&self, name: &str) -> Option<&Team> {
        self.teams.get(name)
    }

    /// All teams, sorted by name.
    pub fn teams(&self) -> impl Iterator<Item = &Team> {
        self.teams.values()
    }

    /// The team a player belongs to.
    pub fn team_of(&self, player: &str) -> Option<&Team> {
        self.teams.get(self.team_of.get(player)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create(name: &str) -> ScoreboardObjective {
        ScoreboardObjective::Create {
            name: name.to_owned(),
            value: format!("title {name}"),
            render_type: ObjectiveRenderType::Integer,
        }
    }

    fn score(owner: &str, objective: &str, value: i32) -> UpdateScore {
        UpdateScore::Change {
            name: owner.to_owned(),
            objective: objective.to_owned(),
            value,
        }
    }

    fn display(position: i8, name: &str) -> DisplayScoreboard {
        DisplayScoreboard {
            position,
            score_name: name.to_owned(),
        }
    }

    fn info() -> TeamInfo {
        TeamInfo {
            display_name: "Red".to_owned(),
            prefix: "[R]".to_owned(),
            suffix: String::new(),
            friendly_flags: 3,
            name_tag_visibility: "always".to_owned(),
            color: 12,
        }
    }

    fn team_create(name: &str, players: &[&str]) -> Teams {
        Teams::Create {
            name: name.to_owned(),
            info: info(),
            players: players.iter().map(|p| (*p).to_owned()).collect(),
        }
    }

    fn names(players: &[&str]) -> Vec<String> {
        players.iter().map(|p| (*p).to_owned()).collect()
    }

    #[test]
    fn objectives_are_created_updated_and_not_duplicated() {
        let mut board = Scoreboard::default();
        board.apply_objective(&create("kills"));
        board.apply_objective(&ScoreboardObjective::Create {
            name: "kills".to_owned(),
            value: "other".to_owned(),
            render_type: ObjectiveRenderType::Hearts,
        });
        assert_eq!(
            board.objective("kills").unwrap().display_name,
            "title kills"
        );
        board.apply_objective(&ScoreboardObjective::Update {
            name: "kills".to_owned(),
            value: "Kills".to_owned(),
            render_type: ObjectiveRenderType::Hearts,
        });
        let objective = board.objective("kills").unwrap();
        assert_eq!(objective.display_name, "Kills");
        assert_eq!(objective.render_type, ObjectiveRenderType::Hearts);
        assert_eq!(board.objectives().count(), 1);
    }

    #[test]
    fn scores_change_and_remove() {
        let mut board = Scoreboard::default();
        board.apply_objective(&create("kills"));
        board.apply_score(&score("Alex", "kills", 4));
        board.apply_score(&score("Bob", "kills", 9));
        board.apply_score(&score("Alex", "kills", 5));
        assert_eq!(board.score("Alex", "kills"), Some(5));
        assert_eq!(
            board.scores_of("kills").collect::<Vec<_>>(),
            vec![("Alex", 5), ("Bob", 9)]
        );
        board.apply_score(&UpdateScore::Remove {
            name: "Alex".to_owned(),
            objective: "kills".to_owned(),
        });
        assert_eq!(board.score("Alex", "kills"), None);
    }

    #[test]
    fn a_score_for_an_unknown_objective_is_ignored() {
        let mut board = Scoreboard::default();
        board.apply_score(&score("Alex", "ghost", 1));
        board.apply_score(&UpdateScore::Remove {
            name: "Alex".to_owned(),
            objective: "ghost".to_owned(),
        });
        assert_eq!(board.score("Alex", "ghost"), None);
    }

    #[test]
    fn removing_an_objective_removes_its_scores_and_display_slots() {
        let mut board = Scoreboard::default();
        board.apply_objective(&create("a"));
        board.apply_objective(&create("b"));
        board.apply_score(&score("x", "a", 1));
        board.apply_score(&score("x", "b", 2));
        board.apply_display(&display(1, "a"));
        board.apply_display(&display(18, "a"));
        board.apply_display(&display(0, "b"));
        board.apply_objective(&ScoreboardObjective::Remove {
            name: "a".to_owned(),
        });
        assert_eq!(board.score("x", "a"), None);
        assert_eq!(board.score("x", "b"), Some(2));
        assert!(board.display_slot(SLOT_SIDEBAR).is_none());
        assert!(board.display_slot(18).is_none());
        assert_eq!(board.display_slot(SLOT_LIST).unwrap().name, "b");
        board.apply_objective(&ScoreboardObjective::Remove {
            name: "missing".to_owned(),
        });
    }

    #[test]
    fn display_slots_set_clear_and_reject_bad_input() {
        let mut board = Scoreboard::default();
        board.apply_objective(&create("a"));
        board.apply_display(&display(2, "a"));
        assert_eq!(board.display_slot(SLOT_BELOW_NAME).unwrap().name, "a");
        board.apply_display(&display(2, ""));
        assert!(board.display_slot(SLOT_BELOW_NAME).is_none());
        board.apply_display(&display(2, "a"));
        board.apply_display(&display(2, "unknown"));
        assert!(board.display_slot(SLOT_BELOW_NAME).is_none());
        board.apply_display(&display(19, "a"));
        board.apply_display(&display(-1, "a"));
        assert!(board.display_slot(19).is_none());
        assert!((0..DISPLAY_SLOT_COUNT).all(|s| board.display_slot(s).is_none()));
    }

    #[test]
    fn a_team_is_created_with_members_and_updated() {
        let mut board = Scoreboard::default();
        board.apply_team(&team_create("red", &["Alex", "Bob"]));
        assert_eq!(
            board.team("red").unwrap().members().collect::<Vec<_>>(),
            ["Alex", "Bob"]
        );
        assert_eq!(board.team_of("Alex").unwrap().name, "red");
        let mut changed = info();
        changed.color = 1;
        board.apply_team(&Teams::Update {
            name: "red".to_owned(),
            info: changed,
        });
        assert_eq!(board.team("red").unwrap().info.color, 1);
        assert_eq!(board.teams().count(), 1);
    }

    #[test]
    fn joining_a_team_leaves_the_previous_one() {
        let mut board = Scoreboard::default();
        board.apply_team(&team_create("red", &["Alex"]));
        board.apply_team(&team_create("blue", &[]));
        board.apply_team(&Teams::AddPlayers {
            name: "blue".to_owned(),
            players: names(&["Alex"]),
        });
        assert_eq!(board.team("red").unwrap().members().count(), 0);
        assert_eq!(board.team_of("Alex").unwrap().name, "blue");
    }

    #[test]
    fn remove_players_only_affects_members_of_that_team() {
        let mut board = Scoreboard::default();
        board.apply_team(&team_create("red", &["Alex"]));
        board.apply_team(&team_create("blue", &[]));
        board.apply_team(&Teams::RemovePlayers {
            name: "blue".to_owned(),
            players: names(&["Alex"]),
        });
        assert_eq!(board.team_of("Alex").unwrap().name, "red");
        board.apply_team(&Teams::RemovePlayers {
            name: "red".to_owned(),
            players: names(&["Alex", "Nobody"]),
        });
        assert!(board.team_of("Alex").is_none());
    }

    #[test]
    fn removing_a_team_frees_its_members_and_unknown_teams_are_ignored() {
        let mut board = Scoreboard::default();
        board.apply_team(&team_create("red", &["Alex"]));
        board.apply_team(&Teams::Remove {
            name: "red".to_owned(),
        });
        assert!(board.team("red").is_none());
        assert!(board.team_of("Alex").is_none());
        board.apply_team(&Teams::Remove {
            name: "ghost".to_owned(),
        });
        board.apply_team(&Teams::Update {
            name: "ghost".to_owned(),
            info: info(),
        });
        board.apply_team(&Teams::AddPlayers {
            name: "ghost".to_owned(),
            players: names(&["Alex"]),
        });
        board.apply_team(&Teams::RemovePlayers {
            name: "ghost".to_owned(),
            players: names(&["Alex"]),
        });
        assert!(board.team_of("Alex").is_none());
        assert_eq!(board.teams().count(), 0);
    }

    #[test]
    fn recreating_a_team_resets_its_members() {
        let mut board = Scoreboard::default();
        board.apply_team(&team_create("red", &["Alex"]));
        board.apply_team(&team_create("red", &["Bob"]));
        assert!(board.team_of("Alex").is_none());
        assert_eq!(board.team_of("Bob").unwrap().name, "red");
    }
}

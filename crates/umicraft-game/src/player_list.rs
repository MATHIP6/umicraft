// SPDX-License-Identifier: MIT OR Apache-2.0

//! The tab list: the players of the server, their ping and game mode, and the
//! header and footer (M10, GAM-7).
//!
//! The model is I/O-free: the game thread applies `Player List Item` and
//! `Player List Header and Footer` packets and the renderer reads the sorted
//! entries. Ordering follows MCP `GuiPlayerTabOverlay.PlayerComparator` and
//! the 80-entry cap of `renderPlayerlist`.

use umicraft_protocol::packets::play::clientbound::{
    AddPlayerEntry, PlayerListHeaderFooter, PlayerListItem,
};

use crate::scoreboard::Scoreboard;

/// A player UUID, most significant bits first.
pub type Uuid = u128;

/// The tab list shows at most this many players.
pub const MAX_VISIBLE: usize = 80;
/// The game mode id of spectators, who are listed last.
const SPECTATOR: i32 = 3;
/// The profile property that carries the skin textures.
const TEXTURES_PROPERTY: &str = "textures";

/// One player of the tab list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerEntry {
    /// The player UUID.
    pub uuid: Uuid,
    /// The player name.
    pub name: String,
    /// The game mode id (0 survival, 1 creative, 2 adventure, 3 spectator).
    pub game_mode: i32,
    /// The latency in milliseconds.
    pub ping: i32,
    /// The display name as chat JSON, when the server overrides the name.
    pub display_name: Option<String>,
    /// The base64 `textures` property that holds the skin, when present.
    pub textures: Option<String>,
}

impl PlayerEntry {
    fn from_packet(add: &AddPlayerEntry) -> Self {
        Self {
            uuid: add.uuid,
            name: add.name.clone(),
            game_mode: add.game_mode,
            ping: add.ping,
            display_name: add.display_name.clone(),
            textures: add
                .properties
                .iter()
                .find(|property| property.name == TEXTURES_PROPERTY)
                .map(|property| property.value.clone()),
        }
    }
}

/// The tab list of the server.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayerList {
    entries: Vec<PlayerEntry>,
    header: Option<String>,
    footer: Option<String>,
}

impl PlayerList {
    /// Creates an empty tab list.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies a `Player List Item` packet.
    ///
    /// An update for a player that is not listed is ignored, like the
    /// reference client, which looks the player up by UUID.
    pub fn apply(&mut self, packet: &PlayerListItem) {
        match packet {
            PlayerListItem::AddPlayer(adds) => {
                for add in adds {
                    let entry = PlayerEntry::from_packet(add);
                    match self.entry_mut(&add.uuid) {
                        Some(existing) => *existing = entry,
                        None => self.entries.push(entry),
                    }
                }
            }
            PlayerListItem::UpdateGameMode(updates) => {
                for (uuid, game_mode) in updates {
                    if let Some(entry) = self.entry_mut(uuid) {
                        entry.game_mode = *game_mode;
                    }
                }
            }
            PlayerListItem::UpdateLatency(updates) => {
                for (uuid, ping) in updates {
                    if let Some(entry) = self.entry_mut(uuid) {
                        entry.ping = *ping;
                    }
                }
            }
            PlayerListItem::UpdateDisplayName(updates) => {
                for (uuid, name) in updates {
                    if let Some(entry) = self.entry_mut(uuid) {
                        entry.display_name = name.clone();
                    }
                }
            }
            PlayerListItem::RemovePlayer(uuids) => {
                self.entries.retain(|entry| !uuids.contains(&entry.uuid));
            }
        }
    }

    /// Stores the header and footer. They stay raw chat JSON: the renderer
    /// flattens them and treats an empty text as absent.
    pub fn set_header_footer(&mut self, packet: &PlayerListHeaderFooter) {
        self.header = Some(packet.header.clone());
        self.footer = Some(packet.footer.clone());
    }

    /// The header as chat JSON.
    #[must_use]
    pub fn header(&self) -> Option<&str> {
        self.header.as_deref()
    }

    /// The footer as chat JSON.
    #[must_use]
    pub fn footer(&self) -> Option<&str> {
        self.footer.as_deref()
    }

    /// The number of listed players.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nobody is listed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entry of a player.
    #[must_use]
    pub fn get(&self, uuid: &Uuid) -> Option<&PlayerEntry> {
        self.entries.iter().find(|entry| entry.uuid == *uuid)
    }

    fn entry_mut(&mut self, uuid: &Uuid) -> Option<&mut PlayerEntry> {
        self.entries.iter_mut().find(|entry| entry.uuid == *uuid)
    }

    /// The players in display order: spectators last, then by team name, then
    /// by player name, capped at [`MAX_VISIBLE`].
    #[must_use]
    pub fn sorted(&self, scoreboard: &Scoreboard) -> Vec<&PlayerEntry> {
        let team = |entry: &PlayerEntry| {
            scoreboard
                .team_of(&entry.name)
                .map_or("", |team| team.name.as_str())
        };
        let mut sorted: Vec<&PlayerEntry> = self.entries.iter().collect();
        sorted.sort_by(|a, b| {
            (a.game_mode == SPECTATOR)
                .cmp(&(b.game_mode == SPECTATOR))
                .then_with(|| team(a).cmp(team(b)))
                .then_with(|| a.name.cmp(&b.name))
        });
        sorted.truncate(MAX_VISIBLE);
        sorted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_protocol::packets::play::clientbound::{PlayerProperty, TeamInfo, Teams};

    fn id(seed: u8) -> Uuid {
        u128::from_be_bytes([seed; 16])
    }

    fn add(seed: u8, name: &str, game_mode: i32) -> AddPlayerEntry {
        AddPlayerEntry {
            uuid: id(seed),
            name: name.to_string(),
            properties: vec![],
            game_mode,
            ping: 10,
            display_name: None,
        }
    }

    fn list_of(adds: Vec<AddPlayerEntry>) -> PlayerList {
        let mut list = PlayerList::new();
        list.apply(&PlayerListItem::AddPlayer(adds));
        list
    }

    fn names<'a>(entries: &[&'a PlayerEntry]) -> Vec<&'a str> {
        entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn players_are_added_updated_and_removed() {
        let mut list = list_of(vec![add(1, "Steve", 0), add(2, "Alex", 0)]);
        assert_eq!(list.len(), 2);

        list.apply(&PlayerListItem::UpdateLatency(vec![(id(1), 99)]));
        list.apply(&PlayerListItem::UpdateGameMode(vec![(id(1), 1)]));
        list.apply(&PlayerListItem::UpdateDisplayName(vec![(
            id(1),
            Some(r#"{"text":"S"}"#.to_string()),
        )]));
        let steve = list.get(&id(1)).unwrap();
        assert_eq!((steve.ping, steve.game_mode), (99, 1));
        assert_eq!(steve.display_name.as_deref(), Some(r#"{"text":"S"}"#));

        list.apply(&PlayerListItem::UpdateDisplayName(vec![(id(1), None)]));
        assert_eq!(list.get(&id(1)).unwrap().display_name, None);

        list.apply(&PlayerListItem::RemovePlayer(vec![id(1)]));
        assert_eq!(list.len(), 1);
        assert!(list.get(&id(1)).is_none());
    }

    #[test]
    fn adding_a_known_uuid_replaces_the_entry() {
        let mut list = list_of(vec![add(1, "Steve", 0)]);
        list.apply(&PlayerListItem::AddPlayer(vec![add(1, "Steve", 2)]));
        assert_eq!(list.len(), 1);
        assert_eq!(list.get(&id(1)).unwrap().game_mode, 2);
    }

    #[test]
    fn an_update_for_an_unknown_player_is_ignored() {
        let mut list = PlayerList::new();
        list.apply(&PlayerListItem::UpdateLatency(vec![(id(9), 5)]));
        list.apply(&PlayerListItem::RemovePlayer(vec![id(9)]));
        assert!(list.is_empty());
    }

    #[test]
    fn the_skin_comes_from_the_textures_property() {
        let mut with_skin = add(1, "Steve", 0);
        with_skin.properties = vec![
            PlayerProperty {
                name: "other".to_string(),
                value: "x".to_string(),
                signature: None,
            },
            PlayerProperty {
                name: "textures".to_string(),
                value: "eyJ0".to_string(),
                signature: Some("sig".to_string()),
            },
        ];
        let list = list_of(vec![with_skin, add(2, "Alex", 0)]);
        assert_eq!(list.get(&id(1)).unwrap().textures.as_deref(), Some("eyJ0"));
        assert_eq!(list.get(&id(2)).unwrap().textures, None);
    }

    #[test]
    fn spectators_come_last_then_team_then_name() {
        let list = list_of(vec![
            add(1, "Zed", 0),
            add(2, "Amy", 3),
            add(3, "Bob", 0),
            add(4, "Cat", 0),
        ]);
        let mut board = Scoreboard::new();
        board.apply_teams(&Teams::Create {
            name: "red".to_string(),
            info: TeamInfo {
                display_name: "Red".to_string(),
                prefix: String::new(),
                suffix: String::new(),
                friendly_flags: 0,
                name_tag_visibility: "always".to_string(),
                color: 12,
            },
            players: vec!["Zed".to_string()],
        });
        // No team sorts before "red"; the spectator is last of all.
        assert_eq!(
            names(&list.sorted(&board)),
            ["Bob", "Cat", "Zed", "Amy"].as_slice()
        );
    }

    #[test]
    fn the_list_is_capped_at_eighty_players() {
        let list = list_of(
            (0..100u8)
                .map(|seed| add(seed, &format!("p{seed:03}"), 0))
                .collect(),
        );
        assert_eq!(list.sorted(&Scoreboard::new()).len(), MAX_VISIBLE);
    }

    #[test]
    fn the_header_and_footer_are_stored_raw() {
        let mut list = PlayerList::new();
        assert_eq!(list.header(), None);
        list.set_header_footer(&PlayerListHeaderFooter {
            header: r#"{"text":"top"}"#.to_string(),
            footer: r#"{"text":"bottom"}"#.to_string(),
        });
        assert_eq!(list.header(), Some(r#"{"text":"top"}"#));
        assert_eq!(list.footer(), Some(r#"{"text":"bottom"}"#));
    }
}

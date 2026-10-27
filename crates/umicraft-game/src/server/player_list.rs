// SPDX-License-Identifier: MIT OR Apache-2.0

//! The tab list: the players the server announced, plus the header and
//! footer text.
//!
//! Source: MCP `NetHandlerPlayClient.handlePlayerListItem`,
//! `NetworkPlayerInfo` and `handlePlayerListHeaderFooter`. Updates for a
//! player that was never added are dropped, as the reference client does.

use std::collections::BTreeMap;

use umicraft_protocol::packets::play::clientbound::player_list_header_footer::PlayerListHeaderFooter;
use umicraft_protocol::packets::play::clientbound::player_list_item::{
    AddPlayerEntry, PlayerListItem, PlayerProperty,
};

/// One tab-list entry (MCP `NetworkPlayerInfo`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerInfo {
    /// The profile UUID.
    pub uuid: u128,
    /// The profile name.
    pub name: String,
    /// The profile properties (skin textures).
    pub properties: Vec<PlayerProperty>,
    /// The game mode id (0 survival .. 3 spectator).
    pub game_mode: i32,
    /// The latency in milliseconds.
    pub ping: i32,
    /// The display name as chat JSON, when the server overrides the name.
    pub display_name: Option<String>,
}

impl From<&AddPlayerEntry> for PlayerInfo {
    fn from(entry: &AddPlayerEntry) -> Self {
        Self {
            uuid: entry.uuid,
            name: entry.name.clone(),
            properties: entry.properties.clone(),
            game_mode: entry.game_mode,
            ping: entry.ping,
            display_name: entry.display_name.clone(),
        }
    }
}

/// The tab list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayerList {
    players: BTreeMap<u128, PlayerInfo>,
    header: String,
    footer: String,
}

impl PlayerList {
    /// Applies a player list packet.
    pub fn apply(&mut self, packet: &PlayerListItem) {
        match packet {
            PlayerListItem::AddPlayer(entries) => {
                for entry in entries {
                    self.players.insert(entry.uuid, PlayerInfo::from(entry));
                }
            }
            PlayerListItem::UpdateGameMode(updates) => {
                self.update(updates, |info, mode| info.game_mode = *mode);
            }
            PlayerListItem::UpdateLatency(updates) => {
                self.update(updates, |info, ping| info.ping = *ping);
            }
            PlayerListItem::UpdateDisplayName(updates) => {
                self.update(updates, |info, name| info.display_name = name.clone());
            }
            PlayerListItem::RemovePlayer(uuids) => {
                for uuid in uuids {
                    self.players.remove(uuid);
                }
            }
        }
    }

    fn update<T>(&mut self, updates: &[(u128, T)], set: impl Fn(&mut PlayerInfo, &T)) {
        for (uuid, value) in updates {
            if let Some(info) = self.players.get_mut(uuid) {
                set(info, value);
            }
        }
    }

    /// Stores the header and footer chat JSON (MCP
    /// `handlePlayerListHeaderFooter`). An empty string means "none".
    pub fn apply_header_footer(&mut self, packet: &PlayerListHeaderFooter) {
        self.header.clone_from(&packet.header);
        self.footer.clone_from(&packet.footer);
    }

    /// The entry of a player.
    pub fn get(&self, uuid: u128) -> Option<&PlayerInfo> {
        self.players.get(&uuid)
    }

    /// All entries in UUID order.
    pub fn iter(&self) -> impl Iterator<Item = &PlayerInfo> {
        self.players.values()
    }

    /// The number of listed players.
    pub fn len(&self) -> usize {
        self.players.len()
    }

    /// Whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }

    /// The header chat JSON ("" when unset).
    pub fn header(&self) -> &str {
        &self.header
    }

    /// The footer chat JSON ("" when unset).
    pub fn footer(&self) -> &str {
        &self.footer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(uuid: u128, name: &str) -> AddPlayerEntry {
        AddPlayerEntry {
            uuid,
            name: name.to_owned(),
            properties: vec![],
            game_mode: 0,
            ping: 10,
            display_name: None,
        }
    }

    fn list_with(uuid: u128) -> PlayerList {
        let mut list = PlayerList::default();
        list.apply(&PlayerListItem::AddPlayer(vec![entry(uuid, "Alex")]));
        list
    }

    #[test]
    fn add_stores_and_replaces_an_entry() {
        let mut list = list_with(1);
        assert_eq!(list.get(1).unwrap().name, "Alex");
        list.apply(&PlayerListItem::AddPlayer(vec![entry(1, "Steve")]));
        assert_eq!(list.len(), 1);
        assert_eq!(list.get(1).unwrap().name, "Steve");
    }

    #[test]
    fn updates_change_one_field_each() {
        let mut list = list_with(1);
        list.apply(&PlayerListItem::UpdateGameMode(vec![(1, 3)]));
        list.apply(&PlayerListItem::UpdateLatency(vec![(1, 250)]));
        list.apply(&PlayerListItem::UpdateDisplayName(vec![(
            1,
            Some("{\"text\":\"A\"}".to_owned()),
        )]));
        let info = list.get(1).unwrap();
        assert_eq!((info.game_mode, info.ping), (3, 250));
        assert_eq!(info.display_name.as_deref(), Some("{\"text\":\"A\"}"));
        list.apply(&PlayerListItem::UpdateDisplayName(vec![(1, None)]));
        assert_eq!(list.get(1).unwrap().display_name, None);
    }

    #[test]
    fn updates_and_removals_of_unknown_players_are_ignored() {
        let mut list = list_with(1);
        list.apply(&PlayerListItem::UpdateLatency(vec![(99, 5)]));
        list.apply(&PlayerListItem::RemovePlayer(vec![99]));
        assert_eq!(list.len(), 1);
        assert_eq!(list.get(1).unwrap().ping, 10);
    }

    #[test]
    fn remove_deletes_the_entry() {
        let mut list = list_with(1);
        list.apply(&PlayerListItem::RemovePlayer(vec![1]));
        assert!(list.is_empty());
        assert_eq!(list.iter().count(), 0);
    }

    #[test]
    fn header_and_footer_are_stored() {
        let mut list = PlayerList::default();
        list.apply_header_footer(&PlayerListHeaderFooter {
            header: "h".to_owned(),
            footer: "f".to_owned(),
        });
        assert_eq!((list.header(), list.footer()), ("h", "f"));
    }
}

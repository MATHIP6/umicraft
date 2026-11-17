// SPDX-License-Identifier: MIT OR Apache-2.0

//! The packets about the tab list.

use umicraft_protocol::packets::play::clientbound::player_list_item::PlayerListItem;

use crate::game::state::Game;
use crate::profiles::profile_from_properties;

/// `Player List Item`: the tab list, and the profiles (skins) of the
/// players it adds.
pub(super) fn update_player_list(game: &mut Game, packet: &PlayerListItem) {
    game.player_list.apply(packet);
    game.server.apply_player_list(packet);
    if let PlayerListItem::AddPlayer(entries) = packet {
        for entry in entries {
            let profile = profile_from_properties(&entry.name, &entry.properties);
            // A skin plugin re-sends the own entry once the skin is
            // applied, so the local body must follow the update.
            if entry.name == game.own_name {
                game.body.uuid = Some(entry.uuid.to_be_bytes());
                game.body.profile = Some(std::sync::Arc::new(profile.clone()));
            }
            game.entities.set_profile(entry.uuid.to_be_bytes(), profile);
        }
    }
    game.hud_dirty = true;
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The body of the local player, animated like any other player and drawn
//! in the third-person perspectives.

use umicraft_assets::profile_textures::AccountSkinFetcher;
use umicraft_game::entity::{EntityFlags, EntityId, EntityItem};

use super::state::Game;
use crate::profiles::{profile_from_properties, with_account_textures};

/// Collects the skin of the account when the lookup finished, and puts it on
/// the profile of the local body for what that profile lacks. The profile is
/// replaced again when the tab list re-sends it without the skin.
pub(super) fn apply_account_skin(game: &mut Game) {
    if let Some(found) = game
        .account_skin
        .as_ref()
        .and_then(AccountSkinFetcher::poll)
    {
        tracing::debug!(skin = ?found.skin, cape = ?found.cape, "skin of the account found");
        game.account_textures = Some(found);
    }
    let (Some(profile), Some(account)) =
        (game.body.profile.as_ref(), game.account_textures.as_ref())
    else {
        return;
    };
    if let Some(worn) = with_account_textures(profile, account) {
        game.body.profile = Some(std::sync::Arc::new(worn));
    }
}

/// Moves the body of the local player to where the tick left the player.
///
/// The body is animated every tick, also in first person, so that a switch
/// to the third person does not start from a standing pose.
pub(super) fn follow_body(game: &mut Game) {
    // The skin of the account, premium or not: the tab list of an offline
    // server has none for the local player, whose account has one all the same.
    // The account is only asked when the server sent no texture at all, as
    // `SkinManager.loadProfileTextures` does.
    apply_account_skin(game);
    let Some(state) = game.state.as_ref() else {
        return;
    };
    // The same bits the server sends for a remote player: sneaking 0x02 and
    // using the held item 0x10 (the sword blocks, the bow is aimed).
    let sneaking = if state.player().sneaking { 0x02 } else { 0 }
        | if game.item_use.is_using() { 0x10 } else { 0 };
    game.body.id = EntityId(game.entity_id);
    game.body.local = true;
    // The skin and the cape of the local player come from its own tab list
    // entry, which is found by name; until it arrives the body wears the
    // default skin.
    if game.body.profile.is_none() {
        let own = game
            .server
            .player_list()
            .iter()
            .find(|info| info.name == game.own_name);
        if let Some(info) = own {
            game.body.uuid = Some(info.uuid.to_be_bytes());
            game.body.profile = Some(std::sync::Arc::new(profile_from_properties(
                &info.name,
                &info.properties,
            )));
        }
    }
    // The local stack comes from the windows (ADR-0025), not from `Entity
    // Equipment`, so the body shows the item in the second and third person.
    game.body.held = game
        .windows
        .selected_item()
        .and_then(EntityItem::from_stack);
    game.body.armor = game
        .windows
        .worn_armor()
        .map(|piece| piece.and_then(EntityItem::from_stack));
    game.body.follow(
        state.position(),
        game.yaw.to_degrees(),
        game.pitch.to_degrees(),
        EntityFlags::new(sneaking, 0x7F),
    );
    // The swing of the third person arm advances with the tick, as the
    // animation of every tracked entity does in `EntityStore::tick`.
    game.body.animation.tick();
}

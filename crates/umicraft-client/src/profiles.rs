// SPDX-License-Identifier: MIT OR Apache-2.0

//! Game profiles of remote players, from the tab list.
//!
//! The server sends the skin and cape of a player as the signed `textures`
//! property of its `Player List Item` entry (MCP `NetworkPlayerInfo`,
//! `SkinManager.loadProfileTextures`). The client reads the URLs from it; it
//! does not check the signature, as vanilla does not either.

use umicraft_assets::profile_textures::{parse_textures_property, ProfileTextures};
use umicraft_game::entity::PlayerProfile;
use umicraft_protocol::packets::play::clientbound::player_list_item::PlayerProperty;

/// Property that carries the texture URLs.
const TEXTURES_PROPERTY: &str = "textures";

/// The profile of a player named `name` with the given properties.
///
/// A missing or unreadable `textures` property is not an error: the player
/// simply wears the default skin of their UUID.
#[must_use]
pub fn profile_from_properties(name: &str, properties: &[PlayerProperty]) -> PlayerProfile {
    let mut profile = PlayerProfile::named(name);
    let property = properties
        .iter()
        .find(|property| property.name == TEXTURES_PROPERTY);
    if property.is_none() {
        tracing::debug!(player = name, "no textures property: default skin");
    }
    let textures = property.and_then(|property| {
        parse_textures_property(&property.value)
            .inspect_err(
                |error| tracing::warn!(player = name, %error, "unreadable textures property"),
            )
            .ok()
    });
    if let Some(textures) = textures {
        profile.slim = textures.skin.is_some().then_some(textures.slim);
        profile.skin_url = textures.skin.as_deref().map(Into::into);
        profile.cape_url = textures.cape.as_deref().map(Into::into);
    }
    profile
}

/// `profile` wearing the skin and the cape of the account, for what the
/// profile lacks: the tab list of an offline server (or of an offline
/// account) carries no `textures` property, and the account has a skin at
/// Mojang all the same.
///
/// MCP `SkinManager.loadProfileTextures` asks the account only when the
/// textures of the tab list came out empty, that is neither a skin nor a cape
/// (`map.isEmpty()`), and then takes what the account has. A server that sent
/// either one decides, so a server skin without a cape stays without a cape.
/// `None` when the profile already has a texture, or the account has none.
#[must_use]
pub fn with_account_textures(
    profile: &PlayerProfile,
    account: &ProfileTextures,
) -> Option<PlayerProfile> {
    if profile.skin_url.is_some() || profile.cape_url.is_some() {
        return None;
    }
    if account.skin.is_none() && account.cape.is_none() {
        return None;
    }
    let mut updated = profile.clone();
    if let Some(url) = &account.skin {
        updated.skin_url = Some(url.as_str().into());
        updated.slim = Some(account.slim);
    }
    if let Some(url) = &account.cape {
        updated.cape_url = Some(url.as_str().into());
    }
    Some(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `{"textures":{"SKIN":{"url":"http://textures.minecraft.net/texture/ab",
    /// "metadata":{"model":"slim"}}}}` in base64.
    const SLIM_SKIN: &str = "eyJ0ZXh0dXJlcyI6eyJTS0lOIjp7InVybCI6Imh0dHA6Ly90ZXh0dXJlcy5taW5lY3JhZnQubmV0L3RleHR1cmUvYWIiLCJtZXRhZGF0YSI6eyJtb2RlbCI6InNsaW0ifX19fQ";

    fn property(name: &str, value: &str) -> PlayerProperty {
        PlayerProperty {
            name: name.to_owned(),
            value: value.to_owned(),
            signature: None,
        }
    }

    #[test]
    fn the_textures_property_gives_the_skin_and_the_arm_model() {
        let profile = profile_from_properties("Steve", &[property("textures", SLIM_SKIN)]);
        assert_eq!(&*profile.name, "Steve");
        assert_eq!(
            profile.skin_url.as_deref(),
            Some("http://textures.minecraft.net/texture/ab")
        );
        assert_eq!(profile.slim, Some(true));
        assert_eq!(profile.cape_url, None);
    }

    fn account(skin: Option<&str>, slim: bool, cape: Option<&str>) -> ProfileTextures {
        ProfileTextures {
            skin: skin.map(str::to_owned),
            slim,
            cape: cape.map(str::to_owned),
        }
    }

    #[test]
    fn an_offline_profile_wears_the_skin_and_cape_of_the_account() {
        // The tab list of an offline server carries no textures.
        let listed = profile_from_properties("Steve", &[]);
        let worn = with_account_textures(
            &listed,
            &account(
                Some("http://textures.minecraft.net/texture/ab"),
                true,
                Some("http://textures.minecraft.net/texture/cd"),
            ),
        )
        .expect("the account fills what is missing");
        assert_eq!(
            worn.skin_url.as_deref(),
            Some("http://textures.minecraft.net/texture/ab")
        );
        assert_eq!(worn.slim, Some(true), "the arm model follows the skin");
        assert_eq!(
            worn.cape_url.as_deref(),
            Some("http://textures.minecraft.net/texture/cd")
        );
        assert_eq!(&*worn.name, "Steve");
    }

    #[test]
    fn a_server_that_sent_any_texture_decides_and_the_account_is_not_asked() {
        // `SkinManager.loadProfileTextures`: the account only fills an empty map.
        let everything = account(
            Some("http://textures.minecraft.net/texture/zz"),
            false,
            Some("http://textures.minecraft.net/texture/cd"),
        );
        // A skin and no cape: the skin stays, and no cape is added either.
        let skin_only = profile_from_properties("Steve", &[property("textures", SLIM_SKIN)]);
        assert_eq!(with_account_textures(&skin_only, &everything), None);
        // A cape and no skin: the player keeps the default skin of their UUID.
        let mut cape_only = PlayerProfile::named("Steve");
        cape_only.cape_url = Some("http://textures.minecraft.net/texture/ee".into());
        assert_eq!(with_account_textures(&cape_only, &everything), None);
    }

    #[test]
    fn an_account_with_only_a_cape_gives_only_the_cape() {
        let listed = profile_from_properties("Steve", &[]);
        let worn = with_account_textures(
            &listed,
            &account(
                None,
                false,
                Some("http://textures.minecraft.net/texture/cd"),
            ),
        )
        .expect("a cape is something");
        assert_eq!(worn.skin_url, None, "no skin: the default skin stays");
        assert_eq!(worn.slim, None);
        assert!(worn.cape_url.is_some());
    }

    #[test]
    fn an_account_without_a_skin_changes_nothing() {
        let listed = profile_from_properties("Steve", &[]);
        assert_eq!(
            with_account_textures(&listed, &account(None, false, None)),
            None
        );
    }

    #[test]
    fn a_profile_without_textures_keeps_the_default_skin() {
        let profile = profile_from_properties("Steve", &[]);
        assert_eq!(profile.slim, None);
        assert_eq!(profile.skin_url, None);
    }

    #[test]
    fn a_broken_textures_property_is_ignored() {
        let profile = profile_from_properties("Steve", &[property("textures", "%%%")]);
        assert_eq!(profile, PlayerProfile::named("Steve"));
    }
}

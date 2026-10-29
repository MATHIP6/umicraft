// SPDX-License-Identifier: MIT OR Apache-2.0

//! Game profiles of remote players, from the tab list.
//!
//! The server sends the skin and cape of a player as the signed `textures`
//! property of its `Player List Item` entry (MCP `NetworkPlayerInfo`,
//! `SkinManager.loadProfileTextures`). The client reads the URLs from it; it
//! does not check the signature, as vanilla does not either.

use umicraft_assets::profile_textures::parse_textures_property;
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
    let textures = properties
        .iter()
        .find(|property| property.name == TEXTURES_PROPERTY)
        .and_then(|property| parse_textures_property(&property.value).ok());
    if let Some(textures) = textures {
        profile.slim = textures.skin.is_some().then_some(textures.slim);
        profile.skin_url = textures.skin.as_deref().map(Into::into);
        profile.cape_url = textures.cape.as_deref().map(Into::into);
    }
    profile
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

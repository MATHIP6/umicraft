// SPDX-License-Identifier: MIT OR Apache-2.0

//! The cape services and the request each one needs.
//!
//! URL shapes come from the behaviour of the services, probed on 2026-11-18
//! (`docs/sources.md`, section "Capes").

/// A third-party cape service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapeProvider {
    /// OptiFine capes, looked up by player name over plain HTTP.
    OptiFine,
    /// LabyMod capes, looked up by UUID.
    LabyMod,
    /// MinecraftCapes, a JSON profile by UUID.
    MinecraftCapes,
    /// Cosmetica, a JSON cloak description by UUID.
    Cosmetica,
}

/// The player a cape is wanted for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapeOwner {
    /// The profile UUID as a 128-bit number.
    pub uuid: u128,
    /// The player name.
    pub name: String,
}

/// How a response body must be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseKind {
    /// The body is the cape PNG.
    Png,
    /// The body is a Cosmetica cloak description.
    CosmeticaJson,
    /// The body is a MinecraftCapes profile.
    MinecraftCapesJson,
}

/// A request to one service: where to go and how to read the answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapeRequest {
    /// The absolute URL to fetch.
    pub url: String,
    /// How the body is to be read.
    pub kind: ResponseKind,
}

impl CapeOwner {
    /// True for a UUID of version 4, which online-mode accounts have.
    ///
    /// Version 2 (NPCs) and version 3 (offline-mode players) never belong to
    /// a real account, so they are not sent to any service.
    #[must_use]
    pub fn is_lookup_allowed(&self) -> bool {
        (self.uuid >> 76) & 0xf == 4
    }

    /// True when the name is 1 to 16 characters of `[A-Za-z0-9_]`, so it is
    /// safe in a URL path without any encoding.
    ///
    /// [`CapeProvider::request`] does not check this (its signature is
    /// fixed by ADR-0081); the resolver skips OptiFine, the only service
    /// that looks up by name, when it is false.
    #[must_use]
    pub fn has_valid_name(&self) -> bool {
        (1..=16).contains(&self.name.len())
            && self
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    /// The UUID as 32 lowercase hex digits.
    fn simple_uuid(&self) -> String {
        format!("{:032x}", self.uuid)
    }

    /// The UUID as five lowercase hex groups joined by dashes.
    fn dashed_uuid(&self) -> String {
        let simple = self.simple_uuid();
        format!(
            "{}-{}-{}-{}-{}",
            &simple[..8],
            &simple[8..12],
            &simple[12..16],
            &simple[16..20],
            &simple[20..]
        )
    }
}

impl CapeProvider {
    /// Every provider, in the order a player is looked up.
    pub const ALL: [Self; 4] = [
        Self::OptiFine,
        Self::LabyMod,
        Self::MinecraftCapes,
        Self::Cosmetica,
    ];

    /// The key of the provider in the mod settings.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::OptiFine => "optifine",
            Self::LabyMod => "labymod",
            Self::MinecraftCapes => "minecraftcapes",
            Self::Cosmetica => "cosmetica",
        }
    }

    /// The provider with this settings key, `None` for any other text
    /// (`minecraft`, the own-cape default, is not a provider).
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|provider| provider.key() == key)
    }

    /// The request that asks this service for the cape of `owner`.
    ///
    /// The OptiFine URL contains the name unchanged: check
    /// [`CapeOwner::has_valid_name`] first.
    ///
    /// # Examples
    ///
    /// ```
    /// use umicraft_assets::capes::{CapeOwner, CapeProvider};
    ///
    /// let owner = CapeOwner { uuid: 0x1234_5678_9abc_4def_8012_3456_789a_bcde, name: "Steve".into() };
    /// assert_eq!(
    ///     CapeProvider::OptiFine.request(&owner).url,
    ///     "http://s.optifine.net/capes/Steve.png"
    /// );
    /// ```
    #[must_use]
    pub fn request(self, owner: &CapeOwner) -> CapeRequest {
        let (url, kind) = match self {
            Self::OptiFine => (
                format!("http://s.optifine.net/capes/{}.png", owner.name),
                ResponseKind::Png,
            ),
            Self::LabyMod => (
                format!("https://dl.labymod.net/capes/{}", owner.dashed_uuid()),
                ResponseKind::Png,
            ),
            Self::Cosmetica => (
                format!(
                    "https://api.cosmetica.cc/users/{}/cape",
                    owner.dashed_uuid()
                ),
                ResponseKind::CosmeticaJson,
            ),
            Self::MinecraftCapes => (
                format!(
                    "https://api.minecraftcapes.net/profile/{}",
                    owner.simple_uuid()
                ),
                ResponseKind::MinecraftCapesJson,
            ),
        };
        CapeRequest { url, kind }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner(uuid: u128, name: &str) -> CapeOwner {
        CapeOwner {
            uuid,
            name: name.to_owned(),
        }
    }

    const V4: u128 = 0x1234_5678_9abc_4def_8012_3456_789a_bcde;

    #[test]
    fn all_is_the_lookup_order() {
        assert_eq!(
            CapeProvider::ALL.map(CapeProvider::key),
            ["optifine", "labymod", "minecraftcapes", "cosmetica"]
        );
    }

    #[test]
    fn keys_round_trip_and_unknown_keys_are_none() {
        for provider in CapeProvider::ALL {
            assert_eq!(CapeProvider::from_key(provider.key()), Some(provider));
        }
        assert_eq!(CapeProvider::from_key("minecraft"), None);
        assert_eq!(CapeProvider::from_key(""), None);
    }

    #[test]
    fn urls_follow_each_service() {
        let o = owner(V4, "Steve_1");
        let url = |p: CapeProvider| p.request(&o);
        assert_eq!(
            url(CapeProvider::OptiFine),
            CapeRequest {
                url: "http://s.optifine.net/capes/Steve_1.png".into(),
                kind: ResponseKind::Png
            }
        );
        assert_eq!(
            url(CapeProvider::LabyMod),
            CapeRequest {
                url: "https://dl.labymod.net/capes/12345678-9abc-4def-8012-3456789abcde".into(),
                kind: ResponseKind::Png
            }
        );
        assert_eq!(
            url(CapeProvider::Cosmetica),
            CapeRequest {
                url: "https://api.cosmetica.cc/users/12345678-9abc-4def-8012-3456789abcde/cape"
                    .into(),
                kind: ResponseKind::CosmeticaJson
            }
        );
        assert_eq!(
            url(CapeProvider::MinecraftCapes),
            CapeRequest {
                url: "https://api.minecraftcapes.net/profile/123456789abc4def80123456789abcde"
                    .into(),
                kind: ResponseKind::MinecraftCapesJson
            }
        );
    }

    #[test]
    fn leading_zeros_are_kept_in_uuids() {
        let o = owner(0x4 << 76, "a");
        assert_eq!(
            CapeProvider::LabyMod.request(&o).url,
            "https://dl.labymod.net/capes/00000000-0000-4000-0000-000000000000"
        );
    }

    #[test]
    fn only_version_four_uuids_are_looked_up() {
        for (version, allowed) in [
            (1u128, false),
            (2, false),
            (3, false),
            (4, true),
            (5, false),
        ] {
            let uuid = (version << 76) | 0x8000_0000_0000_0000;
            assert_eq!(owner(uuid, "a").is_lookup_allowed(), allowed, "{version}");
        }
    }

    #[test]
    fn names_are_checked_for_url_safety() {
        assert!(owner(V4, "Notch").has_valid_name());
        assert!(owner(V4, "a_B_9").has_valid_name());
        assert!(owner(V4, "0123456789abcdef").has_valid_name());
        assert!(!owner(V4, "").has_valid_name());
        assert!(!owner(V4, "0123456789abcdefg").has_valid_name());
        assert!(!owner(V4, "a/b").has_valid_name());
        assert!(!owner(V4, "a b").has_valid_name());
        assert!(!owner(V4, "é").has_valid_name());
    }
}

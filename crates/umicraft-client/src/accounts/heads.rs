// SPDX-License-Identifier: MIT OR Apache-2.0

//! The heads of the account rows: the 8x8 face of each player's skin.
//!
//! A Microsoft account shows its own skin, downloaded once in the background
//! (the texture worker keeps a disk cache); until it arrives, and for an offline
//! account, the head is the default skin of the UUID: Steve or Alex, chosen the
//! way vanilla does (`DefaultPlayerSkin`, MCP `isSlimSkin`).

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use umicraft_assets::profile_textures::{skin_from_image, TextureFetcher};
use umicraft_assets::skin::{procedural_fallback_skin, SkinModel};
use umicraft_auth::Account;

use crate::entity_render::{head_face, load_or_fallback};

const STEVE_PATH: &str = "assets/minecraft/textures/entity/steve.png";
const ALEX_PATH: &str = "assets/minecraft/textures/entity/alex.png";

/// The faces of the two default skins; `None` when a skin is unusable (the
/// row then shows its flat placeholder).
struct Defaults {
    steve: Option<Arc<[u8]>>,
    alex: Option<Arc<[u8]>>,
}

impl Defaults {
    /// The faces of the game's own `steve.png` and `alex.png`, or of the
    /// procedural stand-ins when the assets are not installed.
    fn from_assets() -> Self {
        let jar = crate::official::find_verified_jar().ok();
        let face = |path: &str, model| {
            let (rgba, _, _) = load_or_fallback(jar.as_ref(), path, || {
                (procedural_fallback_skin(model), 64, 64)
            });
            head_face(&rgba, true)
        };
        Self {
            steve: face(STEVE_PATH, SkinModel::Classic),
            alex: face(ALEX_PATH, SkinModel::Slim),
        }
    }

    /// The faces of the procedural stand-ins only: tests, with no disk access.
    fn procedural() -> Self {
        let face = |model| head_face(&procedural_fallback_skin(model), true);
        Self {
            steve: face(SkinModel::Classic),
            alex: face(SkinModel::Slim),
        }
    }
}

/// The 16 bytes of a 32-digit hexadecimal UUID.
fn uuid_bytes(hex: &str) -> Option<[u8; 16]> {
    let mut bytes = [0; 16];
    if hex.len() != 32 || !hex.is_ascii() {
        return None;
    }
    for (byte, pair) in bytes.iter_mut().zip(hex.as_bytes().chunks_exact(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(bytes)
}

/// The faces to show, and the downloads that provide them.
pub struct Heads {
    defaults: OnceCell<Defaults>,
    /// `None` in tests and demos: no download is ever started.
    fetcher: Option<TextureFetcher>,
    /// The face of every skin that finished downloading.
    ready: HashMap<String, Arc<[u8]>>,
    requested: HashSet<String>,
}

impl Heads {
    /// Heads for the real client: default skins read when first needed, skins
    /// downloaded in the background and cached in the data directory.
    #[must_use]
    pub fn with_downloads() -> Self {
        let cache = umicraft_assets::cache::AssetCache::in_data_dir()
            .ok()
            .map(|cache| cache.root().join("skins"));
        Self {
            defaults: OnceCell::new(),
            fetcher: Some(TextureFetcher::spawn(cache)),
            ready: HashMap::new(),
            requested: HashSet::new(),
        }
    }

    /// Heads that touch neither the disk nor the network.
    #[must_use]
    pub fn without_downloads() -> Self {
        let defaults = OnceCell::new();
        let _ = defaults.set(Defaults::procedural());
        Self {
            defaults,
            fetcher: None,
            ready: HashMap::new(),
            requested: HashSet::new(),
        }
    }

    /// The face of `account`: its own skin once downloaded (asking for it on
    /// the first call), else the default skin of its UUID.
    pub fn face_for(&mut self, account: &Account) -> Option<Arc<[u8]>> {
        if let Some(url) = account.skin_url.as_deref() {
            if let Some(face) = self.ready.get(url) {
                return Some(face.clone());
            }
            if let (Some(fetcher), true) = (&self.fetcher, self.requested.insert(url.to_owned())) {
                fetcher.request(url);
            }
        }
        self.default_face(&account.uuid)
    }

    /// Steve or Alex for `uuid`; Steve for a UUID that cannot be read.
    fn default_face(&self, uuid: &str) -> Option<Arc<[u8]>> {
        let defaults = self.defaults.get_or_init(Defaults::from_assets);
        let slim =
            uuid_bytes(uuid).is_some_and(|bytes| SkinModel::default_for_uuid(bytes).is_slim());
        if slim {
            &defaults.alex
        } else {
            &defaults.steve
        }
        .clone()
    }

    /// Takes the downloads that finished; true when a new face is ready and the
    /// rows must be rebuilt.
    pub fn poll(&mut self) -> bool {
        let Some(fetcher) = &self.fetcher else {
            return false;
        };
        let finished = fetcher.poll();
        let mut changed = false;
        for fetched in finished {
            changed |= self.accept(fetched.url, fetched.image);
        }
        changed
    }

    /// Keeps the face of a downloaded image; a failed or unusable one leaves
    /// the account on its default skin. True when a face was added.
    fn accept(&mut self, url: String, image: Result<(Vec<u8>, u32, u32), String>) -> bool {
        let face = image
            .inspect_err(|error| tracing::warn!(%url, %error, "skin download failed"))
            .ok()
            .and_then(|(rgba, width, height)| {
                skin_from_image(&rgba, width, height)
                    .inspect_err(|error| tracing::warn!(%url, %error, "unusable skin image"))
                    .ok()
            })
            .and_then(|skin| head_face(&skin, true));
        face.map(|face| self.ready.insert(url, face)).is_some()
    }
}

#[cfg(test)]
mod tests {
    use umicraft_auth::AccountKind;

    use super::*;

    /// UUID parities worked out by hand from `UUID.hashCode()`: the first folds to
    /// an even hash (Steve), the second differs by one bit of the most
    /// significant half and folds to an odd one (Alex).
    const STEVE_UUID: &str = "069a79f444e94726a5befca90e38aaf5";
    const ALEX_UUID: &str = "069a79f444e94727a5befca90e38aaf5";

    fn account(uuid: &str, skin_url: Option<&str>) -> Account {
        Account {
            kind: AccountKind::Microsoft,
            username: "P".into(),
            uuid: uuid.into(),
            skin_url: skin_url.map(Into::into),
        }
    }

    /// A heads set whose defaults are told apart by their first byte.
    fn heads() -> Heads {
        let heads = Heads::without_downloads();
        let marked = |first: u8| {
            let mut face = vec![0; 256];
            face[0] = first;
            Some(Arc::<[u8]>::from(face))
        };
        let defaults = Defaults {
            steve: marked(1),
            alex: marked(2),
        };
        Heads {
            defaults: {
                let cell = OnceCell::new();
                let _ = cell.set(defaults);
                cell
            },
            ..heads
        }
    }

    fn first_byte(face: Option<Arc<[u8]>>) -> u8 {
        face.expect("a face")[0]
    }

    fn red_skin() -> (Vec<u8>, u32, u32) {
        let mut skin = vec![0; 64 * 64 * 4];
        for y in 8..16 {
            for x in 8..16 {
                skin[(y * 64 + x) * 4..][..4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        (skin, 64, 64)
    }

    #[test]
    fn the_default_skin_follows_the_uuid_like_vanilla() {
        let mut heads = heads();
        assert_eq!(first_byte(heads.face_for(&account(STEVE_UUID, None))), 1);
        assert_eq!(first_byte(heads.face_for(&account(ALEX_UUID, None))), 2);
    }

    #[test]
    fn an_unreadable_uuid_gets_steve() {
        let mut heads = heads();
        assert_eq!(first_byte(heads.face_for(&account("not-a-uuid", None))), 1);
    }

    #[test]
    fn a_downloaded_skin_replaces_the_default_face() {
        let mut heads = heads();
        let url = "http://textures.minecraft.net/texture/abc";
        let owner = account(ALEX_UUID, Some(url));
        assert_eq!(first_byte(heads.face_for(&owner)), 2, "default until ready");
        assert!(heads.accept(url.into(), Ok(red_skin())));
        let face = heads.face_for(&owner).expect("a face");
        assert_eq!(&face[..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn a_failed_download_keeps_the_default_face() {
        let mut heads = heads();
        let url = "http://textures.minecraft.net/texture/abc";
        let owner = account(STEVE_UUID, Some(url));
        assert!(!heads.accept(url.into(), Err("boom".into())));
        assert!(!heads.accept(url.into(), Ok((vec![0; 16], 2, 2))));
        assert_eq!(first_byte(heads.face_for(&owner)), 1);
    }

    #[test]
    fn the_stand_in_skins_give_a_face_without_assets() {
        let mut heads = Heads::without_downloads();
        let face = heads.face_for(&account(STEVE_UUID, None)).expect("a face");
        assert_eq!(face.len(), 256);
    }
}

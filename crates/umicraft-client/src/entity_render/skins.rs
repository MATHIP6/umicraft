// SPDX-License-Identifier: MIT OR Apache-2.0

//! The skins and capes of players: which download is wanted, which is
//! ready, and the GPU textures they become.
//!
//! The downloads run on the worker of [`TextureFetcher`]; the render thread
//! only asks and collects, so a slow server never stalls a frame. Until a
//! skin is ready the player wears the default skin of their UUID.

use std::collections::HashMap;
use std::sync::Arc;

use umicraft_assets::cache::AssetCache;
use umicraft_assets::profile_textures::{check_cape_image, skin_from_image, TextureFetcher};
use umicraft_model::entity::TextureSlot;
use umicraft_render::{EntityTexture, Renderer};

/// What a mesher may ask about downloaded textures.
pub(crate) trait SkinLookup {
    /// The index of the skin at `url`, once it is ready.
    fn skin(&self, url: &str) -> Option<u32>;
    /// The index of the cape at `url`, once it is ready.
    fn cape(&self, url: &str) -> Option<u32>;
}

/// No skin is ever ready: everybody wears the default skin.
pub(crate) struct NoSkins;

impl SkinLookup for NoSkins {
    fn skin(&self, _url: &str) -> Option<u32> {
        None
    }

    fn cape(&self, _url: &str) -> Option<u32> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Loading(SkinKind),
    Ready(u32),
    Failed,
}

/// The downloads of skins and capes and their state.
pub(super) struct SkinSet {
    fetcher: TextureFetcher,
    states: HashMap<String, State>,
    /// The face of every skin that is ready, for the heads of the tab list: bare,
    /// and with the hat layer over it.
    faces: HashMap<String, [Arc<[u8]>; 2]>,
    next_index: u32,
}

/// The 8x8 face of a 64x64 RGBA `skin`, 256 bytes, with the hat layer over it
/// when `hat` is set (`GuiPlayerTabOverlay`: texels (8, 8) and (40, 8)).
pub(crate) fn head_face(skin: &[u8], hat: bool) -> Option<Arc<[u8]>> {
    const SIDE: usize = 64;
    if skin.len() != SIDE * SIDE * 4 {
        return None;
    }
    let at = |x: usize, y: usize| &skin[(y * SIDE + x) * 4..][..4];
    let mut face = Vec::with_capacity(8 * 8 * 4);
    for y in 0..8 {
        for x in 0..8 {
            let (base, hat_pixel) = (at(8 + x, 8 + y), at(40 + x, 8 + y));
            let cover = if hat { u32::from(hat_pixel[3]) } else { 0 };
            for channel in 0..3 {
                let mixed = u32::from(hat_pixel[channel]) * cover
                    + u32::from(base[channel]) * (255 - cover);
                face.push((mixed / 255) as u8);
            }
            face.push(255);
        }
    }
    Some(face.into())
}

impl SkinSet {
    /// Starts the downloader with the on-disk cache of the data directory.
    pub(super) fn new() -> Self {
        let cache = AssetCache::in_data_dir()
            .ok()
            .map(|cache| cache.root().join("skins"));
        Self {
            fetcher: TextureFetcher::spawn(cache),
            states: HashMap::new(),
            faces: HashMap::new(),
            next_index: 0,
        }
    }

    /// The face of the skin at `url`, asking for the download on the first call.
    pub(super) fn face(&mut self, url: &str, hat: bool) -> Option<Arc<[u8]>> {
        self.lookup(url, SkinKind::Skin);
        self.faces
            .get(url)
            .map(|faces| faces[usize::from(hat)].clone())
    }

    /// The index of the texture at `url`, asking for it on the first call.
    pub(super) fn lookup(&mut self, url: &str, kind: SkinKind) -> Option<u32> {
        match self.states.get(url) {
            Some(State::Ready(index)) => Some(*index),
            Some(_) => None,
            None => {
                self.states.insert(url.to_owned(), State::Loading(kind));
                tracing::debug!(url, ?kind, "requesting texture");
                self.fetcher.request(url);
                None
            }
        }
    }

    /// Turns the downloads that finished into GPU textures.
    pub(super) fn collect(
        &mut self,
        renderer: &Renderer,
        textures: &mut HashMap<TextureSlot, EntityTexture>,
    ) {
        for fetched in self.fetcher.poll() {
            let Some(State::Loading(kind)) = self.states.get(&fetched.url).copied() else {
                continue;
            };
            let upload = fetched
                .image
                .inspect_err(
                    |error| tracing::warn!(url = %fetched.url, %error, "texture download failed"),
                )
                .ok()
                .and_then(|(rgba, width, height)| match kind {
                    SkinKind::Skin => skin_from_image(&rgba, width, height)
                        .inspect_err(|error| tracing::warn!(url = %fetched.url, %error, "unusable skin image"))
                        .ok()
                        .map(|skin| (skin, 64, 64)),
                    SkinKind::Cape => check_cape_image(width, height)
                        .inspect_err(|error| tracing::warn!(url = %fetched.url, %error, "unusable cape image"))
                        .ok()
                        .map(|()| (rgba, width, height)),
                });
            let state = match upload {
                Some((rgba, width, height)) => {
                    let index = self.next_index;
                    self.next_index += 1;
                    if kind == SkinKind::Skin {
                        if let (Some(bare), Some(hatted)) =
                            (head_face(&rgba, false), head_face(&rgba, true))
                        {
                            self.faces.insert(fetched.url.clone(), [bare, hatted]);
                        }
                    }
                    let slot = match kind {
                        SkinKind::Skin => TextureSlot::Skin(index),
                        SkinKind::Cape => TextureSlot::Cape(index),
                    };
                    textures.insert(slot, renderer.create_entity_texture(&rgba, width, height));
                    tracing::debug!(url = %fetched.url, index, "texture ready");
                    State::Ready(index)
                }
                None => State::Failed,
            };
            self.states.insert(fetched.url, state);
        }
    }
}

/// Which texture of a profile is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SkinKind {
    /// The skin.
    Skin,
    /// The cape.
    Cape,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 64x64 skin with a red face, a transparent hat and one opaque blue hat
    /// pixel at the top-left of the hat layer.
    fn skin() -> Vec<u8> {
        let mut skin = vec![0; 64 * 64 * 4];
        let mut set = |x: usize, y: usize, rgba: [u8; 4]| {
            skin[(y * 64 + x) * 4..][..4].copy_from_slice(&rgba);
        };
        for y in 8..16 {
            for x in 8..16 {
                set(x, y, [200, 0, 0, 255]);
            }
        }
        set(40, 8, [0, 0, 255, 255]);
        skin
    }

    #[test]
    fn the_face_is_the_front_of_the_head_with_the_hat_over_it() {
        let face = head_face(&skin(), true).expect("a 64x64 skin");
        assert_eq!(face.len(), 256);
        assert_eq!(&face[..4], &[0, 0, 255, 255], "the hat covers the corner");
        assert_eq!(&face[4..8], &[200, 0, 0, 255], "the face shows elsewhere");
        let bare = head_face(&skin(), false).expect("a 64x64 skin");
        assert_eq!(&bare[..4], &[200, 0, 0, 255], "no hat, no cover");
    }

    #[test]
    fn an_image_that_is_not_a_skin_has_no_face() {
        assert!(head_face(&[0; 16], true).is_none());
    }
}

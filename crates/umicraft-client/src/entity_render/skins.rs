// SPDX-License-Identifier: MIT OR Apache-2.0

//! The skins and capes of players: which download is wanted, which is
//! ready, and the GPU textures they become.
//!
//! The downloads run on the worker of [`TextureFetcher`]; the render thread
//! only asks and collects, so a slow server never stalls a frame. Until a
//! skin is ready the player wears the default skin of their UUID.

use std::collections::HashMap;

use umicraft_assets::cache::AssetCache;
use umicraft_assets::profile_textures::{check_cape_image, skin_from_image, TextureFetcher};
use umicraft_model::entity::TextureSlot;
use umicraft_render::{EntityTexture, Renderer};

/// What a mesher may ask about downloaded textures.
pub(super) trait SkinLookup {
    /// The index of the skin at `url`, once it is ready.
    fn skin(&self, url: &str) -> Option<u32>;
    /// The index of the cape at `url`, once it is ready.
    fn cape(&self, url: &str) -> Option<u32>;
}

/// No skin is ever ready: everybody wears the default skin.
pub(super) struct NoSkins;

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
    next_index: u32,
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
            next_index: 0,
        }
    }

    /// The index of the texture at `url`, asking for it on the first call.
    pub(super) fn lookup(&mut self, url: &str, kind: SkinKind) -> Option<u32> {
        match self.states.get(url) {
            Some(State::Ready(index)) => Some(*index),
            Some(_) => None,
            None => {
                self.states.insert(url.to_owned(), State::Loading(kind));
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
                .ok()
                .and_then(|(rgba, width, height)| match kind {
                    SkinKind::Skin => skin_from_image(&rgba, width, height)
                        .ok()
                        .map(|skin| (skin, 64, 64)),
                    SkinKind::Cape => check_cape_image(width, height)
                        .ok()
                        .map(|()| (rgba, width, height)),
                });
            let state = match upload {
                Some((rgba, width, height)) => {
                    let index = self.next_index;
                    self.next_index += 1;
                    let slot = match kind {
                        SkinKind::Skin => TextureSlot::Skin(index),
                        SkinKind::Cape => TextureSlot::Cape(index),
                    };
                    textures.insert(slot, renderer.create_entity_texture(&rgba, width, height));
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

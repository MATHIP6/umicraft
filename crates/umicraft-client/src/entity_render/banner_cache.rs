// SPDX-License-Identifier: MIT OR Apache-2.0

//! The textures of banner designs, composed on first use.
//!
//! The number of designs is unbounded, so they cannot be in the static table
//! of `specs`. Like the horse coats they are made when a frame first draws
//! them, from the user's own client jar. At most [`MAX_DESIGNS`] stay (the
//! reference keeps 256 and drops the ones unused for a minute); when the cache
//! is full the design unused for longest makes room, and a design that finds
//! no room is not drawn that frame. The cache lives and dies with the
//! [`EntityTextures`](super::EntityTextures) that owns it, so a resource
//! reload, which rebuilds those, empties it.

use std::collections::HashMap;

use umicraft_assets::client_jar::ClientJar;
use umicraft_assets::skin::decode_png_bytes;
use umicraft_model::entity::TextureSlot;
use umicraft_render::{EntityTexture, Renderer};

use crate::block_entity_render::{compose_banner, BannerDesigns, Image};

/// Most banner textures kept (`DESIGNS.size() >= 256` in the reference).
const MAX_DESIGNS: usize = 256;

/// The banner textures present and the frame each was last wanted.
#[derive(Debug, Default)]
pub(super) struct BannerCache {
    frame: u64,
    last_used: HashMap<TextureSlot, u64>,
}

impl BannerCache {
    /// Makes sure every design in `designs` has its texture in `textures`,
    /// composing the missing ones.
    pub(super) fn ensure(
        &mut self,
        designs: &BannerDesigns,
        textures: &mut HashMap<TextureSlot, EntityTexture>,
        jar: Option<&ClientJar>,
        renderer: &Renderer,
    ) {
        self.frame += 1;
        for slot in designs.keys() {
            if let Some(stamp) = self.last_used.get_mut(slot) {
                *stamp = self.frame;
            }
        }
        for (slot, design) in designs {
            if self.last_used.contains_key(slot) || !self.make_room(textures) {
                continue;
            }
            let image = compose_banner(design, |path| read_entity_image(jar, path));
            let texture = renderer.create_entity_texture(&image.rgba, image.width, image.height);
            textures.insert(*slot, texture);
            self.last_used.insert(*slot, self.frame);
        }
    }

    /// Drops the design unused for longest if the cache is full. `false` when
    /// every design is in use this frame, so there is no room.
    fn make_room(&mut self, textures: &mut HashMap<TextureSlot, EntityTexture>) -> bool {
        if self.last_used.len() < MAX_DESIGNS {
            return true;
        }
        let oldest = self
            .last_used
            .iter()
            .filter(|(_, stamp)| **stamp != self.frame)
            .min_by_key(|(_, stamp)| **stamp)
            .map(|(slot, _)| *slot);
        let Some(slot) = oldest else {
            return false;
        };
        self.last_used.remove(&slot);
        textures.remove(&slot);
        true
    }
}

/// The PNG at `textures/entity/<path>.png` of the jar, decoded.
fn read_entity_image(jar: Option<&ClientJar>, path: &str) -> Option<Image> {
    let bytes = jar?
        .read_entry(&format!("assets/minecraft/textures/entity/{path}.png"))
        .ok()?;
    let (rgba, width, height) = decode_png_bytes(&bytes).ok()?;
    Some(Image {
        rgba,
        width,
        height,
    })
}

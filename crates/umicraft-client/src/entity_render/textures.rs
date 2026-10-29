// SPDX-License-Identifier: MIT OR Apache-2.0

//! Entity texture registry: the jar paths, the procedural fallbacks and the
//! GPU textures keyed by [`TextureSlot`].
//!
//! The table lives in `specs`; horse coats are composed on first use.

use std::cell::RefCell;
use std::collections::HashMap;

use glam::Mat4;
use umicraft_assets::client_jar::ClientJar;
use umicraft_assets::skin::decode_png_bytes;
use umicraft_game::entity::EntityRenderSnapshot;
use umicraft_game::hand::HandPose;
use umicraft_model::entity::TextureSlot;
use umicraft_render::{EntityTexture, Renderer};

use super::hand::hand_vertices;
use super::horse_coat::horse_coat;
use super::mesh_entities;
use super::specs::texture_specs;

/// Loads official textures from the jar if available, or falls back to procedural assets.
pub fn load_or_fallback<F>(
    jar: Option<&ClientJar>,
    jar_path: &str,
    fallback_fn: F,
) -> (Vec<u8>, u32, u32)
where
    F: FnOnce() -> (Vec<u8>, u32, u32),
{
    if let Some(jar) = jar {
        if let Ok(bytes) = jar.read_entry(jar_path) {
            if let Ok(decoded) = decode_png_bytes(&bytes) {
                return decoded;
            }
        }
    }
    fallback_fn()
}

/// Procedural dirt texture (16×16) used as a clean-room fallback for dropped blocks.
#[must_use]
pub fn procedural_dirt_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 16 * 16 * 4];
    for y in 0..16 {
        for x in 0..16 {
            let idx = (y * 16 + x) * 4;
            let hash = ((x * 37 + y * 17) ^ ((x << 3) + y)) % 256;
            let shade = (hash % 30) as u8 + 110;
            rgba[idx] = (f32::from(shade) * 0.65) as u8;
            rgba[idx + 1] = (f32::from(shade) * 0.45) as u8;
            rgba[idx + 2] = (f32::from(shade) * 0.28) as u8;
            rgba[idx + 3] = 255;
        }
    }
    rgba
}

// ---------------------------------------------------------------------------
// Texture registry
// ---------------------------------------------------------------------------

/// GPU texture registry keyed by [`TextureSlot`].
///
/// Adding a new mob = add one entry in [`texture_specs`].
pub struct EntityTextures {
    /// The textures of the table, and the horse coats seen so far.
    textures: RefCell<HashMap<TextureSlot, EntityTexture>>,
    /// The jar the coats are composed from.
    jar: Option<ClientJar>,
}

impl EntityTextures {
    /// Loads all entity textures from the verified jar or procedural fallbacks.
    #[must_use]
    pub fn load(renderer: &Renderer) -> Self {
        let jar = crate::official::find_verified_jar().ok();
        let mut textures = HashMap::new();
        for spec in texture_specs() {
            let (rgba, w, h) = load_or_fallback(jar.as_ref(), spec.jar_path, spec.fallback);
            textures.insert(spec.slot, renderer.create_entity_texture(&rgba, w, h));
        }
        Self {
            textures: RefCell::new(textures),
            jar,
        }
    }

    /// Uploads the first-person hand, or hides it when `pose` shows no arm.
    ///
    /// The arm wears the default skin until the local profile is known (the
    /// same one the third-person body wears).
    pub fn upload_hand(&self, pose: &HandPose, bob: Mat4, renderer: &mut Renderer) {
        let vertices = hand_vertices(pose, bob, false);
        let textures = self.textures.borrow();
        match (vertices.is_empty(), textures.get(&TextureSlot::Steve)) {
            (false, Some(skin)) => renderer.set_hand(&vertices, skin.bind_group()),
            _ => renderer.clear_hand(),
        }
    }

    /// Meshes all visible entities and uploads them to the GPU renderer.
    pub fn mesh_and_upload(&self, snapshots: &[EntityRenderSnapshot], renderer: &mut Renderer) {
        let batches = mesh_entities(snapshots);
        self.compose_missing_coats(batches.keys(), renderer);
        let textures = self.textures.borrow();
        let pairs: Vec<_> = batches
            .iter()
            .filter_map(|(slot, verts)| {
                if verts.is_empty() {
                    return None;
                }
                textures
                    .get(slot)
                    .map(|tex| (verts.as_slice(), tex.bind_group()))
            })
            .collect();
        renderer.upload_entities(&pairs);
    }

    /// Creates the texture of every horse coat in `slots` that is not there yet.
    fn compose_missing_coats<'a>(
        &self,
        slots: impl Iterator<Item = &'a TextureSlot>,
        renderer: &Renderer,
    ) {
        for slot in slots {
            let TextureSlot::HorseCoat(variant) = *slot else {
                continue;
            };
            if self.textures.borrow().contains_key(slot) {
                continue;
            }
            let (rgba, w, h) = horse_coat(self.jar.as_ref(), variant);
            let texture = renderer.create_entity_texture(&rgba, w, h);
            self.textures.borrow_mut().insert(*slot, texture);
        }
    }
}

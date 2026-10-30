// SPDX-License-Identifier: MIT OR Apache-2.0

//! Entity texture registry: the jar paths, the procedural fallbacks and the
//! GPU textures keyed by [`TextureSlot`].
//!
//! The table lives in `specs`; horse coats are composed on first use.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use glam::Mat4;
use umicraft_assets::client_jar::ClientJar;
use umicraft_assets::skin::decode_png_bytes;
use umicraft_game::entity::EntityRenderSnapshot;
use umicraft_game::hand::HandPose;
use umicraft_model::entity::TextureSlot;
use umicraft_render::{EntityTexture, Renderer};

use super::armor::armor_texture;
use super::hand::hand_vertices;
use super::horse_coat::horse_coat;
use super::mesh_entities_with;
use super::skins::{SkinKind, SkinLookup, SkinSet};
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
    /// The skins and capes of players.
    skins: RefCell<SkinSet>,
    /// The camera yaw the flames of burning entities face.
    view_yaw: Cell<f32>,
    /// Whether the local player is a spectator, who sees invisible entities.
    viewer_is_spectator: Cell<bool>,
    /// Where the camera is, for the distance at which a shadow fades.
    camera: Cell<glam::Vec3>,
}

impl EntityTextures {
    /// Loads all entity textures from the verified jar or procedural fallbacks.
    #[must_use]
    pub fn load(renderer: &Renderer) -> Self {
        let jar = crate::official::find_verified_jar().ok();
        let mut textures = HashMap::new();
        for spec in texture_specs() {
            let (rgba, w, h) = load_or_fallback(jar.as_ref(), spec.jar_path, spec.fallback);
            // The glowing layers scroll their coordinates past the edge.
            let texture = if spec.slot.is_additive() {
                renderer.create_repeating_entity_texture(&rgba, w, h)
            } else {
                renderer.create_entity_texture(&rgba, w, h)
            };
            textures.insert(spec.slot, texture);
        }
        Self {
            textures: RefCell::new(textures),
            jar,
            skins: RefCell::new(SkinSet::new()),
            view_yaw: Cell::new(0.0),
            viewer_is_spectator: Cell::new(false),
            camera: Cell::new(glam::Vec3::ZERO),
        }
    }

    /// Sets the camera yaw (radians, `umicraft_render::Camera` convention)
    /// that the flames of burning entities face.
    pub fn set_view_yaw(&self, yaw: f32) {
        self.view_yaw.set(yaw);
    }

    /// Sets the position of the camera, which decides how far a shadow is
    /// still drawn.
    pub fn set_camera_position(&self, position: glam::Vec3) {
        self.camera.set(position);
    }

    /// Tells whether the local player is a spectator, who sees the invisible
    /// entities as ghosts (MCP `isInvisibleToPlayer`).
    pub fn set_viewer_spectator(&self, spectator: bool) {
        self.viewer_is_spectator.set(spectator);
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
        self.skins
            .borrow_mut()
            .collect(renderer, &mut self.textures.borrow_mut());
        let meshed = mesh_entities_with(
            snapshots,
            self,
            self.view_yaw.get(),
            self.viewer_is_spectator.get(),
            self.camera.get(),
        );
        self.compose_missing_coats(meshed.solid.keys(), renderer);
        let textures = self.textures.borrow();
        let solid: Vec<_> = meshed
            .solid
            .iter()
            .filter(|(_, verts)| !verts.is_empty())
            .filter_map(|(slot, verts)| {
                textures
                    .get(slot)
                    .map(|tex| (verts.as_slice(), tex.bind_group()))
            })
            .collect();
        renderer.upload_entities(&solid);
        let additive: Vec<_> = meshed
            .additive
            .iter()
            .filter(|(_, verts)| !verts.is_empty())
            .filter_map(|(slot, verts)| {
                textures
                    .get(slot)
                    .map(|tex| (verts.as_slice(), tex.bind_group()))
            })
            .collect();
        renderer.upload_additive_entities(&additive);
        let glint: Vec<_> = meshed
            .glint
            .iter()
            .filter(|(_, verts)| !verts.is_empty())
            .filter_map(|(slot, verts)| {
                textures
                    .get(slot)
                    .map(|tex| (verts.as_slice(), tex.bind_group()))
            })
            .collect();
        renderer.upload_glint_entities(&glint);
        let ghost: Vec<_> = meshed
            .ghost
            .iter()
            .filter(|(_, verts)| !verts.is_empty())
            .filter_map(|(slot, verts)| {
                textures
                    .get(slot)
                    .map(|tex| (verts.as_slice(), tex.bind_group()))
            })
            .collect();
        renderer.upload_ghost_entities(&ghost);
        let shadows: Vec<_> = meshed
            .shadows
            .iter()
            .filter(|(_, verts)| !verts.is_empty())
            .filter_map(|(slot, verts)| {
                textures
                    .get(slot)
                    .map(|tex| (verts.as_slice(), tex.bind_group()))
            })
            .collect();
        renderer.upload_shadow_entities(&shadows);
    }

    /// Creates the texture of every horse coat in `slots` that is not there yet.
    fn compose_missing_coats<'a>(
        &self,
        slots: impl Iterator<Item = &'a TextureSlot>,
        renderer: &Renderer,
    ) {
        for slot in slots {
            if self.textures.borrow().contains_key(slot) {
                continue;
            }
            let (rgba, w, h) = match *slot {
                TextureSlot::HorseCoat(variant) => horse_coat(self.jar.as_ref(), variant),
                // A missing armour texture leaves the piece undrawn.
                TextureSlot::Armor { .. }
                | TextureSlot::LeatherArmor { .. }
                | TextureSlot::LeatherOverlay { .. } => armor_texture(self.jar.as_ref(), *slot)
                    .unwrap_or((vec![0; 64 * 32 * 4], 64, 32)),
                _ => continue,
            };
            let texture = renderer.create_entity_texture(&rgba, w, h);
            self.textures.borrow_mut().insert(*slot, texture);
        }
    }
}

impl SkinLookup for EntityTextures {
    fn skin(&self, url: &str) -> Option<u32> {
        self.skins.borrow_mut().lookup(url, SkinKind::Skin)
    }

    fn cape(&self, url: &str) -> Option<u32> {
        self.skins.borrow_mut().lookup(url, SkinKind::Cape)
    }
}

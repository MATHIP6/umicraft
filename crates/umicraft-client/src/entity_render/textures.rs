// SPDX-License-Identifier: MIT OR Apache-2.0

//! Entity texture registry: the jar paths, the procedural fallbacks and the
//! GPU textures keyed by [`TextureSlot`].
//!
//! The table lives in `specs`; horse coats are composed on first use.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::block_entity_render::BannerDesign;
use crate::block_entity_render::{BannerDesigns, BlockEntityMeshes};
use glam::{Mat4, Vec3};
use umicraft_assets::client_jar::ClientJar;
use umicraft_assets::skin::decode_png_bytes;
use umicraft_game::entity::EntityRenderSnapshot;
use umicraft_game::hand::HandPose;
use umicraft_mesh::item::ItemMesh;
use umicraft_model::entity::TextureSlot;
use umicraft_render::{EntityTexture, EntityVertex, Renderer};
use umicraft_ui::FontAtlas;

use super::armor::armor_texture;
use super::banner_cache::BannerCache;
use super::hand::hand_vertices;
use super::horse_coat::horse_coat;
use super::mesh_entities_with;
use super::mobs::skin_slot;
use super::player::posed_player;
use super::skins::{SkinKind, SkinLookup, SkinSet};
use super::specs::texture_specs;
use crate::map_render::{map_arm_vertices, MapBatch};

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
    /// The composed banner designs among `textures`.
    banners: RefCell<BannerCache>,
    /// The camera yaw the flames of burning entities face.
    view_yaw: Cell<f32>,
    /// The camera pitch the experience orbs face.
    view_pitch: Cell<f32>,
    /// Whether the local player is a spectator, who sees invisible entities.
    viewer_is_spectator: Cell<bool>,
    /// Whether the shadows under the entities are drawn ("Entity Shadows").
    shadows_enabled: Cell<bool>,
    /// Where the camera is, for the distance at which a shadow fades.
    camera: Cell<glam::Vec3>,
    /// How far through its 30 ticks the elder guardian that haunts the screen
    /// is, `None` when it does not.
    appearance: Cell<Option<f32>>,
    /// What the player sees, to leave out the entities that are not in view.
    cull_view: Cell<Option<super::cull::CullView>>,
    /// The chests of the frame, meshed by the block-entity renderer and drawn
    /// with the entities (P7c). Taken by the next upload.
    block_entities: RefCell<BlockEntityMeshes>,
}

impl EntityTextures {
    /// Loads all entity textures from the verified jar or procedural fallbacks.
    #[must_use]
    pub fn load(renderer: &Renderer) -> Self {
        let jar = crate::official::find_verified_jar().ok();
        let mut textures = HashMap::new();
        for spec in texture_specs() {
            let (rgba, w, h) = load_or_fallback(jar.as_ref(), spec.jar_path, spec.fallback);
            // The glowing layers and the guardian beam scroll their
            // coordinates past the edge.
            let texture = if spec.slot.repeats() {
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
            banners: RefCell::default(),
            cull_view: Cell::new(None),
            view_yaw: Cell::new(0.0),
            view_pitch: Cell::new(0.0),
            viewer_is_spectator: Cell::new(false),
            shadows_enabled: Cell::new(true),
            camera: Cell::new(glam::Vec3::ZERO),
            appearance: Cell::new(None),
            block_entities: RefCell::default(),
        }
    }

    /// [`Self::load`] plus the sign text texture of `font`, if there is one.
    #[must_use]
    pub fn load_with_font(renderer: &Renderer, font: Option<&FontAtlas>) -> Self {
        let textures = Self::load(renderer);
        if let Some(font) = font {
            textures.set_sign_font(renderer, font);
        }
        textures
    }

    /// Makes the texture of the sign text from the coverage of `font`: black
    /// with the coverage as alpha, so the entity pass's alpha test cuts the
    /// glyphs out (the reference draws the text in black, colour `0`). The
    /// glyph UVs of the sign renderer address this atlas, so it must be called
    /// again with the new atlas whenever the UI font changes.
    pub fn set_sign_font(&self, renderer: &Renderer, font: &FontAtlas) {
        let rgba: Vec<u8> = font
            .pixels
            .iter()
            .flat_map(|&coverage| [0, 0, 0, coverage])
            .collect();
        let texture = renderer.create_entity_texture(&rgba, font.width, font.height);
        self.textures
            .borrow_mut()
            .insert(TextureSlot::SignText, texture);
    }

    /// Sets the camera yaw (radians, `umicraft_render::Camera` convention)
    /// that the flames of burning entities face.
    pub fn set_view_yaw(&self, yaw: f32) {
        self.view_yaw.set(yaw);
    }

    /// Sets the camera pitch (radians, `umicraft_render::Camera` convention)
    /// that the experience orbs face.
    pub fn set_view_pitch(&self, pitch: f32) {
        self.view_pitch.set(pitch);
    }

    /// Sets where the player is and looks (the space of the entity snapshots), to
    /// leave out the entities that are out of view.
    pub fn set_cull_view(&self, eye: glam::Vec3, forward: glam::Vec3) {
        self.cull_view
            .set(Some(super::cull::CullView { eye, forward }));
    }

    /// Sets the position of the camera, which decides how far a shadow is
    /// still drawn.
    pub fn set_camera_position(&self, position: glam::Vec3) {
        self.camera.set(position);
    }

    /// Sets how far through its life the elder guardian that haunts the screen
    /// is (`0..=1`, MCP `MobAppearance`), or `None` for none.
    pub fn set_mob_appearance(&self, progress: Option<f32>) {
        self.appearance.set(progress);
    }

    /// Hands over the tile-entity vertices of this frame, which the next
    /// upload draws with the entities.
    pub(crate) fn set_block_entities(&self, meshes: BlockEntityMeshes) {
        *self.block_entities.borrow_mut() = meshes;
    }

    /// Switches the shadows under the entities (`GameSettings` "Entity
    /// Shadows"); on by default.
    pub fn set_shadows_enabled(&self, enabled: bool) {
        self.shadows_enabled.set(enabled);
    }

    /// Tells whether the local player is a spectator, who sees the invisible
    /// entities as ghosts (MCP `isInvisibleToPlayer`).
    pub fn set_viewer_spectator(&self, spectator: bool) {
        self.viewer_is_spectator.set(spectator);
    }

    /// The texture and arm model of the first-person hand: those of the local
    /// player's body (the downloaded skin and its slim flag), or the default
    /// skin until the local profile is known.
    #[must_use]
    pub fn hand_skin(&self, local: Option<&EntityRenderSnapshot>) -> HandSkin {
        match local {
            Some(body) => {
                let (_, is_slim) = posed_player(body);
                (
                    skin_slot(body, is_slim, self),
                    is_slim,
                    body.flags.skin_parts,
                )
            }
            None => (TextureSlot::Steve, false, ALL_SKIN_PARTS),
        }
    }

    /// Uploads the first-person hand, or hides it when `pose` shows no arm.
    pub fn upload_hand(&self, pose: &HandPose, bob: Mat4, skin: HandSkin, renderer: &mut Renderer) {
        let vertices = hand_vertices(pose, bob, (skin.1, skin.2));
        let textures = self.textures.borrow();
        match (vertices.is_empty(), textures.get(&skin.0)) {
            (false, Some(skin)) => renderer.set_hand(&vertices, skin.bind_group()),
            _ => renderer.clear_hand(),
        }
    }

    /// Composes the texture of `design` if it is missing (a banner icon in
    /// the GUI, which the tile-entity meshes of the frame do not cover).
    pub(crate) fn ensure_banner(&self, design: &BannerDesign, renderer: &Renderer) {
        let designs = BannerDesigns::from([(design.slot(), design.clone())]);
        self.banners.borrow_mut().ensure(
            &designs,
            &mut self.textures.borrow_mut(),
            self.jar.as_ref(),
            renderer,
        );
    }

    /// The bind group of the texture in `slot`, once it exists.
    pub(crate) fn bind_group_of(&self, slot: TextureSlot) -> Option<wgpu::BindGroup> {
        self.textures
            .borrow()
            .get(&slot)
            .map(|texture| texture.bind_group().clone())
    }

    /// Uploads the hand with an item drawn by a tile-entity model (a skull or
    /// a banner): the arm, if the pose shows it, then the item with its own
    /// texture. The hand is cleared when that texture is not ready.
    pub fn upload_hand_item(
        &self,
        pose: &HandPose,
        bob: Mat4,
        skin: HandSkin,
        item: (TextureSlot, &[EntityVertex]),
        renderer: &mut Renderer,
    ) {
        let arm = hand_vertices(pose, bob, (skin.1, skin.2));
        let textures = self.textures.borrow();
        let (Some(skin), Some(texture)) = (textures.get(&skin.0), textures.get(&item.0)) else {
            return renderer.clear_hand();
        };
        renderer.set_hand_batches(&[
            (arm.as_slice(), skin.bind_group()),
            (item.1, texture.bind_group()),
        ]);
    }

    /// Uploads the hands that hold a map: both arms, then the `map` batches.
    pub fn upload_hand_map(
        &self,
        pose: &HandPose,
        bob: Mat4,
        skin: HandSkin,
        map: &[MapBatch],
        renderer: &mut Renderer,
    ) {
        let arms = map_arm_vertices(pose, bob, (skin.1, skin.2));
        let textures = self.textures.borrow();
        let Some(skin) = textures.get(&skin.0) else {
            return renderer.clear_hand();
        };
        let mut batches = vec![(arms.as_slice(), skin.bind_group())];
        batches.extend(map.iter().map(|b| (b.vertices.as_slice(), &b.bind_group)));
        renderer.set_hand_batches(&batches);
    }

    /// Meshes all visible entities and uploads them to the GPU renderer.
    pub fn mesh_and_upload(&self, snapshots: &[EntityRenderSnapshot], renderer: &mut Renderer) {
        self.mesh_and_upload_with(snapshots, &[], renderer);
    }

    /// [`Self::mesh_and_upload`] with extra batches, such as framed maps.
    pub fn mesh_and_upload_with(
        &self,
        snapshots: &[EntityRenderSnapshot],
        extra: &[MapBatch],
        renderer: &mut Renderer,
    ) {
        umicraft_core::profile_scope!("mesh_entities");
        self.skins
            .borrow_mut()
            .collect(renderer, &mut self.textures.borrow_mut());
        let mut meshed = mesh_entities_with(
            snapshots,
            self,
            (self.view_yaw.get(), self.view_pitch.get()),
            self.viewer_is_spectator.get(),
            (self.camera.get(), self.cull_view.get()),
        );
        let apparition = self.appearance.get().map(|progress| {
            super::appearance::elder_guardian(
                self,
                self.camera.get(),
                (self.view_yaw.get(), self.view_pitch.get()),
                progress,
            )
        });
        let block_entities = self.block_entities.take();
        for (slot, vertices) in block_entities.solid {
            meshed.solid.entry(slot).or_default().extend(vertices);
        }
        for (slot, vertices) in block_entities.additive {
            meshed.additive.entry(slot).or_default().extend(vertices);
        }
        self.banners.borrow_mut().ensure(
            &block_entities.banners,
            &mut self.textures.borrow_mut(),
            self.jar.as_ref(),
            renderer,
        );
        self.compose_missing_coats(meshed.solid.keys(), renderer);
        let textures = self.textures.borrow();
        let mut solid = bound_batches(&meshed.solid, &textures);
        solid.extend(extra.iter().map(|b| (b.vertices.as_slice(), &b.bind_group)));
        renderer.upload_entities(&solid);
        let additive = bound_batches(&meshed.additive, &textures);
        renderer.upload_additive_entities(&additive);
        let glint = bound_batches(&meshed.glint, &textures);
        renderer.upload_glint_entities(&glint);
        let ghost = bound_batches(&meshed.ghost, &textures);
        renderer.upload_ghost_entities(&ghost);
        renderer.upload_tinted_entities(&bound_batches(&meshed.tinted, &textures));
        let apparition = apparition
            .as_ref()
            .map_or_else(Vec::new, |meshes| bound_batches(meshes, &textures));
        renderer.upload_apparition_entities(&apparition);
        if !self.shadows_enabled.get() {
            meshed.shadows.clear();
        }
        let shadows = bound_batches(&meshed.shadows, &textures);
        renderer.upload_shadow_entities(&shadows);
    }

    /// The face of the skin at `url` for a tab list head, once it is downloaded,
    /// with the hat layer when `hat` is set; the first call starts the download.
    pub(crate) fn face(&self, url: &str, hat: bool) -> Option<std::sync::Arc<[u8]>> {
        self.skins.borrow_mut().face(url, hat)
    }

    /// Draws `body` into the portrait of the icon atlas, `placement` carrying
    /// the model at the world origin into the icon box (`GuiInventory`'s
    /// player model). Only the lit layers are drawn: no shadow, no glint. `held`
    /// is the item geometry of the body (the item in the hand, a helmet block),
    /// already carried by `placement`.
    pub(crate) fn render_portrait(
        &self,
        body: &EntityRenderSnapshot,
        placement: Mat4,
        held: &ItemMesh,
        renderer: &mut Renderer,
    ) {
        self.skins
            .borrow_mut()
            .collect(renderer, &mut self.textures.borrow_mut());
        let meshed = mesh_entities_with(
            std::slice::from_ref(body),
            self,
            (0.0, 0.0),
            false,
            (Vec3::ZERO, None),
        );
        self.compose_missing_coats(meshed.solid.keys(), renderer);
        let textures = self.textures.borrow();
        let mut posed: Vec<(TextureSlot, Vec<EntityVertex>)> = meshed
            .solid
            .into_iter()
            .filter(|(_, vertices)| !vertices.is_empty())
            .collect();
        for vertex in posed.iter_mut().flat_map(|(_, vertices)| vertices) {
            vertex.position = placement
                .transform_point3(Vec3::from(vertex.position))
                .to_array();
            vertex.normal = placement
                .transform_vector3(Vec3::from(vertex.normal))
                .normalize_or_zero()
                .to_array();
        }
        let batches: Vec<_> = posed
            .iter()
            .filter_map(|(slot, vertices)| {
                textures
                    .get(slot)
                    .map(|texture| (vertices.as_slice(), texture.bind_group()))
            })
            .collect();
        renderer.render_portrait(&batches, held);
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

/// The batches of `meshes` that have vertices, each with the bind group of
/// its texture; a slot whose texture is not loaded is left out.
fn bound_batches<'a>(
    meshes: &'a HashMap<TextureSlot, Vec<EntityVertex>>,
    textures: &'a HashMap<TextureSlot, EntityTexture>,
) -> Vec<(&'a [EntityVertex], &'a wgpu::BindGroup)> {
    meshes
        .iter()
        .filter(|(_, verts)| !verts.is_empty())
        .filter_map(|(slot, verts)| {
            textures
                .get(slot)
                .map(|tex| (verts.as_slice(), tex.bind_group()))
        })
        .collect()
}

/// The texture of the first-person arm, whether its model is slim and the
/// skin layers the player shows (`EnumPlayerModelParts`).
pub type HandSkin = (TextureSlot, bool, u8);

/// Every skin layer on, as before the local profile is known.
const ALL_SKIN_PARTS: u8 = 0x7F;

impl SkinLookup for EntityTextures {
    fn skin(&self, url: &str) -> Option<u32> {
        self.skins.borrow_mut().lookup(url, SkinKind::Skin)
    }

    fn cape(&self, url: &str) -> Option<u32> {
        self.skins.borrow_mut().lookup(url, SkinKind::Cape)
    }
}

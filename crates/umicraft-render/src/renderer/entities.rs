// SPDX-License-Identifier: MIT OR Apache-2.0

//! The entity batches and textures, and the first-person hand.

use super::Renderer;
use crate::entity_pass::{EntityRenderer, EntityVertex};

impl Renderer {
    /// Access to the entity renderer pipeline.
    #[must_use]
    pub fn entity_renderer(&self) -> &EntityRenderer {
        &self.entity_renderer
    }

    /// Uploads entity vertex batches to be drawn in the next frame.
    pub fn upload_entities(&mut self, batches: &[(&[EntityVertex], &wgpu::BindGroup)]) {
        self.entity_batches.set(&self.device, &self.queue, batches);
    }

    /// Uploads the entities the mods marked for the mask of their post passes
    /// (API 1.4): vertices made with [`mask_vertex`](crate::mask_vertex),
    /// `tested` ones drawn where the world shows them, `through` ones whole.
    /// `marked` is how many entities they are, which the passes read as
    /// `umi.mask.y`.
    pub fn upload_mask_entities(
        &mut self,
        tested: &[(&[EntityVertex], &wgpu::BindGroup)],
        through: &[(&[EntityVertex], &wgpu::BindGroup)],
        marked: u32,
    ) {
        self.entity_mask
            .set(&self.device, &self.queue, tested, through);
        self.post_chain.set_marked(marked);
    }

    /// Uploads the glowing entity layers (eyes, charge, aura, glint) to be
    /// drawn after the entities. The `light` of their vertices is the tint
    /// (bits 0..=23, `0xBBGGRR`) and bit 24 squares the colour.
    pub fn upload_additive_entities(&mut self, batches: &[(&[EntityVertex], &wgpu::BindGroup)]) {
        self.additive_batches
            .set(&self.device, &self.queue, batches);
    }

    /// Uploads the shadows under the entities: squares whose `light` is the
    /// opacity in its low byte.
    pub fn upload_shadow_entities(&mut self, batches: &[(&[EntityVertex], &wgpu::BindGroup)]) {
        self.shadow_batches.set(&self.device, &self.queue, batches);
    }

    /// Uploads the invisible entities a spectator sees (their model, drawn at
    /// 15 % opacity without writing depth).
    pub fn upload_ghost_entities(&mut self, batches: &[(&[EntityVertex], &wgpu::BindGroup)]) {
        self.ghost_batches.set(&self.device, &self.queue, batches);
    }

    /// Uploads the guardian beams: opaque, unlit vertices whose light word
    /// is their red, green and blue.
    pub fn upload_tinted_entities(&mut self, batches: &[(&[EntityVertex], &wgpu::BindGroup)]) {
        self.tinted_batches.set(&self.device, &self.queue, batches);
    }

    /// Uploads the elder guardian apparition (its model, translucent by the
    /// opacity bits of its vertices, writing depth).
    pub fn upload_apparition_entities(&mut self, batches: &[(&[EntityVertex], &wgpu::BindGroup)]) {
        self.apparition_batches
            .set(&self.device, &self.queue, batches);
    }

    /// Uploads the enchantment glint of worn armour: the same vertices as the
    /// armour, drawn only where the armour wrote its depth.
    pub fn upload_glint_entities(&mut self, batches: &[(&[EntityVertex], &wgpu::BindGroup)]) {
        self.glint_batches.set(&self.device, &self.queue, batches);
    }

    /// Sets the first-person hand for the following frames.
    ///
    /// `vertices` are in camera space (the camera at the origin looking down
    /// −Z, the view bobbing already applied) and use the skin texture of
    /// `bind_group`; an empty slice hides the hand. It is drawn on top of the
    /// world and under the HUD.
    pub fn set_hand(&mut self, vertices: &[EntityVertex], bind_group: &wgpu::BindGroup) {
        self.set_hand_batches(&[(vertices, bind_group)]);
    }

    /// Sets the first-person hand as several batches, drawn in order (the
    /// arms, then a held map with its background, pixels and markers).
    /// Empty batches are skipped.
    pub fn set_hand_batches(&mut self, batches: &[(&[EntityVertex], &wgpu::BindGroup)]) {
        self.hand.set(&self.device, &self.queue, batches);
    }

    /// Hides the first-person hand.
    pub fn clear_hand(&mut self) {
        self.hand.clear();
    }

    /// Clears any queued entity batches.
    pub fn clear_entities(&mut self) {
        self.entity_batches.clear();
        self.additive_batches.clear();
        self.glint_batches.clear();
        self.ghost_batches.clear();
        self.apparition_batches.clear();
        self.tinted_batches.clear();
        self.shadow_batches.clear();
    }

    /// Creates an entity texture (such as a mob skin or player skin) for entity batches.
    #[must_use]
    pub fn create_entity_texture(
        &self,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> crate::entity_pass::EntityTexture {
        self.entity_renderer
            .create_texture(&self.device, &self.queue, rgba, width, height)
    }

    /// Creates a repeating entity texture for the glowing layers, whose
    /// coordinates scroll past the edge.
    #[must_use]
    pub fn create_repeating_entity_texture(
        &self,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> crate::entity_pass::EntityTexture {
        self.entity_renderer.create_repeating_texture(
            &self.device,
            &self.queue,
            rgba,
            width,
            height,
        )
    }
}

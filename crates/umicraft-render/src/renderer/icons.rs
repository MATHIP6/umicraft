// SPDX-License-Identifier: MIT OR Apache-2.0

//! The item and entity icons of the GUI, and the player portrait.

use umicraft_mesh::item::ItemMesh;
use umicraft_ui::{DrawCommand, DrawList};
use wgpu::util::DeviceExt;

use super::pipelines::{create_gpu_mesh_pass, draw_gpu_pass};
use super::Renderer;
use crate::entity_pass::EntityVertex;
use crate::items::IconProvider;

impl Renderer {
    /// Whether the icon atlas holds the icon `key`.
    #[must_use]
    pub fn has_icon(&self, key: u64) -> bool {
        self.icon_atlas.contains(key)
    }

    /// Forgets the icon `key`, so that it is built again when asked for.
    pub fn forget_icon(&mut self, key: u64) {
        self.icon_atlas.forget(key);
    }

    /// Renders an icon drawn by a tile-entity model (a skull or a banner) into
    /// the icon atlas under `key`.
    ///
    /// `vertices` are posed into the 16x16 GUI box like an [`ItemMesh`]
    /// (`umicraft_model::item::pose::gui_pose`) and sample `texture`, an entity
    /// texture bind group. Nothing is drawn for an empty list. The icon uses
    /// the icon variant of the entity pipeline, lit at full light by the two
    /// fixed GUI lights (`gui_shade` in the entity shader).
    pub fn render_entity_icon(
        &mut self,
        key: u64,
        vertices: &[EntityVertex],
        texture: &wgpu::BindGroup,
    ) {
        if vertices.is_empty() {
            return;
        }
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("entity icon vertices"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let batch = [(buffer.slice(..), vertices.len() as u32, texture)];
        let (entities, camera) = (&self.entity_renderer, &self.icon_entity_bind_group);
        self.icon_atlas
            .render_with(&self.device, &self.queue, key, |pass| {
                entities.render_icon(pass, camera, batch);
            });
    }

    /// Redraws the portrait, the large icon of the inventory screen that
    /// shows the player, under [`crate::PORTRAIT_KEY`].
    ///
    /// Each batch is a vertex list and the entity texture it samples. The
    /// vertices are posed in the 16x16 GUI box like a
    /// [`Renderer::render_entity_icon`] icon, but the box fills the whole
    /// [`crate::PORTRAIT_SIZE`] square, so the caller scales the model to fit.
    /// The pipeline is the icon one: the two GUI lights at full light.
    pub fn render_portrait(
        &mut self,
        batches: &[(&[EntityVertex], &wgpu::BindGroup)],
        items: &ItemMesh,
    ) {
        let buffers: Vec<(wgpu::Buffer, u32, &wgpu::BindGroup)> = batches
            .iter()
            .filter(|(vertices, _)| !vertices.is_empty())
            .map(|&(vertices, texture)| {
                let buffer = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("portrait vertices"),
                        contents: bytemuck::cast_slice(vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                (buffer, vertices.len() as u32, texture)
            })
            .collect();
        // The item in the hand, already in the icon box, drawn with the pipeline
        // of the item icons (same camera, no culling).
        let held = create_gpu_mesh_pass(
            &self.device,
            "portrait item",
            &items.vertices,
            &items.indices,
        );
        let (entities, camera) = (&self.entity_renderer, &self.icon_entity_bind_group);
        let (icon_pipeline, icon_group) = (&self.icon_pipeline, &self.icon_bind_group);
        self.icon_atlas
            .render_portrait(&self.device, &self.queue, |pass| {
                entities.render_icon(
                    pass,
                    camera,
                    buffers.iter().map(|(b, n, t)| (b.slice(..), *n, *t)),
                );
                if let Some(held) = &held {
                    pass.set_pipeline(icon_pipeline);
                    pass.set_bind_group(0, icon_group, &[]);
                    draw_gpu_pass(pass, held);
                }
            });
    }

    /// Renders the icons `draw_list` asks for that the atlas lacks.
    pub(super) fn prepare_icons(&mut self, draw_list: &DrawList, icons: &mut dyn IconProvider) {
        for command in draw_list.commands() {
            let DrawCommand::Item { key, .. } = command else {
                continue;
            };
            if self.icon_atlas.contains(*key) {
                continue;
            }
            self.icon_mesh.clear();
            if icons.build_icon(*key, &mut self.icon_mesh) {
                self.icon_atlas.render(
                    &self.device,
                    &self.queue,
                    &self.icon_pipeline,
                    &self.icon_bind_group,
                    *key,
                    &self.icon_mesh,
                );
            }
        }
    }
}

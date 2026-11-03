// SPDX-License-Identifier: MIT OR Apache-2.0

//! The icon atlas: item meshes rendered once into a grid of cells.
//!
//! Each icon is drawn into a small scratch target (colour + depth) and then
//! copied into its atlas cell, because a render pass clears depth for its
//! whole attachment and the atlas is far larger than one icon. Meshes are in
//! the 16x16 GUI box of `gui_pose` (origin top-left, y down, larger z
//! nearer), which the orthographic camera below maps to the whole cell.

use std::collections::HashMap;

use bytemuck::bytes_of;
use umicraft_mesh::item::ItemMesh;
use wgpu::util::DeviceExt;

use crate::renderer::{create_gpu_mesh_pass, draw_gpu_pass, CameraUniform, DEPTH_FORMAT};

/// Side of the square icon atlas texture in pixels.
pub const ICON_ATLAS_SIZE: u32 = 2048;
/// Side of one icon cell in pixels (four times the 16 px GUI icon).
pub const ICON_CELL_SIZE: u32 = 64;

/// Key under which the portrait (the player model of the inventory screen)
/// is drawn. Icon keys are hashes of item stacks, and `u64::MAX` is the
/// placeholder of a slot without item resources; this value is reserved.
pub const PORTRAIT_KEY: u64 = u64::MAX - 1;
/// Side of the portrait in cells.
const PORTRAIT_CELLS: u32 = 4;
/// Side of the portrait in pixels: it covers 64x64 GUI pixels at the same
/// density as an icon.
pub const PORTRAIT_SIZE: u32 = PORTRAIT_CELLS * ICON_CELL_SIZE;

const CELLS_PER_ROW: u32 = ICON_ATLAS_SIZE / ICON_CELL_SIZE;
/// The last rows of the atlas are reserved for the portrait, which is
/// redrawn in place so that a moving mouse never uses up item icon cells.
const CELL_COUNT: u32 = CELLS_PER_ROW * (CELLS_PER_ROW - PORTRAIT_CELLS);
/// Top-left cell of the portrait.
const PORTRAIT_CELL: (u32, u32) = (0, CELLS_PER_ROW - PORTRAIT_CELLS);

/// The atlas texture, its cell bookkeeping and the scratch targets.
pub(crate) struct IconAtlas {
    atlas: wgpu::Texture,
    atlas_view: wgpu::TextureView,
    scratch: wgpu::Texture,
    scratch_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    camera_buffer: wgpu::Buffer,
    cells: HashMap<u64, u32>,
    next_cell: u32,
}

impl IconAtlas {
    /// Creates an empty atlas rendering to textures of `format` (the format
    /// the terrain pipelines were built for).
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        let atlas = create_color_texture(
            device,
            "icon atlas",
            ICON_ATLAS_SIZE,
            format,
            wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let scratch = create_color_texture(
            device,
            "icon scratch",
            PORTRAIT_SIZE,
            format,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("icon depth"),
            size: extent(PORTRAIT_SIZE),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("icon camera"),
            contents: bytes_of(&CameraUniform::icon()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let this = Self {
            atlas_view: atlas.create_view(&wgpu::TextureViewDescriptor::default()),
            scratch_view: scratch.create_view(&wgpu::TextureViewDescriptor::default()),
            depth_view: depth.create_view(&wgpu::TextureViewDescriptor::default()),
            atlas,
            scratch,
            camera_buffer,
            cells: HashMap::new(),
            next_cell: 0,
        };
        this.clear_atlas(device, queue);
        this
    }

    /// The uniform buffer of the icon camera, for building its bind group.
    pub(crate) fn camera_buffer(&self) -> &wgpu::Buffer {
        &self.camera_buffer
    }

    /// The atlas as a sampled texture.
    pub(crate) fn view(&self) -> &wgpu::TextureView {
        &self.atlas_view
    }

    /// Forgets every icon, e.g. after the block texture array was replaced.
    pub(crate) fn invalidate(&mut self) {
        self.cells.clear();
        self.next_cell = 0;
    }

    /// The normalised `[u_min, v_min, u_max, v_max]` of a rendered icon.
    pub(crate) fn uv(&self, key: u64) -> Option<[f32; 4]> {
        if key == PORTRAIT_KEY {
            return Some(portrait_uv());
        }
        self.cells.get(&key).map(|&cell| cell_uv(cell))
    }

    /// Whether `key` already has a cell. The portrait always does: it is
    /// redrawn by [`IconAtlas::render_portrait`], never built on demand.
    pub(crate) fn contains(&self, key: u64) -> bool {
        key == PORTRAIT_KEY || self.cells.contains_key(&key)
    }

    /// Renders `mesh` into a free cell under `key`.
    ///
    /// `pipeline` and `bind_group` are the terrain pipeline variant without
    /// culling and its bind group over this atlas's camera. When every cell
    /// is taken the atlas is emptied first, so icons of the current frame
    /// that were already placed are redrawn on the next frame.
    pub(crate) fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipeline: &wgpu::RenderPipeline,
        bind_group: &wgpu::BindGroup,
        key: u64,
        mesh: &ItemMesh,
    ) -> bool {
        let Some(gpu_mesh) =
            create_gpu_mesh_pass(device, "icon mesh", &mesh.vertices, &mesh.indices)
        else {
            return false;
        };
        self.render_with(device, queue, key, |pass| {
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            draw_gpu_pass(pass, &gpu_mesh);
        });
        true
    }

    /// Draws into a free cell under `key` with `draw`, which receives the
    /// pass of the cell (cleared colour and depth, the icon camera's box).
    ///
    /// When every cell is taken the atlas is emptied first, so icons of the
    /// current frame that were already placed are redrawn on the next frame.
    pub(crate) fn render_with(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: u64,
        draw: impl FnOnce(&mut wgpu::RenderPass<'_>),
    ) {
        if self.next_cell >= CELL_COUNT {
            self.invalidate();
        }
        let cell = self.next_cell;
        self.next_cell += 1;
        let origin = cell_origin(cell);
        self.draw_into(device, queue, ICON_CELL_SIZE, origin, draw);
        self.cells.insert(key, cell);
    }

    /// Redraws the portrait block with `draw`, which receives a pass whose
    /// camera maps the 16x16 GUI box to the whole block.
    pub(crate) fn render_portrait(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        draw: impl FnOnce(&mut wgpu::RenderPass<'_>),
    ) {
        let origin = cell_origin_of(PORTRAIT_CELL.0, PORTRAIT_CELL.1);
        self.draw_into(device, queue, PORTRAIT_SIZE, origin, draw);
    }

    /// Draws a `size` x `size` square with `draw` into the scratch target and
    /// copies it to `origin` of the atlas.
    fn draw_into(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: u32,
        origin: wgpu::Origin3d,
        draw: impl FnOnce(&mut wgpu::RenderPass<'_>),
    ) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("icon encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("icon pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.scratch_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // The scratch target is sized for the portrait; a smaller icon
            // uses its corner and the camera maps the box to that corner.
            pass.set_viewport(0.0, 0.0, size as f32, size as f32, 0.0, 1.0);
            draw(&mut pass);
        }
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.scratch,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &self.atlas,
                mip_level: 0,
                origin,
                aspect: wgpu::TextureAspect::All,
            },
            extent(size),
        );
        queue.submit(Some(encoder.finish()));
    }

    /// Forgets the icon under `key`; its cell stays unused until the atlas
    /// wraps.
    pub(crate) fn forget(&mut self, key: u64) {
        self.cells.remove(&key);
    }

    /// Clears the atlas to transparent so unused cells sample nothing.
    fn clear_atlas(&self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("icon atlas clear"),
        });
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("icon atlas clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.atlas_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        }));
        queue.submit(Some(encoder.finish()));
    }
}

fn create_color_texture(
    device: &wgpu::Device,
    label: &str,
    side: u32,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: side,
            height: side,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

fn extent(side: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: side,
        height: side,
        depth_or_array_layers: 1,
    }
}

fn cell_origin(cell: u32) -> wgpu::Origin3d {
    cell_origin_of(cell % CELLS_PER_ROW, cell / CELLS_PER_ROW)
}

fn cell_origin_of(column: u32, row: u32) -> wgpu::Origin3d {
    wgpu::Origin3d {
        x: column * ICON_CELL_SIZE,
        y: row * ICON_CELL_SIZE,
        z: 0,
    }
}

/// The normalised rectangle of the portrait block.
fn portrait_uv() -> [f32; 4] {
    let origin = cell_origin_of(PORTRAIT_CELL.0, PORTRAIT_CELL.1);
    let size = ICON_ATLAS_SIZE as f32;
    let (u, v) = (origin.x as f32 / size, origin.y as f32 / size);
    let side = PORTRAIT_SIZE as f32 / size;
    [u, v, u + side, v + side]
}

/// The normalised rectangle of `cell` in the atlas.
fn cell_uv(cell: u32) -> [f32; 4] {
    let origin = cell_origin(cell);
    let size = ICON_ATLAS_SIZE as f32;
    let (u, v) = (origin.x as f32 / size, origin.y as f32 / size);
    let cell_size = ICON_CELL_SIZE as f32 / size;
    [u, v, u + cell_size, v + cell_size]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_tile_the_atlas_row_by_row() {
        assert_eq!(cell_uv(0), [0.0, 0.0, 1.0 / 32.0, 1.0 / 32.0]);
        let second_row = cell_uv(CELLS_PER_ROW);
        assert_eq!(second_row[0], 0.0);
        assert_eq!(second_row[1], 1.0 / 32.0);
    }

    #[test]
    fn icon_cells_end_where_the_portrait_begins() {
        let last = cell_uv(CELL_COUNT - 1);
        let portrait = portrait_uv();
        assert_eq!(last[2], 1.0, "the last row of icons spans the atlas");
        assert_eq!(last[3], portrait[1], "and the portrait starts right below");
    }

    #[test]
    fn the_portrait_fills_the_bottom_left_corner() {
        let uv = portrait_uv();
        assert_eq!(uv, [0.0, 28.0 / 32.0, 4.0 / 32.0, 1.0]);
        assert_eq!(
            (uv[2] - uv[0]) * ICON_ATLAS_SIZE as f32,
            PORTRAIT_SIZE as f32
        );
    }
}

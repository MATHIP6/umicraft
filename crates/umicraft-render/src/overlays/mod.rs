// SPDX-License-Identifier: MIT OR Apache-2.0

//! The full-screen overlays (P8-f): underwater, inside a wall, fire,
//! pumpkin, nether portal and vignette.
//!
//! | Module | Owns |
//! |---|---|
//! | `data` | [`ScreenOverlays`], the plain data the client fills |
//! | `geometry` | the quads, built like `ItemRenderer` and `GuiIngame` |
//! | `textures` | the original stand-ins for the three pictures |
//!
//! # Draw order
//!
//! The reference draws the three overlays of the eye (`renderOverlays`) right
//! after the hand and the vignette, pumpkin and portal first thing in the GUI
//! (`renderGameOverlay`), before the hotbar. This pass is called once at that
//! point: no depth test, no depth write, alpha blending; the vignette
//! multiplies the screen instead.
//!
//! # Frame cost
//!
//! At most [`MAX_VERTICES`] vertices are rebuilt into one reused list, and
//! nothing is drawn or uploaded while no overlay shows.

mod data;
mod geometry;
mod textures;

use bytemuck::cast_slice;

use crate::sky::{self, SkyImage};
use crate::Camera;
pub use data::{Portal, ScreenOverlays, Underwater};
use geometry::{Batch, Blend, ScreenVertex, MAX_VERTICES};

/// The shader of the pass.
const SHADER: &str = include_str!("screen.wgsl");

/// The three pictures of the pass, each `None` for the fallback.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Pictures<'a> {
    /// `misc/underwater.png`.
    pub underwater: Option<&'a SkyImage>,
    /// `misc/pumpkinblur.png`.
    pub pumpkin_blur: Option<&'a SkyImage>,
    /// `misc/vignette.png`.
    pub vignette: Option<&'a SkyImage>,
}

/// GPU resources of the overlay pass.
pub(crate) struct ScreenRenderer {
    alpha: wgpu::RenderPipeline,
    vignette: wgpu::RenderPipeline,
    pictures_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pictures: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    batch: Batch,
}

impl ScreenRenderer {
    /// Creates the pipelines and the fallback pictures. `terrain_layout` is
    /// the layout of the terrain bind group, whose block atlas gives the
    /// sprites of walls, fire and portal.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        terrain_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        let pictures_layout = pictures_layout(device);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("screen overlay shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("screen overlay pipeline layout"),
            bind_group_layouts: &[Some(terrain_layout), Some(&pictures_layout)],
            immediate_size: 0,
        });
        // Repeat: the water picture goes past 0..1.
        let sampler = sky::sampler(device, wgpu::AddressMode::Repeat);
        Self {
            alpha: pipeline(device, &layout, &shader, format, Blend::Alpha),
            vignette: pipeline(device, &layout, &shader, format, Blend::Vignette),
            pictures: pictures_group(
                device,
                queue,
                &pictures_layout,
                &sampler,
                Pictures::default(),
            ),
            pictures_layout,
            sampler,
            vertex_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("screen overlay vertices"),
                size: (MAX_VERTICES * std::mem::size_of::<ScreenVertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            batch: Batch::default(),
        }
    }

    /// Replaces the three pictures; a missing or malformed one restores its
    /// original fallback.
    pub(crate) fn set_pictures(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pictures: Pictures<'_>,
    ) {
        self.pictures = pictures_group(
            device,
            queue,
            &self.pictures_layout,
            &self.sampler,
            pictures,
        );
    }

    /// Builds and uploads the quads of the frame.
    pub(crate) fn update(
        &mut self,
        queue: &wgpu::Queue,
        overlays: &ScreenOverlays,
        camera: &Camera,
    ) {
        self.batch.rebuild(overlays, camera.hand_projection());
        if !self.batch.vertices.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, cast_slice(&self.batch.vertices));
        }
    }

    /// Draws the frame's quads with the terrain's bind group.
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, terrain: &wgpu::BindGroup) {
        if self.batch.draws.is_empty() {
            return;
        }
        pass.set_bind_group(0, terrain, &[]);
        pass.set_bind_group(1, &self.pictures, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        for draw in &self.batch.draws {
            pass.set_pipeline(match draw.blend {
                Blend::Alpha => &self.alpha,
                Blend::Vignette => &self.vignette,
            });
            pass.draw(draw.range.clone(), 0..1);
        }
    }
}

/// The bind layout of the pictures: three textures and a sampler.
fn pictures_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let texture = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("screen overlay pictures layout"),
        entries: &[
            texture(0),
            texture(1),
            texture(2),
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

/// The image to upload: the given one when it is well formed, else `fallback`.
fn valid_or(image: Option<&SkyImage>, fallback: fn() -> SkyImage) -> SkyImage {
    match image {
        Some(image) if image.is_valid() => image.clone(),
        _ => fallback(),
    }
}

/// Uploads the pictures and binds them with the sampler.
fn pictures_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    pictures: Pictures<'_>,
) -> wgpu::BindGroup {
    let views = [
        sky::upload(
            device,
            queue,
            &valid_or(pictures.underwater, textures::fallback_underwater),
        ),
        sky::upload(
            device,
            queue,
            &valid_or(pictures.pumpkin_blur, textures::fallback_pumpkin),
        ),
        sky::upload(
            device,
            queue,
            &valid_or(pictures.vignette, textures::fallback_vignette),
        ),
    ];
    let mut entries: Vec<wgpu::BindGroupEntry<'_>> = views
        .iter()
        .zip(0..)
        .map(|(view, binding)| wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::TextureView(view),
        })
        .collect();
    entries.push(wgpu::BindGroupEntry {
        binding: 3,
        resource: wgpu::BindingResource::Sampler(sampler),
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("screen overlay pictures"),
        layout,
        entries: &entries,
    })
}

/// The blend state of a mode: the reference's `tryBlendFuncSeparate`.
fn blend_state(blend: Blend) -> wgpu::BlendState {
    let (src, dst) = match blend {
        Blend::Alpha => (
            wgpu::BlendFactor::SrcAlpha,
            wgpu::BlendFactor::OneMinusSrcAlpha,
        ),
        Blend::Vignette => (wgpu::BlendFactor::Zero, wgpu::BlendFactor::OneMinusSrc),
    };
    wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: src,
            dst_factor: dst,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::Zero,
            operation: wgpu::BlendOperation::Add,
        },
    }
}

/// A pipeline that ignores the depth buffer and blends as `blend`.
fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    blend: Blend,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("screen overlay pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<ScreenVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &ScreenVertex::ATTRIBUTES,
            })],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..wgpu::PrimitiveState::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: crate::renderer::DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(blend_state(blend)),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shader is only compiled on a GPU device; this validates it without one.
    #[test]
    fn the_screen_shader_is_valid_wgsl() {
        let module = naga::front::wgsl::parse_str(SHADER).expect("the shader parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("the shader validates");
        for entry in ["vs_main", "fs_main"] {
            assert!(module.entry_points.iter().any(|point| point.name == entry));
        }
    }

    #[test]
    fn the_vertex_is_ten_floats() {
        assert_eq!(std::mem::size_of::<ScreenVertex>(), 40);
    }

    #[test]
    fn a_malformed_picture_falls_back() {
        let bad = SkyImage {
            width: 2,
            height: 2,
            rgba: vec![0; 3],
        };
        assert!(valid_or(Some(&bad), textures::fallback_vignette).is_valid());
        assert!(valid_or(None, textures::fallback_vignette).is_valid());
    }
}

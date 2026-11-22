// SPDX-License-Identifier: MIT OR Apache-2.0

//! The render pipelines the renderer builds itself (terrain and overlays),
//! and the GPU mesh helpers other passes share.

use umicraft_mesh::Vertex;
use wgpu::util::DeviceExt;

use super::{GpuMeshPass, DEPTH_FORMAT, OVERLAY_ATTRIBUTES, TERRAIN_ATTRIBUTES};
use crate::overlay::OverlayVertex;

/// The terrain shader with the shared fog falloff in front.
pub(super) const TERRAIN_WGSL: &str = concat!(
    include_str!("../fog.wgsl"),
    include_str!("../lightmap.wgsl"),
    include_str!("../dynamic_lights.wgsl"),
    include_str!("../terrain.wgsl")
);

/// Pipeline settings that differ between the opaque and transparent terrain
/// passes. Both share the vertex entry point and bind group layout.
pub(super) struct TerrainPipelineState {
    pub(super) label: &'static str,
    pub(super) fragment_entry: &'static str,
    pub(super) depth_write: bool,
    pub(super) depth_compare: wgpu::CompareFunction,
    pub(super) blend: wgpu::BlendState,
    pub(super) bias: wgpu::DepthBiasState,
    pub(super) cull_mode: Option<wgpu::Face>,
}

impl TerrainPipelineState {
    /// Opaque pass: closer depth wins, depth is written, alpha cutout runs in
    /// the fragment shader.
    pub(super) fn opaque() -> Self {
        Self {
            label: "terrain pipeline",
            fragment_entry: "fs_main",
            depth_write: true,
            depth_compare: wgpu::CompareFunction::Less,
            blend: wgpu::BlendState::REPLACE,
            bias: wgpu::DepthBiasState::default(),
            cull_mode: Some(wgpu::Face::Back),
        }
    }

    /// The opaque pass for quads that never need the alpha test: the same
    /// state, a fragment shader without `discard`, so the GPU keeps early
    /// depth rejection.
    pub(super) fn solid() -> Self {
        Self {
            label: "terrain solid pipeline",
            fragment_entry: "fs_solid",
            ..Self::opaque()
        }
    }

    /// Item icons: the opaque pass without back-face culling, because the
    /// GUI chains mirror space and the reference draws icons uncullled.
    pub(super) fn icon() -> Self {
        Self {
            label: "icon pipeline",
            fragment_entry: "fs_icon",
            cull_mode: None,
            ..Self::opaque()
        }
    }

    /// Transparent pass: `LessEqual` keeps coincident/behind faces from
    /// disappearing, depth write is off so blended faces accumulate, and
    /// straight alpha is blended (`SrcAlpha`/`OneMinusSrcAlpha`).
    pub(super) fn transparent() -> Self {
        Self {
            label: "terrain transparent pipeline",
            fragment_entry: "fs_transparent",
            depth_write: false,
            depth_compare: wgpu::CompareFunction::LessEqual,
            blend: wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
            },
            bias: wgpu::DepthBiasState::default(),
            cull_mode: Some(wgpu::Face::Back),
        }
    }

    /// Break cracks: the destroy-stage texture multiplies the colour below
    /// it twice (`DstColor`/`SrcColor`) and is pulled towards the camera so
    /// it does not fight the block faces (MCP
    /// `RenderGlobal.drawBlockDamageTexture`: `tryBlendFuncSeparate(774, 768,
    /// 1, 0)`, `doPolygonOffset(-3, -3)`).
    pub(super) fn cracks() -> Self {
        Self {
            label: "terrain crack pipeline",
            fragment_entry: "fs_crack",
            depth_write: false,
            depth_compare: wgpu::CompareFunction::LessEqual,
            blend: wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Dst,
                    dst_factor: wgpu::BlendFactor::Src,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::Zero,
                    operation: wgpu::BlendOperation::Add,
                },
            },
            bias: wgpu::DepthBiasState {
                constant: -3,
                slope_scale: -3.0,
                clamp: 0.0,
            },
            cull_mode: Some(wgpu::Face::Back),
        }
    }
}

pub(super) fn create_terrain_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    bind_group_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    state: TerrainPipelineState,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("terrain pipeline layout"),
        bind_group_layouts: &[Some(bind_group_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(state.label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &TERRAIN_ATTRIBUTES,
            })],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: state.cull_mode,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(state.depth_write),
            depth_compare: Some(state.depth_compare),
            stencil: wgpu::StencilState::default(),
            bias: state.bias,
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(state.fragment_entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(state.blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Uploads one mesh pass, returning `None` when the pass has no drawable
/// geometry.
///
/// Both slices are required: wgpu rejects a zero-sized buffer, and a malformed
/// pass with indices but no vertices cannot be drawn. Valid `umicraft-mesh`
/// output always has four vertices per face.
pub(crate) fn create_gpu_mesh_pass(
    device: &wgpu::Device,
    label: &str,
    vertices: &[Vertex],
    indices: &[u32],
) -> Option<GpuMeshPass> {
    if vertices.is_empty() || indices.is_empty() {
        return None;
    }
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    Some(GpuMeshPass {
        vertex_buffer,
        index_buffer,
        index_count: indices.len() as u32,
    })
}

/// Binds and draws one uploaded mesh pass.
pub(crate) fn draw_gpu_pass(pass: &mut wgpu::RenderPass<'_>, mesh: &GpuMeshPass) {
    draw_gpu_pass_buffers(
        pass,
        &mesh.vertex_buffer,
        &mesh.index_buffer,
        mesh.index_count,
    );
}

/// Binds and draws the first `index_count` indices of two mesh buffers.
pub(crate) fn draw_gpu_pass_buffers(
    pass: &mut wgpu::RenderPass<'_>,
    vertices: &wgpu::Buffer,
    indices: &wgpu::Buffer,
    index_count: u32,
) {
    pass.set_vertex_buffer(0, vertices.slice(..));
    pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..index_count, 0, 0..1);
}

/// Builds a UI overlay pipeline over [`OverlayVertex`] quads with `blend`.
pub(crate) fn create_overlay_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    bind_group_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    blend: wgpu::BlendState,
) -> wgpu::RenderPipeline {
    create_overlay_pipeline_with_entry(device, shader, bind_group_layout, format, blend, "fs_main")
}

/// [`create_overlay_pipeline`] with a named fragment entry point of `shader`.
pub(super) fn create_overlay_pipeline_with_entry(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    bind_group_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    blend: wgpu::BlendState,
    fragment_entry: &str,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("overlay pipeline layout"),
        bind_group_layouts: &[Some(bind_group_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("overlay pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<OverlayVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &OVERLAY_ATTRIBUTES,
            })],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment_entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

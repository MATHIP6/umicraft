// SPDX-License-Identifier: MIT OR Apache-2.0

//! Name tags drawn in the world pass with the scene's depth buffer.
//!
//! Vanilla draws a standing player's tag twice (MCP `Render.renderLivingLabel`):
//! the box and a faint copy of the text with the depth test off, so they show
//! through terrain, then the text again at full strength with the depth test
//! on. A crouching player's tag (`RendererLivingEntity.renderName`) is only the
//! box and the faint text, with the depth test on. The UI overlay has no depth
//! test, so the tags come here instead: each one is a flat quad at the depth
//! of its anchor, which is what a billboard is.

use std::ops::Range;

use umicraft_ui::{DrawList, FontAtlas};
use wgpu::util::DeviceExt;

use crate::overlay::OverlayVertex;
use crate::renderer::{DEPTH_FORMAT, OVERLAY_ATTRIBUTES};
use crate::ui_pass::UiPass;

/// One tag: what is drawn regardless of the scene, what is drawn only where
/// nothing hides it, and the depth (`0..1`) of the anchor. The lists are in
/// window pixels and hold rectangles and text only.
#[derive(Debug, Clone, Default)]
pub struct Nametag {
    /// Drawn without a depth test.
    pub always: DrawList,
    /// Drawn where the tag is not behind the scene.
    pub tested: DrawList,
    /// Normalised device depth of the anchor, as the world pass writes it.
    pub depth: f32,
}

/// The vertex ranges of one tag in the shared buffer.
struct Span {
    always: Range<u32>,
    tested: Range<u32>,
}

/// The depth-tested pipeline and the buffers of the tags of the frame.
pub(crate) struct NametagRenderer {
    pipeline: wgpu::RenderPipeline,
    scratch: UiPass,
    vertices: Option<wgpu::Buffer>,
    /// One depth per tag, read per instance.
    depths: Option<wgpu::Buffer>,
    spans: Vec<Span>,
}

impl NametagRenderer {
    pub(crate) fn new(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("nametag pipeline layout"),
            bind_group_layouts: &[Some(layout)],
            immediate_size: 0,
        });
        let depth_attribute = [wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32,
            offset: 0,
            shader_location: 3,
        }];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("nametag pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_tag"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<OverlayVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &OVERLAY_ATTRIBUTES,
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: 4,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &depth_attribute,
                    }),
                ],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            scratch: UiPass::new(1, 1),
            vertices: None,
            depths: None,
            spans: Vec::new(),
        }
    }

    /// Replaces the tags of the frame; `size` is the window in pixels.
    ///
    /// The buffers are made anew each frame: a few tags are a few hundred
    /// vertices, and the buffer of the overlay is not shared on purpose.
    pub(crate) fn set(
        &mut self,
        device: &wgpu::Device,
        tags: &[Nametag],
        size: (u32, u32),
        atlas: &FontAtlas,
    ) {
        self.spans.clear();
        self.vertices = None;
        self.depths = None;
        if tags.is_empty() {
            return;
        }
        self.scratch.resize(size.0, size.1);
        let mut vertices: Vec<OverlayVertex> = Vec::new();
        let mut depths: Vec<f32> = Vec::with_capacity(tags.len());
        for tag in tags {
            let mut append = |list: &DrawList| {
                self.scratch.rebuild(list, atlas);
                let first = vertices.len() as u32;
                vertices.extend_from_slice(self.scratch.vertices());
                first..vertices.len() as u32
            };
            let always = append(&tag.always);
            let tested = append(&tag.tested);
            self.spans.push(Span { always, tested });
            depths.push(tag.depth);
        }
        // Tags clipped away entirely leave nothing to bind (an empty buffer
        // slice is invalid).
        if vertices.is_empty() {
            self.spans.clear();
            return;
        }
        let buffer = |label, contents: &[u8]| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage: wgpu::BufferUsages::VERTEX,
            })
        };
        self.vertices = Some(buffer("nametag vertices", bytemuck::cast_slice(&vertices)));
        self.depths = Some(buffer("nametag depths", bytemuck::cast_slice(&depths)));
    }

    /// Draws the tags: `always` with the overlay pipeline, `tested` with the
    /// depth-tested one, both over the UI font atlas.
    pub(crate) fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        overlay_pipeline: &wgpu::RenderPipeline,
        font_bind_group: &wgpu::BindGroup,
    ) {
        let (Some(vertices), Some(depths)) = (&self.vertices, &self.depths) else {
            return;
        };
        pass.set_bind_group(0, font_bind_group, &[]);
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_vertex_buffer(1, depths.slice(..));
        for (index, span) in self.spans.iter().enumerate() {
            let index = index as u32;
            if !span.always.is_empty() {
                pass.set_pipeline(overlay_pipeline);
                pass.draw(span.always.clone(), 0..1);
            }
            if !span.tested.is_empty() {
                pass.set_pipeline(&self.pipeline);
                pass.draw(span.tested.clone(), index..index + 1);
            }
        }
    }
}

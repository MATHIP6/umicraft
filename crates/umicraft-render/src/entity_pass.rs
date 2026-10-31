// SPDX-License-Identifier: MIT OR Apache-2.0

//! Entity GPU rendering pass: vertex definitions, shader pipeline, textures and drawing.

use bytemuck::{Pod, Zeroable};

/// Entity vertex format consumed by `entity.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct EntityVertex {
    /// World-space vertex position.
    pub position: [f32; 3],
    /// Normal vector.
    pub normal: [f32; 3],
    /// Texture coordinates (0.0 to 1.0).
    pub uv: [f32; 2],
    /// Packed block and sky light nibbles (bits 0..=7); bit 8 tints the
    /// vertex red for the hurt flash, bits 9..=12 are the white flash level
    /// of a fusing creeper, bit 13 enables the dye tint of bits 14..=17.
    pub light: u32,
}

/// Vertex attributes layout for [`EntityVertex`].
pub const ENTITY_ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
    0 => Float32x3,
    1 => Float32x3,
    2 => Float32x2,
    3 => Uint32
];

/// A standalone GPU texture for an entity (skin or mob texture).
pub struct EntityTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

impl EntityTexture {
    /// Uploads an RGBA8 texture to the GPU.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> Self {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("entity texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("entity texture bind group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        Self {
            texture,
            view,
            bind_group,
        }
    }

    /// Replaces the pixels of the texture; `rgba` must hold exactly the
    /// `width * height * 4` bytes the texture was created with, otherwise
    /// the upload is skipped.
    pub fn write(&self, queue: &wgpu::Queue, rgba: &[u8]) {
        let size = self.texture.size();
        if rgba.len() as u64 != u64::from(size.width) * u64::from(size.height) * 4 {
            return;
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size.width * 4),
                rows_per_image: Some(size.height),
            },
            size,
        );
    }

    /// Access to bind group.
    #[must_use]
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    /// Access to underlying texture.
    #[must_use]
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    /// Access to texture view.
    #[must_use]
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }
}

/// A draw call batch sharing one entity texture.
pub struct EntityBatch {
    /// Texture bind group.
    pub bind_group: wgpu::BindGroup,
    /// Vertices to render.
    pub vertices: Vec<EntityVertex>,
}

/// The entity rendering pipeline.
pub struct EntityRenderer {
    pipeline: wgpu::RenderPipeline,
    /// The additive pipeline of glowing layers (eyes, charge, enchantment).
    additive_pipeline: wgpu::RenderPipeline,
    /// The additive pipeline of the enchantment glint, which only draws where
    /// the armour wrote its depth (`depthFunc(GL_EQUAL)`).
    glint_pipeline: wgpu::RenderPipeline,
    /// The translucent pipeline of invisible entities seen by a spectator.
    ghost_pipeline: wgpu::RenderPipeline,
    /// The pipeline of the shadows under the entities.
    shadow_pipeline: wgpu::RenderPipeline,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// The same filtering with the texture repeating, for scrolling layers.
    repeat_sampler: wgpu::Sampler,
}

impl EntityRenderer {
    /// Creates the entity rendering pipeline.
    pub fn new(
        device: &wgpu::Device,
        camera_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("entity shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("entity.wgsl").into()),
        });

        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("entity texture bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("entity pipeline layout"),
            bind_group_layouts: &[Some(camera_layout), Some(&texture_layout)],
            immediate_size: 0,
        });

        let pipeline = build_pipeline(
            device,
            &pipeline_layout,
            &shader,
            format,
            PipelineKind {
                label: "entity pipeline",
                vertex: "vs_main",
                fragment: "fs_main",
                blend: wgpu::BlendState::ALPHA_BLENDING,
                depth_write: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
            },
        );
        let additive_pipeline = build_pipeline(
            device,
            &pipeline_layout,
            &shader,
            format,
            PipelineKind {
                label: "entity additive pipeline",
                vertex: "vs_additive",
                fragment: "fs_additive",
                blend: ADDITIVE,
                depth_write: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
            },
        );
        let ghost_pipeline = build_pipeline(
            device,
            &pipeline_layout,
            &shader,
            format,
            PipelineKind {
                label: "entity ghost pipeline",
                vertex: "vs_main",
                fragment: "fs_ghost",
                blend: wgpu::BlendState::ALPHA_BLENDING,
                depth_write: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
            },
        );
        let shadow_pipeline = build_pipeline(
            device,
            &pipeline_layout,
            &shader,
            format,
            PipelineKind {
                label: "entity shadow pipeline",
                vertex: "vs_shadow",
                fragment: "fs_shadow",
                blend: wgpu::BlendState::ALPHA_BLENDING,
                depth_write: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
            },
        );
        let glint_pipeline = build_pipeline(
            device,
            &pipeline_layout,
            &shader,
            format,
            PipelineKind {
                label: "entity glint pipeline",
                vertex: "vs_additive",
                fragment: "fs_additive",
                blend: ADDITIVE,
                depth_write: false,
                depth_compare: wgpu::CompareFunction::Equal,
            },
        );

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("entity sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let repeat_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("entity repeating sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        Self {
            pipeline,
            additive_pipeline,
            glint_pipeline,
            ghost_pipeline,
            shadow_pipeline,
            texture_layout,
            sampler,
            repeat_sampler,
        }
    }

    /// Creates an entity texture on the GPU.
    pub fn create_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> EntityTexture {
        EntityTexture::new(
            device,
            queue,
            &self.texture_layout,
            &self.sampler,
            rgba,
            width,
            height,
        )
    }

    /// Creates a texture that repeats, for the layers whose coordinates scroll
    /// past the edge (the creeper charge, the wither aura, the glint).
    pub fn create_repeating_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> EntityTexture {
        EntityTexture::new(
            device,
            queue,
            &self.texture_layout,
            &self.repeat_sampler,
            rgba,
            width,
            height,
        )
    }

    /// Renders the shadows under the entities.
    pub fn render_shadows<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        camera_bind_group: &'a wgpu::BindGroup,
        batches: impl IntoIterator<Item = (&'a wgpu::Buffer, u32, &'a wgpu::BindGroup)>,
    ) {
        self.draw_batches(pass, &self.shadow_pipeline, camera_bind_group, batches);
    }

    /// Renders the invisible entities a spectator sees: translucent, drawn
    /// over the opaque ones without writing depth.
    pub fn render_ghosts<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        camera_bind_group: &'a wgpu::BindGroup,
        batches: impl IntoIterator<Item = (&'a wgpu::Buffer, u32, &'a wgpu::BindGroup)>,
    ) {
        self.draw_batches(pass, &self.ghost_pipeline, camera_bind_group, batches);
    }

    /// Renders the enchantment glint: added like the other glowing layers, but
    /// only where the armour it covers wrote the same depth.
    pub fn render_glint<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        camera_bind_group: &'a wgpu::BindGroup,
        batches: impl IntoIterator<Item = (&'a wgpu::Buffer, u32, &'a wgpu::BindGroup)>,
    ) {
        self.draw_batches(pass, &self.glint_pipeline, camera_bind_group, batches);
    }

    /// Renders the glowing layers: their colour is added to the frame without
    /// lighting and without writing depth, so they glow over the body.
    pub fn render_additive<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        camera_bind_group: &'a wgpu::BindGroup,
        batches: impl IntoIterator<Item = (&'a wgpu::Buffer, u32, &'a wgpu::BindGroup)>,
    ) {
        self.draw_batches(pass, &self.additive_pipeline, camera_bind_group, batches);
    }

    fn draw_batches<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        pipeline: &'a wgpu::RenderPipeline,
        camera_bind_group: &'a wgpu::BindGroup,
        batches: impl IntoIterator<Item = (&'a wgpu::Buffer, u32, &'a wgpu::BindGroup)>,
    ) {
        let mut set_pipeline = false;
        for (buffer, vertex_count, texture_bind_group) in batches {
            if !set_pipeline {
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, camera_bind_group, &[]);
                set_pipeline = true;
            }
            pass.set_bind_group(1, texture_bind_group, &[]);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..vertex_count, 0..1);
        }
    }

    /// Renders entity batches.
    pub fn render<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        camera_bind_group: &'a wgpu::BindGroup,
        batches: impl IntoIterator<Item = (&'a wgpu::Buffer, u32, &'a wgpu::BindGroup)>,
    ) {
        let mut set_pipeline = false;
        for (buffer, vertex_count, texture_bind_group) in batches {
            if !set_pipeline {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, camera_bind_group, &[]);
                set_pipeline = true;
            }
            pass.set_bind_group(1, texture_bind_group, &[]);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..vertex_count, 0..1);
        }
    }
}

/// Adds the source colour to the frame (`blendFunc(1, 1)`), keeping its alpha.
const ADDITIVE: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::Zero,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
};

/// What differs between the entity pipelines.
struct PipelineKind {
    label: &'static str,
    vertex: &'static str,
    fragment: &'static str,
    blend: wgpu::BlendState,
    depth_write: bool,
    depth_compare: wgpu::CompareFunction,
}

fn build_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    kind: PipelineKind,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(kind.label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(kind.vertex),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<EntityVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &ENTITY_ATTRIBUTES,
            })],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(kind.fragment),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(kind.blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: crate::renderer::DEPTH_FORMAT,
            depth_write_enabled: Some(kind.depth_write),
            depth_compare: Some(kind.depth_compare),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

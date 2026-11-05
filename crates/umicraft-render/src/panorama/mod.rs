// SPDX-License-Identifier: MIT OR Apache-2.0

//! The rotating panorama behind the title screen (`GuiMainMenu`).
//!
//! | Module | Owns |
//! |---|---|
//! | `view` | the cube geometry and the camera of `renderSkybox` |
//! | `panorama.wgsl` | the textured-quad shader |
//!
//! The six faces are six textured quads of a unit cube seen from inside, so
//! no cubemap is needed. The pass covers the target and writes depth 0, so the
//! renderer draws it right after the sky, the world passes behind it fail
//! their depth test and the menu draws on top.
//!
//! # Blur
//!
//! By default the cube is drawn the way `rotateAndBlurSkybox` does: 64
//! jittered passes and seven box-filter passes on a 256 x 256 capture (see
//! [`blur`]). [`PanoramaRenderer::set_blur`] switches back to the single
//! sharp pass straight to the screen, which has the same rotation, field of
//! view and crop.

mod blur;
mod view;

use umicraft_ui::menus::{PanoramaTextures, PanoramaView};
use wgpu::util::DeviceExt;

use crate::sky::SkyImage;
use blur::{BlurChain, PASS_UNIFORM};
use view::{clip_from_cube, cube_vertices, FaceVertex, FACE_VERTICES};

/// Number of faces of the panorama cube.
const FACES: usize = 6;

/// GPU resources of the panorama pass.
pub(crate) struct PanoramaRenderer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    faces: Option<Uploaded>,
    generation: u64,
    active: bool,
    blur: BlurChain,
    blur_enabled: bool,
    /// Distance between two pass uniforms in the uniform buffer.
    stride: u64,
}

/// The uploaded faces and the ids that were handed out for them.
struct Uploaded {
    textures: PanoramaTextures,
    bind_groups: [wgpu::BindGroup; FACES],
}

impl PanoramaRenderer {
    /// Creates the pipeline; no faces are uploaded yet.
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("panorama shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("panorama.wgsl").into()),
        });
        let layout = bind_group_layout(device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("panorama pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment);
        let stride = PASS_UNIFORM.next_multiple_of(alignment);
        Self {
            blur: BlurChain::new(device, &pipeline_layout, &shader, format),
            blur_enabled: true,
            stride,
            pipeline: pipeline(device, &pipeline_layout, &shader, format),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("panorama sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..wgpu::SamplerDescriptor::default()
            }),
            uniform_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("panorama uniforms"),
                size: stride * u64::from(blur::CUBE_PASSES),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            vertex_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("panorama cube"),
                contents: bytemuck::cast_slice(&cube_vertices()),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            layout,
            faces: None,
            generation: 0,
            active: false,
        }
    }

    /// Uploads the six faces (file order) and returns the ids the UI hands
    /// back in [`PanoramaView`]; `None` when a face is malformed. A new upload
    /// invalidates the ids of the previous one.
    pub(crate) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        images: &[SkyImage; FACES],
    ) -> Option<PanoramaTextures> {
        if !images.iter().all(SkyImage::is_valid) {
            return None;
        }
        self.generation += 1;
        let base = self.generation * FACES as u64;
        let textures = PanoramaTextures {
            faces: std::array::from_fn(|i| base + i as u64),
        };
        let bind_groups = std::array::from_fn(|i| {
            let view = crate::sky::upload(device, queue, &images[i]);
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("panorama face bind group"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.uniform_buffer,
                            offset: 0,
                            size: wgpu::BufferSize::new(PASS_UNIFORM),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            })
        });
        self.faces = Some(Uploaded {
            textures,
            bind_groups,
        });
        Some(textures)
    }

    /// Chooses what the next frame draws; `None`, or a view whose ids are not
    /// the current upload, draws nothing.
    pub(crate) fn set_view(
        &mut self,
        queue: &wgpu::Queue,
        view: Option<PanoramaView>,
        target: (u32, u32),
    ) {
        self.active = false;
        let (Some(view), Some(faces)) = (view, self.faces.as_ref()) else {
            return;
        };
        if view.textures != faces.textures {
            return;
        }
        if self.blur_enabled {
            let rotation = (view.rotation, target);
            self.blur
                .update(queue, &self.uniform_buffer, self.stride, rotation);
        } else {
            let matrix = clip_from_cube(target.0, target.1, view.rotation);
            let mut slot = [0u8; PASS_UNIFORM as usize];
            slot[..64].copy_from_slice(bytemuck::cast_slice(&matrix.to_cols_array()));
            slot[64..].copy_from_slice(bytemuck::cast_slice(&[1.0f32, 0.0, 0.0, 0.0]));
            queue.write_buffer(&self.uniform_buffer, 0, &slot);
        }
        self.active = true;
    }

    /// Chooses between the blurred capture (default) and the single sharp
    /// pass; takes effect with the next [`Self::set_view`].
    pub(crate) fn set_blur(&mut self, enabled: bool) {
        self.blur_enabled = enabled;
    }

    /// Renders the capture and its blur before the main pass; nothing without
    /// a view or with the blur off.
    pub(crate) fn prepare(&self, encoder: &mut wgpu::CommandEncoder) {
        let (true, true, Some(faces)) = (self.active, self.blur_enabled, self.faces.as_ref())
        else {
            return;
        };
        let cube = (&self.vertex_buffer, &faces.bind_groups[..], self.stride);
        self.blur.encode(encoder, cube);
    }

    /// Draws the panorama in the main pass when a view is set.
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        let (true, Some(faces)) = (self.active, self.faces.as_ref()) else {
            return;
        };
        if self.blur_enabled {
            self.blur.blit(pass);
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        for (index, bind_group) in faces.bind_groups.iter().enumerate() {
            let first = index as u32 * FACE_VERTICES;
            pass.set_bind_group(0, bind_group, &[0]);
            pass.draw(first..first + FACE_VERTICES, 0..1);
        }
    }
}

/// The bind group layout: the matrix, one face and its sampler.
fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let entry = |binding, visibility, ty| wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty,
        count: None,
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("panorama bind group layout"),
        entries: &[
            entry(
                0,
                wgpu::ShaderStages::VERTEX_FRAGMENT,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(PASS_UNIFORM),
                },
            ),
            entry(
                1,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
            ),
            entry(
                2,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            ),
        ],
    })
}

/// Opaque quads without culling that write depth 0, so the world passes drawn
/// after them are hidden; the UI draws without a depth test and goes over it.
fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("panorama pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<FaceVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &FaceVertex::ATTRIBUTES,
            })],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: crate::renderer::DEPTH_FORMAT,
            depth_write_enabled: Some(true),
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
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    /// The shader is only compiled on a GPU device; this validates it without one.
    #[test]
    fn the_panorama_shader_is_valid_wgsl() {
        let module =
            naga::front::wgsl::parse_str(include_str!("panorama.wgsl")).expect("the shader parses");
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
}

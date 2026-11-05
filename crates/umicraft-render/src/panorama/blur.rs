// SPDX-License-Identifier: MIT OR Apache-2.0

//! The blur of `GuiMainMenu.rotateAndBlurSkybox`, on a 256 x 256 target.
//!
//! Vanilla renders the cube 64 times with sub-pixel offsets at opacity
//! `255 / (j + 1)` into a 256 x 256 viewport, which averages the passes,
//! then seven times copies that picture and draws three copies shifted by one
//! texel with opacity 1, 1/2, 1/3 (a three-tap box filter; the draw is
//! quarter-turned, so the passes alternate between the two axes). Last it
//! stretches the picture over the screen.
//!
//! Here: one render pass draws the 64 cube passes into texture `A`
//! (per-pass matrices in one dynamic-offset uniform buffer), seven passes
//! ping-pong the box filter between `A` and `B` (x, y, x, ... , x), and the
//! main pass draws `B` with the crop and quarter turn of the unblurred path.
//! Cost: 384 small draws and 8 passes at 256 x 256 per title frame, see
//! `docs/perf-log.md`.

use glam::Mat4;
use umicraft_ui::menus::PanoramaRotation;
use wgpu::util::DeviceExt;

use super::view::{capture_from_cube, fit_scale, FaceVertex};

/// Edge of the capture in pixels.
pub(super) const CAPTURE: u32 = 256;
/// Cube passes per frame (`i * i` with `i = 8`).
pub(super) const CUBE_PASSES: u32 = 64;
/// Blur passes per frame (seven `rotateAndBlurSkybox` calls).
const BLUR_PASSES: u32 = 7;
/// The capture format: stored values, no sRGB (ADR-0033).
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Bytes of one cube pass uniform: the matrix and the opacity vector.
pub(super) const PASS_UNIFORM: u64 = 80;

/// The opacity of cube pass `j`: `255 / (j + 1)` in integer arithmetic.
pub(super) fn pass_alpha(j: u32) -> f32 {
    (255 / (j + 1)) as f32 / 255.0
}

/// The translation of cube pass `j` (`translate(f, f1, 0)`).
pub(super) fn pass_jitter(j: u32) -> [f32; 2] {
    let step = |n: u32| ((n as f32) / 8.0 - 0.5) / 64.0;
    [step(j % 8), step(j / 8)]
}

/// The offscreen capture, the blur pipelines and their bind groups.
pub(super) struct BlurChain {
    capture: [(wgpu::Texture, wgpu::TextureView); 2],
    cube_pipeline: wgpu::RenderPipeline,
    blur_pipeline: wgpu::RenderPipeline,
    blit_pipeline: wgpu::RenderPipeline,
    blur_groups: [wgpu::BindGroup; 2],
    blit_group: wgpu::BindGroup,
    blit_params: wgpu::Buffer,
    /// Scratch for the 64 pass uniforms, reused every frame.
    scratch: Vec<u8>,
}

impl BlurChain {
    pub(super) fn new(
        device: &wgpu::Device,
        cube_layout: &wgpu::PipelineLayout,
        cube_shader: &wgpu::ShaderModule,
        main_format: wgpu::TextureFormat,
    ) -> Self {
        let make = || {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("panorama capture"),
                size: wgpu::Extent3d {
                    width: CAPTURE,
                    height: CAPTURE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (texture, view)
        };
        let capture = [make(), make()];
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("panorama blur shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("blur.wgsl").into()),
        });
        let layout = image_layout(device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("panorama blur pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let repeat = sampler(device, wgpu::FilterMode::Nearest, wgpu::AddressMode::Repeat);
        let clamp = sampler(
            device,
            wgpu::FilterMode::Linear,
            wgpu::AddressMode::ClampToEdge,
        );
        let texel = 1.0 / CAPTURE as f32;
        let axis_x = params_buffer(device, [texel, 0.0, 0.0, 0.0]);
        let axis_y = params_buffer(device, [0.0, texel, 0.0, 0.0]);
        let blit_params = params_buffer(device, [1.0, 1.0, 0.0, 0.0]);
        let group = |view: &wgpu::TextureView, sampler: &wgpu::Sampler, buffer: &wgpu::Buffer| {
            image_group(device, &layout, view, sampler, buffer)
        };
        Self {
            blur_groups: [
                group(&capture[0].1, &repeat, &axis_x),
                group(&capture[1].1, &repeat, &axis_y),
            ],
            blit_group: group(&capture[1].1, &clamp, &blit_params),
            blit_params,
            cube_pipeline: cube_pipeline(device, cube_layout, cube_shader),
            blur_pipeline: full_pipeline(
                device,
                &pipeline_layout,
                &shader,
                "fs_blur",
                FORMAT,
                false,
            ),
            blit_pipeline: full_pipeline(
                device,
                &pipeline_layout,
                &shader,
                "fs_blit",
                main_format,
                true,
            ),
            capture,
            scratch: Vec::new(),
        }
    }

    /// Writes the 64 pass uniforms (stride `stride`) and the screen crop.
    pub(super) fn update(
        &mut self,
        queue: &wgpu::Queue,
        cube_uniforms: &wgpu::Buffer,
        stride: u64,
        (rotation, target): (PanoramaRotation, (u32, u32)),
    ) {
        self.scratch.clear();
        self.scratch
            .resize((stride * u64::from(CUBE_PASSES)) as usize, 0);
        for j in 0..CUBE_PASSES {
            let matrix: Mat4 = capture_from_cube(rotation, pass_jitter(j));
            let at = (stride * u64::from(j)) as usize;
            let slot = &mut self.scratch[at..at + PASS_UNIFORM as usize];
            slot[..64].copy_from_slice(bytemuck::cast_slice(&matrix.to_cols_array()));
            slot[64..].copy_from_slice(bytemuck::cast_slice(&[pass_alpha(j), 0.0, 0.0, 0.0]));
        }
        queue.write_buffer(cube_uniforms, 0, &self.scratch);
        let [kx, ky] = fit_scale(target.0, target.1);
        queue.write_buffer(
            &self.blit_params,
            0,
            bytemuck::cast_slice(&[kx, ky, 0.0, 0.0]),
        );
    }

    /// Renders the cube passes and the blur into the capture textures.
    pub(super) fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        cube: (&wgpu::Buffer, &[wgpu::BindGroup], u64),
    ) {
        let (vertices, faces, stride) = cube;
        {
            let mut pass = begin_capture(encoder, &self.capture[0].1, true);
            pass.set_pipeline(&self.cube_pipeline);
            pass.set_vertex_buffer(0, vertices.slice(..));
            for j in 0..CUBE_PASSES {
                for (index, bind_group) in faces.iter().enumerate() {
                    let first = index as u32 * super::view::FACE_VERTICES;
                    let offset = (stride * u64::from(j)) as u32;
                    pass.set_bind_group(0, bind_group, &[offset]);
                    pass.draw(first..first + super::view::FACE_VERTICES, 0..1);
                }
            }
        }
        for step in 0..BLUR_PASSES as usize {
            let (source, target) = (step % 2, 1 - step % 2);
            let mut pass = begin_capture(encoder, &self.capture[target].1, false);
            pass.set_pipeline(&self.blur_pipeline);
            pass.set_bind_group(0, &self.blur_groups[source], &[]);
            pass.draw(0..3, 0..1);
        }
    }

    /// Draws the blurred capture over the target of the main pass.
    pub(super) fn blit(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.blit_pipeline);
        pass.set_bind_group(0, &self.blit_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// Begins a pass into one capture texture, cleared to black or kept.
fn begin_capture<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    clear: bool,
) -> wgpu::RenderPass<'e> {
    let load = if clear {
        wgpu::LoadOp::Clear(wgpu::Color::BLACK)
    } else {
        wgpu::LoadOp::Load
    };
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("panorama capture pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

fn sampler(
    device: &wgpu::Device,
    filter: wgpu::FilterMode,
    address: wgpu::AddressMode,
) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("panorama capture sampler"),
        address_mode_u: address,
        address_mode_v: address,
        mag_filter: filter,
        min_filter: filter,
        ..wgpu::SamplerDescriptor::default()
    })
}

fn params_buffer(device: &wgpu::Device, value: [f32; 4]) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("panorama blur params"),
        contents: bytemuck::cast_slice(&value),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}

fn image_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let entry = |binding, visibility, ty| wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty,
        count: None,
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("panorama image layout"),
        entries: &[
            entry(
                0,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
            ),
            entry(
                1,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            ),
            entry(
                2,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(16),
                },
            ),
        ],
    })
}

fn image_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    params: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("panorama image group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: params.as_entire_binding(),
            },
        ],
    })
}

/// The cube pass into the capture: alpha blended, no depth.
fn cube_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("panorama capture cube pipeline"),
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
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: FORMAT,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::COLOR,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// A full-target triangle pipeline: the blur into the capture, or the blit
/// into the main pass (which also writes depth 0 like the unblurred pass).
fn full_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    fragment: &str,
    format: wgpu::TextureFormat,
    depth: bool,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("panorama full pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_full"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: depth.then(|| wgpu::DepthStencilState {
            format: crate::renderer::DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment),
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
    use super::*;

    #[test]
    fn the_first_pass_is_opaque_and_the_rest_fade_in_integer_steps() {
        assert_eq!(pass_alpha(0), 1.0);
        assert_eq!(pass_alpha(1), 127.0 / 255.0);
        assert_eq!(pass_alpha(63), 3.0 / 255.0);
    }

    #[test]
    fn the_jitter_covers_an_eight_by_eight_grid_around_the_centre() {
        assert_eq!(pass_jitter(0), [-0.5 / 64.0, -0.5 / 64.0]);
        assert_eq!(pass_jitter(63), [(7.0 / 8.0 - 0.5) / 64.0; 2]);
        assert_eq!(pass_jitter(9)[0], (1.0 / 8.0 - 0.5) / 64.0);
        assert_eq!(pass_jitter(9)[1], (1.0 / 8.0 - 0.5) / 64.0);
    }

    #[test]
    fn the_blur_shader_is_valid_wgsl() {
        let module = naga::front::wgsl::parse_str(include_str!("blur.wgsl")).expect("parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("validates");
        for entry in ["vs_full", "fs_blur", "fs_blit"] {
            assert!(module.entry_points.iter().any(|point| point.name == entry));
        }
    }
}

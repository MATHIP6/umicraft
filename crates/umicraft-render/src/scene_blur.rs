// SPDX-License-Identifier: MIT OR Apache-2.0

//! The blur of the game behind a menu (the mods menu).
//!
//! While a menu asks for it, the world is drawn into the `scene` texture
//! instead of the screen. [`SceneBlur::blur`] then smooths it at half
//! resolution (two rounds of a horizontal and a vertical Gaussian), and
//! [`SceneBlur::blit`] puts the sharp scene on the screen with the blurred one
//! over it, as opaque as the strength asks: a menu fades the blur in with it.
//! The HUD and the menu are drawn sharp on top.

use crate::renderer::DEPTH_FORMAT;

/// How far apart the taps are, in texels of the half-resolution targets.
const TAP_STEP: f32 = 1.5;

/// The size of the half-resolution targets for a `full`-pixel edge.
fn half(full: u32) -> u32 {
    (full / 2).max(1)
}

/// The textures and bind groups that depend on the window size.
struct Targets {
    scene: (wgpu::Texture, wgpu::TextureView),
    ping: (wgpu::Texture, wgpu::TextureView),
    pong: (wgpu::Texture, wgpu::TextureView),
    /// scene -> ping, ping -> pong, pong -> ping, ping -> pong.
    groups: [wgpu::BindGroup; 4],
    /// The sharp scene, for the copy onto the screen.
    scene_group: wgpu::BindGroup,
    /// pong, the blurred scene, for the mix over the sharp one.
    blurred_group: wgpu::BindGroup,
}

/// The blur pipelines and the targets they draw into.
pub(crate) struct SceneBlur {
    blur_pipeline: wgpu::RenderPipeline,
    blit_pipeline: wgpu::RenderPipeline,
    mix_pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// The tap step along x and along y, and the strength of the blur.
    params: [wgpu::Buffer; 3],
    format: wgpu::TextureFormat,
    targets: Targets,
}

impl SceneBlur {
    /// Creates the blur for a `width` x `height` screen drawn in `format`.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        (width, height): (u32, u32),
        format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene blur shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene_blur.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene blur layout"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene blur pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("scene blur sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let params = std::array::from_fn(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("scene blur params"),
                size: std::mem::size_of::<[f32; 4]>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        // `inside_main_pass`: the copy and the mix run inside the main pass,
        // which has a depth attachment; they neither test nor write it.
        let pipeline =
            |entry: &'static str, inside_main_pass: bool, blend: Option<wgpu::BlendState>| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(entry),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_full"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some(entry),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: inside_main_pass.then(|| wgpu::DepthStencilState {
                        format: DEPTH_FORMAT,
                        depth_write_enabled: Some(false),
                        depth_compare: Some(wgpu::CompareFunction::Always),
                        stencil: wgpu::StencilState::default(),
                        bias: wgpu::DepthBiasState::default(),
                    }),
                    multisample: wgpu::MultisampleState::default(),
                    multiview_mask: None,
                    cache: None,
                })
            };
        let blur_pipeline = pipeline("fs_blur", false, None);
        let blit_pipeline = pipeline("fs_blit", true, None);
        let mix_pipeline = pipeline("fs_blit_mix", true, Some(wgpu::BlendState::ALPHA_BLENDING));
        let targets = Self::targets(device, &layout, &sampler, &params, (width, height), format);
        let blur = Self {
            blur_pipeline,
            blit_pipeline,
            mix_pipeline,
            layout,
            sampler,
            params,
            format,
            targets,
        };
        blur.write_params(queue, (width, height));
        blur.set_strength(queue, 1.0);
        blur
    }

    /// Builds the textures and bind groups for a `width` x `height` screen.
    fn targets(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        params: &[wgpu::Buffer; 3],
        (width, height): (u32, u32),
        format: wgpu::TextureFormat,
    ) -> Targets {
        let make = |label: &'static str, (w, h): (u32, u32)| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (texture, view)
        };
        let small = (half(width), half(height));
        let scene = make("scene", (width.max(1), height.max(1)));
        let ping = make("scene blur ping", small);
        let pong = make("scene blur pong", small);
        let group = |view: &wgpu::TextureView, axis: usize| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("scene blur group"),
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
                        resource: params[axis].as_entire_binding(),
                    },
                ],
            })
        };
        Targets {
            groups: [
                group(&scene.1, 0),
                group(&ping.1, 1),
                group(&pong.1, 0),
                group(&ping.1, 1),
            ],
            scene_group: group(&scene.1, 2),
            blurred_group: group(&pong.1, 2),
            scene,
            ping,
            pong,
        }
    }

    /// Writes the tap steps for a `width` x `height` screen.
    fn write_params(&self, queue: &wgpu::Queue, (width, height): (u32, u32)) {
        let x = [TAP_STEP / half(width) as f32, 0.0, 0.0, 0.0];
        let y = [0.0, TAP_STEP / half(height) as f32, 0.0, 0.0];
        queue.write_buffer(&self.params[0], 0, bytemuck::cast_slice(&x));
        queue.write_buffer(&self.params[1], 0, bytemuck::cast_slice(&y));
    }

    /// Sets how much of the blurred picture shows over the sharp one, from
    /// `0.0` (none) to `1.0` (all of it).
    pub(crate) fn set_strength(&self, queue: &wgpu::Queue, strength: f32) {
        let values = [0.0, 0.0, strength.clamp(0.0, 1.0), 0.0];
        queue.write_buffer(&self.params[2], 0, bytemuck::cast_slice(&values));
    }

    /// Rebuilds the targets for a new window size.
    pub(crate) fn resize(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, size: (u32, u32)) {
        self.targets = Self::targets(
            device,
            &self.layout,
            &self.sampler,
            &self.params,
            size,
            self.format,
        );
        self.write_params(queue, size);
    }

    /// The texture the world is drawn into while the blur is on.
    pub(crate) fn scene_view(&self) -> &wgpu::TextureView {
        &self.targets.scene.1
    }

    /// Blurs the scene: two rounds of a horizontal and a vertical pass.
    pub(crate) fn blur(&self, encoder: &mut wgpu::CommandEncoder) {
        let targets = &self.targets;
        let steps = [
            (&targets.groups[0], &targets.ping.1),
            (&targets.groups[1], &targets.pong.1),
            (&targets.groups[2], &targets.ping.1),
            (&targets.groups[3], &targets.pong.1),
        ];
        for (group, target) in steps {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene blur"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.blur_pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    /// Draws the sharp scene over the target of `pass`, then the blurred one
    /// over it as opaque as the strength says.
    pub(crate) fn blit<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        pass.set_pipeline(&self.blit_pipeline);
        pass.set_bind_group(0, &self.targets.scene_group, &[]);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&self.mix_pipeline);
        pass.set_bind_group(0, &self.targets.blurred_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shader is only compiled on a GPU device; this parses and validates
    /// it without one.
    #[test]
    fn the_scene_blur_shader_is_valid_wgsl() {
        let module = naga::front::wgsl::parse_str(include_str!("scene_blur.wgsl"))
            .expect("the shader parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("the shader validates");
        let entries: Vec<_> = module
            .entry_points
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        for name in ["vs_full", "fs_blur", "fs_blit", "fs_blit_mix"] {
            assert!(entries.contains(&name), "{name}");
        }
    }

    #[test]
    fn the_blur_targets_are_half_the_screen_and_never_empty() {
        assert_eq!(half(1920), 960);
        assert_eq!(half(1), 1);
        assert_eq!(half(0), 1);
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The blur of the game behind a menu (the mods menu).
//!
//! While a menu asks for it, the world is drawn into the `scene` texture
//! instead of the screen. [`SceneBlur::blur`] then smooths it with a dual
//! filter (Kawase): three downsamples to a half, a quarter and an eighth of the
//! screen, then two upsamples back to a half. [`SceneBlur::blit`] puts the
//! sharp scene on the screen with the blurred one over it, graded (darker and
//! a little less saturated, with a vignette on the overlay) and as opaque as
//! the strength asks: a menu fades the blur in with it. The HUD and the menu
//! are drawn sharp on top.
//!
//! The look follows the mods menu design spec (section 3.2): about 8 px of
//! blur and brightness x0.62 on the overlay, about 10 px and x0.55 on the menu
//! pages, saturation x0.85 on both.

use crate::renderer::DEPTH_FORMAT;

/// The blur radius, in pixels, that a tap offset of `1.0` gives with three
/// downsamples.
const BASE_RADIUS: f32 = 8.0;
/// Passes of the filter: three down, two up.
const PASSES: usize = 5;
/// Where the strength and the grading are kept, after the passes.
const MIX: usize = PASSES;
/// The window size, in pixels, the design was drawn for.
const REFERENCE: (f32, f32) = (1280.0, 720.0);
/// The range the size of the blur follows the window within.
const SCALE_RANGE: (f32, f32) = (0.75, 2.0);

/// How the blurred picture looks and how much of it shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackgroundBlur {
    /// How much of the blurred picture shows over the sharp one, `0.0` to `1.0`.
    pub strength: f32,
    /// The overlay (the landing page) is blurred less, dimmed less and gets a
    /// vignette; the pages of the menu are blurred more and dimmed more.
    pub overlay: bool,
}

/// What the shader needs, resolved from a [`BackgroundBlur`] and the window.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Look {
    strength: f32,
    /// Tap offset of the filter, `1.0` for [`BASE_RADIUS`] pixels.
    offset: f32,
    brightness: f32,
    saturation: f32,
    /// `1.0` with the vignette, else `0.0`.
    vignette: f32,
}

impl Look {
    /// The look of `blur` in a window of `size` pixels: the radius grows with
    /// the window like the rest of the design, within [`SCALE_RANGE`].
    fn resolve(blur: BackgroundBlur, size: (u32, u32)) -> Self {
        let ratio = (size.0 as f32 / REFERENCE.0).min(size.1 as f32 / REFERENCE.1);
        let scale = ratio.clamp(SCALE_RANGE.0, SCALE_RANGE.1);
        let (radius, brightness, vignette) = if blur.overlay {
            (8.0, 0.62, 1.0)
        } else {
            (10.0, 0.55, 0.0)
        };
        Self {
            strength: blur.strength.clamp(0.0, 1.0),
            offset: radius * scale / BASE_RADIUS,
            brightness,
            saturation: 0.85,
            vignette,
        }
    }
}

/// The size of the half-resolution targets for a `full`-pixel edge.
fn half(full: u32) -> u32 {
    (full / 2).max(1)
}

/// The textures and bind groups that depend on the window size.
struct Targets {
    scene: (wgpu::Texture, wgpu::TextureView),
    /// The half, quarter and eighth of the screen going down, then the quarter
    /// and the half going up; the last one is the blurred scene.
    levels: [(wgpu::Texture, wgpu::TextureView); PASSES],
    /// The source of each pass: the scene, then every level but the last.
    groups: [wgpu::BindGroup; PASSES],
    /// The sharp scene, for the copy onto the screen.
    scene_group: wgpu::BindGroup,
    /// The blurred scene, for the mix over the sharp one.
    blurred_group: wgpu::BindGroup,
}

/// The size of each level of the filter for a `size` screen, in the order of
/// [`Targets::levels`].
fn level_sizes((width, height): (u32, u32)) -> [(u32, u32); PASSES] {
    let first = (half(width), half(height));
    let second = (half(first.0), half(first.1));
    let third = (half(second.0), half(second.1));
    [first, second, third, second, first]
}

/// The blur pipelines and the targets they draw into.
pub(crate) struct SceneBlur {
    down_pipeline: wgpu::RenderPipeline,
    up_pipeline: wgpu::RenderPipeline,
    blit_pipeline: wgpu::RenderPipeline,
    mix_pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// One per pass, then the strength and the grading.
    params: [wgpu::Buffer; PASSES + 1],
    format: wgpu::TextureFormat,
    size: (u32, u32),
    look: Look,
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
                size: std::mem::size_of::<[f32; 8]>() as u64,
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
        let down_pipeline = pipeline("fs_down", false, None);
        let up_pipeline = pipeline("fs_up", false, None);
        let blit_pipeline = pipeline("fs_blit", true, None);
        let mix_pipeline = pipeline("fs_blit_mix", true, Some(wgpu::BlendState::ALPHA_BLENDING));
        let targets = Self::targets(device, &layout, &sampler, &params, (width, height), format);
        let look = Look::resolve(
            BackgroundBlur {
                strength: 1.0,
                overlay: false,
            },
            (width, height),
        );
        let blur = Self {
            down_pipeline,
            up_pipeline,
            blit_pipeline,
            mix_pipeline,
            layout,
            sampler,
            params,
            format,
            size: (width, height),
            look,
            targets,
        };
        blur.write_params(queue);
        blur
    }

    /// Builds the textures and bind groups for a `width` x `height` screen.
    fn targets(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        params: &[wgpu::Buffer; PASSES + 1],
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
        let scene = make("scene", (width.max(1), height.max(1)));
        let levels = level_sizes((width, height)).map(|size| make("scene blur level", size));
        let group = |view: &wgpu::TextureView, buffer: usize| {
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
                        resource: params[buffer].as_entire_binding(),
                    },
                ],
            })
        };
        let groups = std::array::from_fn(|pass| {
            let view = if pass == 0 {
                &scene.1
            } else {
                &levels[pass - 1].1
            };
            group(view, pass)
        });
        Targets {
            groups,
            scene_group: group(&scene.1, MIX),
            blurred_group: group(&levels[PASSES - 1].1, MIX),
            scene,
            levels,
        }
    }

    /// Writes what every pass and the mix need: the half texel of the source
    /// of each pass and the tap offset, then the strength and the grading.
    fn write_params(&self, queue: &wgpu::Queue) {
        let (width, height) = self.size;
        let sources = [(width.max(1), height.max(1))]
            .into_iter()
            .chain(level_sizes(self.size).into_iter().take(PASSES - 1));
        for (buffer, (w, h)) in self.params.iter().zip(sources) {
            let values = [
                0.5 / w as f32,
                0.5 / h as f32,
                0.0,
                self.look.offset,
                0.0,
                0.0,
                0.0,
                0.0,
            ];
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(&values));
        }
        let look = self.look;
        let mix = [
            0.0,
            0.0,
            look.strength,
            0.0,
            look.brightness,
            look.saturation,
            look.vignette,
            0.0,
        ];
        queue.write_buffer(&self.params[MIX], 0, bytemuck::cast_slice(&mix));
    }

    /// Sets the look of the blur: how much of it shows, and whether it is the
    /// overlay's or a page's.
    pub(crate) fn set_look(&mut self, queue: &wgpu::Queue, blur: BackgroundBlur) {
        let look = Look::resolve(blur, self.size);
        if look != self.look {
            self.look = look;
            self.write_params(queue);
        }
    }

    /// Rebuilds the targets for a new window size.
    pub(crate) fn resize(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, size: (u32, u32)) {
        self.size = size;
        self.targets = Self::targets(
            device,
            &self.layout,
            &self.sampler,
            &self.params,
            size,
            self.format,
        );
        self.write_params(queue);
    }

    /// The texture the world is drawn into while the blur is on.
    pub(crate) fn scene_view(&self) -> &wgpu::TextureView {
        &self.targets.scene.1
    }

    /// Blurs the scene: three downsamples and two upsamples.
    pub(crate) fn blur(&self, encoder: &mut wgpu::CommandEncoder) {
        let targets = &self.targets;
        for (pass_index, (group, level)) in targets.groups.iter().zip(&targets.levels).enumerate() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene blur"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &level.1,
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
            let going_down = pass_index < 3;
            pass.set_pipeline(if going_down {
                &self.down_pipeline
            } else {
                &self.up_pipeline
            });
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
        for name in ["vs_full", "fs_down", "fs_up", "fs_blit", "fs_blit_mix"] {
            assert!(entries.contains(&name), "{name}");
        }
    }

    #[test]
    fn the_blur_targets_are_half_the_screen_and_never_empty() {
        assert_eq!(half(1920), 960);
        assert_eq!(half(1), 1);
        assert_eq!(half(0), 1);
    }

    #[test]
    fn the_levels_halve_down_to_an_eighth_and_come_back_up() {
        assert_eq!(
            level_sizes((1280, 720)),
            [(640, 360), (320, 180), (160, 90), (320, 180), (640, 360)]
        );
        assert_eq!(level_sizes((1, 1)), [(1, 1); PASSES]);
    }

    #[test]
    fn the_overlay_and_the_pages_follow_the_design_values() {
        let reference = (1280, 720);
        let overlay = Look::resolve(
            BackgroundBlur {
                strength: 2.0,
                overlay: true,
            },
            reference,
        );
        assert_eq!(overlay.strength, 1.0, "the strength is clamped");
        assert_eq!(overlay.offset, 1.0, "8 px is the base radius");
        assert_eq!((overlay.brightness, overlay.saturation), (0.62, 0.85));
        assert_eq!(overlay.vignette, 1.0);
        let page = Look::resolve(
            BackgroundBlur {
                strength: 1.0,
                overlay: false,
            },
            reference,
        );
        assert_eq!(page.offset, 1.25, "10 px");
        assert_eq!(page.brightness, 0.55);
        assert_eq!(page.vignette, 0.0);
    }

    #[test]
    fn the_blur_grows_with_the_window_within_the_design_range() {
        let page = BackgroundBlur {
            strength: 1.0,
            overlay: false,
        };
        let small = Look::resolve(page, (320, 180)).offset;
        let huge = Look::resolve(page, (7680, 4320)).offset;
        assert_eq!(small, 1.25 * 0.75);
        assert_eq!(huge, 1.25 * 2.0);
    }
}

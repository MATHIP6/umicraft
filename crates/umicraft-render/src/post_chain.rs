// SPDX-License-Identifier: MIT OR Apache-2.0

//! The post-processing passes of mods (the shader contract of the modding API).
//!
//! A pass is a fragment shader that turns the picture of the world into a new
//! one. The chain runs **after the hand** is drawn (a held item is part of the
//! picture a player expects a filter to cover) and **before the blurs** (the menu
//! blur and the motion blur read the already filtered picture, as they would read
//! the plain one), while the screen overlays and the HUD are drawn afterwards,
//! sharp: a filter must not make the interface unreadable, and a mod cannot read
//! or change it.
//!
//! With at least one enabled pass the world goes into the `scene` texture of
//! [`crate::scene_blur`]. The passes ping-pong between it and one extra target of
//! the same size so that the result always ends in the scene texture; an odd
//! number of passes needs one final copy. The CPU decisions live in [`plan`] and
//! carry the tests. With no enabled pass nothing here runs and the extra target
//! does not exist, so the frame is the one of a renderer without mods.
//!
//! Every pass also reads the scene depth buffer (`umi_depth`, API 1.1). The post
//! passes are encoded after the main pass that writes the depth has ended and carry
//! no depth attachment themselves, so the texture is only ever sampled while they
//! run; the depth stays intact for the passes drawn over the screen afterwards.
//! Without a depth view a 1x1 placeholder is bound and `umi.depth.z` is 0.
//!
//! The WGSL prelude (bindings, uniform, the `umi_vs` vertex shader) is owned by
//! `umicraft-modhost`, which hands the composed source over. A source that the
//! GPU refuses is kept as a failed pass: it is never drawn and the error comes
//! back from [`PostChain::set_passes`].

use std::time::Instant;

use bytemuck::{Pod, Zeroable};

use crate::camera::{FAR, NEAR};

/// A post pass to compile: the composed WGSL (prelude included) of a mod.
#[derive(Debug, Clone)]
pub struct PostPassSource {
    /// A name for GPU debug tools and logs.
    pub label: String,
    /// The whole module: the host's prelude and the mod's `fs_main`.
    pub wgsl: String,
}

/// What a pass is given each frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PostPassFrame {
    /// Whether the pass runs this frame (its mod is enabled).
    pub enabled: bool,
    /// The values the mod set, visible to the shader as `umi.params`.
    pub params: [[f32; 4]; 4],
}

/// The uniform of a pass: the `UmiPost` struct of the prelude, 96 bytes.
///
/// `depth` closes the struct (offset 80, a 16-aligned `vec4`), so a 1.0 shader's
/// view of the earlier fields is unchanged.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct PostUniform {
    resolution: [f32; 2],
    time: f32,
    frame: u32,
    params: [[f32; 4]; 4],
    /// `x` near, `y` far, `z` 1.0 when `umi_depth` is the real scene depth, `w` unused.
    depth: [f32; 4],
}

/// Where a pass reads and writes: the `scene` texture or the extra target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot {
    Scene,
    Extra,
}

/// The pure decisions about a frame with a chain, free of any GPU object.
pub(crate) mod plan {
    use super::Slot;

    /// The slot the `index`-th enabled pass reads and the one it writes: the
    /// first reads the scene, then every pass reads what the last one wrote.
    pub(crate) fn step(index: usize) -> (Slot, Slot) {
        if index.is_multiple_of(2) {
            (Slot::Scene, Slot::Extra)
        } else {
            (Slot::Extra, Slot::Scene)
        }
    }

    /// Whether the result of `enabled` passes is in the extra target and must
    /// be copied back into the scene texture.
    pub(crate) fn needs_final_copy(enabled: usize) -> bool {
        enabled % 2 == 1
    }

    /// Whether the world is drawn into the scene texture instead of the screen:
    /// any of the post chain, the motion blur and the menu blur reads it.
    pub(crate) fn world_to_scene(post: bool, motion: bool, blur: bool) -> bool {
        post || motion || blur
    }
}

/// A compiled pass and what it needs on the GPU.
struct PassGpu {
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    /// Reading the scene and reading the extra target.
    groups: Option<[wgpu::BindGroup; 2]>,
}

/// The extra full-size target of the ping-pong.
struct Extra {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

/// The post passes of the mods and the target they ping-pong through.
pub(crate) struct PostChain {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// Bound as `umi_depth` when the renderer has no depth view to offer.
    placeholder_depth: wgpu::TextureView,
    /// Whether the bound depth is the real scene depth.
    depth_valid: bool,
    format: wgpu::TextureFormat,
    size: (u32, u32),
    started: Instant,
    frame: u32,
    /// One entry per source, `None` for a source the GPU refused.
    passes: Vec<Option<PassGpu>>,
    /// Per source: compiled and enabled this frame.
    enabled: Vec<bool>,
    /// How many entries of `enabled` are set.
    active: usize,
    /// Present only while at least one pass compiled.
    extra: Option<Extra>,
}

impl PostChain {
    /// An empty chain for a `size` target of `format`.
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        size: (u32, u32),
    ) -> Self {
        Self {
            layout: bind_group_layout(device),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("post sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            placeholder_depth: create_placeholder_depth(device),
            depth_valid: false,
            format,
            size,
            started: Instant::now(),
            frame: 0,
            passes: Vec::new(),
            enabled: Vec::new(),
            active: 0,
            extra: None,
        }
    }

    /// Whether at least one pass runs this frame.
    pub(crate) fn is_active(&self) -> bool {
        self.active > 0
    }

    /// Replaces the passes. Each source is compiled inside a validation error
    /// scope, so a bad shader is reported instead of panicking; the result has
    /// one entry per source. `scene` is the view the first pass reads and `depth`
    /// the scene depth view (`None` binds the placeholder, marked invalid).
    pub(crate) fn set_passes(
        &mut self,
        device: &wgpu::Device,
        scene: &wgpu::TextureView,
        depth: Option<&wgpu::TextureView>,
        sources: &[PostPassSource],
    ) -> Vec<Result<(), String>> {
        self.passes.clear();
        let results = sources
            .iter()
            .map(|source| match self.compile(device, source) {
                Ok(pass) => {
                    self.passes.push(Some(pass));
                    Ok(())
                }
                Err(error) => {
                    self.passes.push(None);
                    Err(error)
                }
            })
            .collect();
        self.enabled.clear();
        self.enabled.resize(self.passes.len(), false);
        self.active = 0;
        self.rebuild_targets(device, scene, depth);
        results
    }

    /// Compiles one pass, returning the GPU error message if it is refused.
    fn compile(&self, device: &wgpu::Device, source: &PostPassSource) -> Result<PassGpu, String> {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let pipeline = self.create_pipeline(device, source);
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&source.label),
            size: std::mem::size_of::<PostUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        match pollster::block_on(scope.pop()) {
            Some(error) => Err(error.to_string()),
            None => Ok(PassGpu {
                pipeline,
                uniform,
                groups: None,
            }),
        }
    }

    fn create_pipeline(
        &self,
        device: &wgpu::Device,
        source: &PostPassSource,
    ) -> wgpu::RenderPipeline {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&source.label),
            source: wgpu::ShaderSource::Wgsl(source.wgsl.as_str().into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post pipeline layout"),
            bind_group_layouts: &[Some(&self.layout)],
            immediate_size: 0,
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&source.label),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("umi_vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    }

    /// Recreates the extra target and the bind groups for the current size and
    /// the (possibly new) scene and depth views. Nothing is allocated without a pass.
    fn rebuild_targets(
        &mut self,
        device: &wgpu::Device,
        scene: &wgpu::TextureView,
        depth: Option<&wgpu::TextureView>,
    ) {
        self.depth_valid = depth.is_some();
        let depth = depth.unwrap_or(&self.placeholder_depth);
        if self.passes.iter().all(Option::is_none) {
            self.extra = None;
            return;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("post extra target"),
            size: wgpu::Extent3d {
                width: self.size.0,
                height: self.size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        for pass in self.passes.iter_mut().flatten() {
            let group = |source: &wgpu::TextureView| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("post group"),
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(source),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: pass.uniform.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(depth),
                        },
                    ],
                })
            };
            pass.groups = Some([group(scene), group(&view)]);
        }
        self.extra = Some(Extra { texture, view });
    }

    /// Follows a window resize: the extra target and the groups are rebuilt
    /// around the new `scene` and `depth` views.
    pub(crate) fn resize(
        &mut self,
        device: &wgpu::Device,
        scene: &wgpu::TextureView,
        depth: Option<&wgpu::TextureView>,
        size: (u32, u32),
    ) {
        self.size = size;
        self.rebuild_targets(device, scene, depth);
    }

    /// Sets which passes run and writes their uniforms. Extra or missing
    /// `frames` mean disabled. Allocation-free.
    pub(crate) fn update(&mut self, queue: &wgpu::Queue, frames: &[PostPassFrame]) {
        self.frame = self.frame.wrapping_add(1);
        let time = self.started.elapsed().as_secs_f32();
        let resolution = [self.size.0 as f32, self.size.1 as f32];
        let depth = [NEAR, FAR, f32::from(u8::from(self.depth_valid)), 0.0];
        self.active = 0;
        for (index, enabled) in self.enabled.iter_mut().enumerate() {
            let frame = frames.get(index).filter(|frame| frame.enabled);
            let (Some(frame), Some(pass)) = (frame, self.passes[index].as_ref()) else {
                *enabled = false;
                continue;
            };
            *enabled = true;
            self.active += 1;
            let uniform = PostUniform {
                resolution,
                time,
                frame: self.frame,
                params: frame.params,
                depth,
            };
            queue.write_buffer(&pass.uniform, 0, bytemuck::bytes_of(&uniform));
        }
    }

    /// Runs the enabled passes in order; the result ends in `scene`.
    pub(crate) fn run(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        (scene, scene_view): (&wgpu::Texture, &wgpu::TextureView),
    ) {
        let (Some(extra), true) = (self.extra.as_ref(), self.is_active()) else {
            return;
        };
        let enabled = self
            .passes
            .iter()
            .zip(&self.enabled)
            .filter_map(|(pass, &enabled)| pass.as_ref().filter(|_| enabled));
        for (index, pass) in enabled.enumerate() {
            let Some(groups) = pass.groups.as_ref() else {
                continue;
            };
            let (read, write) = plan::step(index);
            let target = match write {
                Slot::Scene => scene_view,
                Slot::Extra => &extra.view,
            };
            let group = match read {
                Slot::Scene => &groups[0],
                Slot::Extra => &groups[1],
            };
            let mut render = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post pass"),
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
            render.set_pipeline(&pass.pipeline);
            render.set_bind_group(0, group, &[]);
            render.draw(0..3, 0..1);
        }
        if plan::needs_final_copy(self.active) {
            encoder.copy_texture_to_texture(
                extra.texture.as_image_copy(),
                scene.as_image_copy(),
                extra.texture.size(),
            );
        }
    }
}

/// A 1x1 depth texture bound when the scene depth cannot be.
fn create_placeholder_depth(device: &wgpu::Device) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("post placeholder depth"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// The layout of the contract: the scene, a linear sampler, the uniform and the
/// scene depth (read with `textureLoad`: a depth texture takes no filtering sampler).
fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("post layout"),
        entries: &layout_entries(),
    })
}

fn layout_entries() -> [wgpu::BindGroupLayoutEntry; 4] {
    let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty,
        count: None,
    };
    [
        entry(
            0,
            wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
        ),
        entry(
            1,
            wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        ),
        entry(
            2,
            wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<PostUniform>() as u64),
            },
        ),
        entry(
            3,
            wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
        ),
    ]
}

/// The shader prelude whose bindings this chain implements (the F3.7 contract).
///
/// The mod API composes every pass with its own copy, `umicraft_modhost::PRELUDE`;
/// a client test checks that the two are the same text, and the tests here check
/// that the layout built in Rust matches the module this prelude declares.
pub const POST_PRELUDE: &str = PRELUDE_TEXT;

const PRELUDE_TEXT: &str = r#"struct UmiPost {
    resolution: vec2<f32>,
    time: f32,
    frame: u32,
    params: array<vec4<f32>, 4>,
    depth: vec4<f32>,
};
@group(0) @binding(0) var umi_scene: texture_2d<f32>;
@group(0) @binding(1) var umi_sampler: sampler;
@group(0) @binding(2) var<uniform> umi: UmiPost;
@group(0) @binding(3) var umi_depth: texture_depth_2d;

struct UmiVertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn umi_vs(@builtin(vertex_index) index: u32) -> UmiVertexOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: UmiVertexOut;
    out.position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = uv;
    return out;
}

fn umi_linear_depth(uv: vec2<f32>) -> f32 {
    if umi.depth.z < 0.5 {
        return umi.depth.y;
    }
    let size = vec2<i32>(textureDimensions(umi_depth));
    let texel = clamp(vec2<i32>(uv * vec2<f32>(size)), vec2<i32>(0), size - vec2<i32>(1));
    let d = textureLoad(umi_depth, texel, 0);
    return umi.depth.x * umi.depth.y / (umi.depth.y - d * (umi.depth.y - umi.depth.x));
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    const GREYSCALE: &str = r#"
@fragment
fn fs_main(in: UmiVertexOut) -> @location(0) vec4<f32> {
    let color = textureSample(umi_scene, umi_sampler, in.uv);
    let grey = dot(color.rgb, vec3<f32>(0.299, 0.587, 0.114));
    return vec4<f32>(mix(color.rgb, vec3<f32>(grey), umi.params[0].x), color.a);
}
"#;

    fn module() -> naga::Module {
        let source = format!("{POST_PRELUDE}{GREYSCALE}");
        let module = naga::front::wgsl::parse_str(&source).expect("the pass parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .expect("the pass validates");
        module
    }

    #[test]
    fn the_uniform_is_96_bytes_like_the_prelude_struct() {
        assert_eq!(std::mem::size_of::<PostUniform>(), 96);
        assert_eq!(std::mem::offset_of!(PostUniform, resolution), 0);
        assert_eq!(std::mem::offset_of!(PostUniform, time), 8);
        assert_eq!(std::mem::offset_of!(PostUniform, frame), 12);
        assert_eq!(std::mem::offset_of!(PostUniform, params), 16);
        assert_eq!(std::mem::offset_of!(PostUniform, depth), 80);
    }

    #[test]
    fn the_prelude_has_the_two_entry_points() {
        let module = module();
        let names: Vec<_> = module
            .entry_points
            .iter()
            .map(|e| (e.name.as_str(), e.stage))
            .collect();
        assert_eq!(
            names,
            [
                ("umi_vs", naga::ShaderStage::Vertex),
                ("fs_main", naga::ShaderStage::Fragment)
            ]
        );
    }

    #[test]
    fn the_prelude_uniform_is_as_large_as_the_rust_struct() {
        let module = module();
        let (_, uniform) = module
            .global_variables
            .iter()
            .find(|(_, global)| global.binding.as_ref().is_some_and(|b| b.binding == 2))
            .expect("binding 2");
        let size = module.types[uniform.ty].inner.size(module.to_ctx());
        assert_eq!(size as usize, std::mem::size_of::<PostUniform>());
    }

    #[test]
    fn the_layout_matches_the_globals_of_the_module() {
        let module = module();
        let entries = layout_entries();
        assert_eq!(module.global_variables.len(), entries.len());
        for (_, global) in module.global_variables.iter() {
            let binding = global.binding.as_ref().expect("a bound global");
            assert_eq!(binding.group, 0);
            let entry = entries
                .iter()
                .find(|entry| entry.binding == binding.binding)
                .expect("a layout entry for the binding");
            let inner = &module.types[global.ty].inner;
            match (&entry.ty, inner, global.space) {
                (
                    wgpu::BindingType::Texture {
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    naga::TypeInner::Image {
                        dim: naga::ImageDimension::D2,
                        arrayed: false,
                        class:
                            naga::ImageClass::Sampled {
                                kind: naga::ScalarKind::Float,
                                multi: false,
                            },
                    },
                    naga::AddressSpace::Handle,
                ) => {}
                (
                    wgpu::BindingType::Texture {
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Depth,
                    },
                    naga::TypeInner::Image {
                        dim: naga::ImageDimension::D2,
                        arrayed: false,
                        class: naga::ImageClass::Depth { multi: false },
                    },
                    naga::AddressSpace::Handle,
                ) => {}
                (
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    naga::TypeInner::Sampler { comparison: false },
                    naga::AddressSpace::Handle,
                ) => {}
                (
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        ..
                    },
                    naga::TypeInner::Struct { .. },
                    naga::AddressSpace::Uniform,
                ) => {}
                other => panic!("binding {} mismatch: {other:?}", binding.binding),
            }
        }
    }

    /// The inverse of the DirectX-style perspective depth, as `umi_linear_depth`
    /// writes it in the prelude (kept in step with the WGSL by this test).
    fn linear_depth(depth: f32, near: f32, far: f32) -> f32 {
        near * far / (far - depth * (far - near))
    }

    #[test]
    fn the_prelude_depth_formula_inverts_the_camera_projection() {
        let projection = glam::camera::rh::proj::directx::perspective(1.0, 1.0, NEAR, FAR);
        for blocks in [NEAR, 1.0_f32, 8.0, 64.0, 300.0, FAR] {
            let clip = projection * glam::Vec4::new(0.0, 0.0, -blocks, 1.0);
            let depth = clip.z / clip.w;
            let back = linear_depth(depth, NEAR, FAR);
            assert!((back - blocks).abs() <= blocks * 1.0e-3, "{blocks}: {back}");
        }
        assert!((linear_depth(0.0, NEAR, FAR) - NEAR).abs() < 1.0e-6);
        assert!((linear_depth(1.0, NEAR, FAR) - FAR).abs() < FAR * 2.0e-3);
    }

    #[test]
    fn the_prelude_declares_the_depth_texture_and_helper() {
        let module = module();
        assert!(module
            .functions
            .iter()
            .any(|(_, f)| f.name.as_deref() == Some("umi_linear_depth")));
        let (_, depth) = module
            .global_variables
            .iter()
            .find(|(_, global)| global.binding.as_ref().is_some_and(|b| b.binding == 3))
            .expect("binding 3");
        assert_eq!(depth.name.as_deref(), Some("umi_depth"));
    }

    #[test]
    fn passes_alternate_between_the_scene_and_the_extra_target() {
        for enabled in 1..=4_usize {
            let mut written = Slot::Scene;
            for index in 0..enabled {
                let (read, write) = plan::step(index);
                assert_eq!(read, written, "pass {index} of {enabled} reads the last");
                assert_ne!(read, write);
                written = write;
            }
            let ends_in_extra = written == Slot::Extra;
            assert_eq!(plan::needs_final_copy(enabled), ends_in_extra);
        }
    }

    #[test]
    fn the_first_pass_reads_the_scene_and_one_pass_needs_the_copy() {
        assert_eq!(plan::step(0), (Slot::Scene, Slot::Extra));
        assert!(plan::needs_final_copy(1));
        assert!(!plan::needs_final_copy(2));
        assert!(plan::needs_final_copy(3));
        assert!(!plan::needs_final_copy(4));
    }

    #[test]
    fn the_world_goes_to_the_scene_when_anything_reads_it() {
        assert!(!plan::world_to_scene(false, false, false));
        for (post, motion, blur) in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
            (true, true, false),
            (true, false, true),
        ] {
            assert!(plan::world_to_scene(post, motion, blur));
        }
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The enchantment glint drawn over GUI icons.
//!
//! The original renderer re-draws an enchanted model twice with the glint
//! texture bound, additively blended, tinted `0x8040CC`, and a texture matrix
//! `scale(8) * translate(phase, 0) * rotate(angle)` (MC 1.8
//! `RenderItem.renderEffect`): layer one scrolls with period 3 s at -50
//! degrees, layer two the opposite way with period 4.873 s at 10 degrees. The
//! model's texture coordinates are atlas coordinates, so one icon pixel spans
//! `1/64` of the 64 px glint texture; the scale of 8 cancels the atlas scale
//! and leaves `uv = rotate(angle) * p + (phase, 0)` with `p` the position in
//! the icon times 0.25. This module computes those per-layer transforms
//! ([`GlintUniform`]) and owns the pipeline that applies them
//! (`gui_glint.wgsl`), masked by the icon alpha.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::renderer::create_overlay_pipeline;

/// The GUI glint shader: the shared maths (WGSL has no includes) maths followed by
/// `gui_glint.wgsl`.
pub(crate) const GUI_GLINT_WGSL: &str = concat!(
    include_str!("../glint_common.wgsl"),
    include_str!("../gui_glint.wgsl")
);

/// Scroll period and rotation (degrees) of the two layers, and the scroll
/// direction of each (MC 1.8 `RenderItem.renderEffect`).
const LAYERS: [(f32, f32, f32); 2] = [(3.0, -50.0, 1.0), (4.873, 10.0, -1.0)];

/// One glint layer's texture transform: `uv = rotation * p + offset`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct GlintLayer {
    /// Column-major 2x2 rotation matrix.
    rotation: [f32; 4],
    /// Scroll offset in `xy`; `zw` pads to 16 bytes.
    offset: [f32; 4],
}

/// The uniform buffer of `gui_glint.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct GlintUniform {
    layers: [GlintLayer; 2],
}

impl GlintUniform {
    /// The transforms of both layers `seconds` after the clock started.
    pub(crate) fn at(seconds: f32) -> Self {
        Self {
            layers: LAYERS.map(|(period, degrees, direction)| {
                let (sin, cos) = degrees.to_radians().sin_cos();
                let phase = direction * (seconds.rem_euclid(period) / period);
                GlintLayer {
                    rotation: [cos, sin, -sin, cos],
                    offset: [phase, 0.0, 0.0, 0.0],
                }
            }),
        }
    }
}

/// The glint pipeline and, once a texture was uploaded, its bindings.
pub(crate) struct GlintPass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    glint_sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
    /// `None` until [`GlintPass::set_texture`]; glint batches are skipped then.
    bind_group: Option<wgpu::BindGroup>,
}

impl GlintPass {
    /// Builds the pipeline for a render target of `format`.
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gui glint shader"),
            source: wgpu::ShaderSource::Wgsl(GUI_GLINT_WGSL.into()),
        });
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let sampler_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gui glint bind group layout"),
            entries: &[
                texture_entry(0),
                sampler_entry(1),
                texture_entry(2),
                sampler_entry(3),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
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
        // Colour adds on top of the icon; alpha is left as it is.
        let additive = wgpu::BlendState {
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
        let pipeline = create_overlay_pipeline(device, &shader, &layout, format, additive);
        // The texture tiles: it is sampled far outside 0..1 as it scrolls.
        let glint_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glint sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("glint uniform"),
            contents: bytemuck::bytes_of(&GlintUniform::at(0.0)),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            pipeline,
            layout,
            glint_sampler,
            uniform,
            bind_group: None,
        }
    }

    /// Uploads the glint texture and binds it with the icon atlas.
    ///
    /// Returns the new texture's view so the world glint can share it. A zero
    /// size or a pixel buffer that is not `width * height * 4` bytes is
    /// ignored, keeps the previous texture and returns `None`.
    pub(crate) fn set_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        icon_view: &wgpu::TextureView,
        icon_sampler: &wgpu::Sampler,
        size: (u32, u32),
        rgba: &[u8],
    ) -> Option<wgpu::TextureView> {
        let (width, height) = size;
        if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
            return None;
        }
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("glint texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            rgba,
        );
        let glint_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gui glint bind group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(icon_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(icon_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&glint_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.glint_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.uniform.as_entire_binding(),
                },
            ],
        }));
        Some(glint_view)
    }

    /// The repeating sampler of the glint texture.
    pub(crate) fn sampler(&self) -> &wgpu::Sampler {
        &self.glint_sampler
    }

    /// The buffer holding the [`GlintUniform`] of the current clock.
    pub(crate) fn uniform(&self) -> &wgpu::Buffer {
        &self.uniform
    }

    /// Writes the layer transforms for the clock reading `seconds`.
    pub(crate) fn set_time(&self, queue: &wgpu::Queue, seconds: f32) {
        queue.write_buffer(
            &self.uniform,
            0,
            bytemuck::bytes_of(&GlintUniform::at(seconds)),
        );
    }

    /// Selects the glint pipeline and bindings on `pass`.
    ///
    /// Returns `false`, leaving the pass untouched, while no glint texture was
    /// uploaded: the caller then skips the batch.
    pub(crate) fn bind(&self, pass: &mut wgpu::RenderPass<'_>) -> bool {
        let Some(bind_group) = &self.bind_group else {
            return false;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
    }

    #[test]
    fn the_uniform_is_four_vec4() {
        assert_eq!(std::mem::size_of::<GlintUniform>(), 64);
    }

    #[test]
    fn layers_rotate_by_minus_fifty_and_ten_degrees() {
        let uniform = GlintUniform::at(0.0);
        let [first, second] = uniform.layers;
        close(first.rotation[0], 50.0_f32.to_radians().cos());
        close(first.rotation[1], -50.0_f32.to_radians().sin());
        close(first.rotation[2], 50.0_f32.to_radians().sin());
        close(second.rotation[0], 10.0_f32.to_radians().cos());
        close(second.rotation[1], 10.0_f32.to_radians().sin());
    }

    #[test]
    fn layers_scroll_in_opposite_directions_with_their_own_periods() {
        let [first, second] = GlintUniform::at(1.5).layers;
        close(first.offset[0], 0.5);
        close(second.offset[0], -1.5 / 4.873);
        assert_eq!(GlintUniform::at(0.0).layers[0].offset[0], 0.0);
    }

    #[test]
    fn the_scroll_wraps_at_the_period() {
        let [first, _] = GlintUniform::at(3.75).layers;
        close(first.offset[0], 0.25);
    }

    #[test]
    fn the_glint_shader_is_valid_wgsl() {
        let module = naga::front::wgsl::parse_str(GUI_GLINT_WGSL).expect("the shader parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("the shader validates");
        for entry in ["vs_main", "fs_main"] {
            assert!(
                module.entry_points.iter().any(|point| point.name == entry),
                "missing {entry}"
            );
        }
    }
}

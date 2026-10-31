// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sky as geometry, mirroring `RenderGlobal.renderSky` and
//! `renderSkyEnd` of the 1.8 reference.
//!
//! | Module | Owns |
//! |---|---|
//! | [`state`] | [`SkyState`]: every colour and factor, pure |
//! | `dome` | the sky grid, the void plane and the black box |
//! | `stars` | the 1500-candidate star field of `Random(10842)` |
//! | `celestial` | sun, moon phases and the sunrise fan |
//! | `end` | the End cube |
//! | `textures` | [`SkyImage`], [`SkyTextures`] and the original fallbacks |
//! | `plan` | the ordered layers of a frame, as data |
//! | `gpu` | pipelines, bind layout and texture upload |
//!
//! The pass draws around the eye (the camera matrix without its translation),
//! with the depth test off and no depth write, first in the main pass. The
//! clear colour is the fog colour, so the dome fades into it at the horizon
//! exactly as the reference's linear sky fog does (`setupFog(-1)`: start 0,
//! end at the view distance).
//!
//! Clouds are not drawn here: the reference draws them after the terrain with
//! the depth test, so they have their own pass in [`crate::clouds`]. Only their
//! tint ([`SkyState::cloud_color`]) is computed with the sky.

mod celestial;
mod cloud_color;
mod dome;
mod end;
mod gpu;
mod plan;
mod stars;
mod state;
mod textures;
mod vertex;

use std::ops::Range;

use glam::Mat4;

use crate::{Camera, RenderEnvironment};
use plan::{Blend, Layer, Mesh, PlanInput, MAX_LAYERS};

pub(crate) use gpu::{sampler, upload};
pub(crate) use state::{skylight_subtracted, SkyKind};

/// The sky shader with the shared fog falloff in front.
const SKY_WGSL: &str = concat!(include_str!("../fog.wgsl"), include_str!("sky.wgsl"));
pub use state::SkyState;
pub(crate) use textures::hash;
pub use textures::{SkyImage, SkyTextures};
pub(crate) use vertex::{quad, SkyVertex};

/// Where each mesh lives in the shared vertex buffer.
struct MeshTable {
    grid: Range<u32>,
    void_box: Range<u32>,
    sun: Range<u32>,
    moons: Range<u32>,
    stars: Range<u32>,
    end_cube: Range<u32>,
    fan: Range<u32>,
}

impl MeshTable {
    /// Concatenates every static mesh into one vertex list and records the
    /// ranges; the fan is last, reserved for the per-frame rewrite.
    fn build() -> (Self, Vec<SkyVertex>) {
        let mut vertices = Vec::new();
        let mut push = |mesh: &[SkyVertex]| {
            let start = vertices.len() as u32;
            vertices.extend_from_slice(mesh);
            start..vertices.len() as u32
        };
        let table = Self {
            grid: push(&dome::grid()),
            void_box: push(&dome::box_faces()),
            sun: push(&celestial::sun()),
            moons: push(&celestial::moons()),
            stars: push(&stars::vertices()),
            end_cube: push(&end::cube()),
            fan: push(&[SkyVertex::plain([0.0; 3]); celestial::FAN_VERTICES]),
        };
        (table, vertices)
    }

    /// The vertex range of `mesh`.
    fn range(&self, mesh: Mesh) -> Range<u32> {
        match mesh {
            Mesh::Grid => self.grid.clone(),
            Mesh::VoidBox => self.void_box.clone(),
            Mesh::Sun => self.sun.clone(),
            Mesh::Moon(phase) => {
                let start = self.moons.start + u32::from(phase % 8) * 6;
                start..start + 6
            }
            Mesh::Stars => self.stars.clone(),
            Mesh::EndCube => self.end_cube.clone(),
            Mesh::Fan => self.fan.clone(),
        }
    }
}

/// GPU resources of the sky pass.
///
/// The renderer calls [`SkyRenderer::update`] once per frame, then
/// [`SkyRenderer::draw`] first inside the main render pass.
pub(crate) struct SkyRenderer {
    pipelines: [wgpu::RenderPipeline; 3],
    layout: wgpu::BindGroupLayout,
    samplers: [wgpu::Sampler; 2],
    uniform_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    meshes: MeshTable,
    /// Bind groups indexed by `Image as usize`: white, sun, moon, end sky.
    bind_groups: [wgpu::BindGroup; 4],
    /// The layers of the current frame, reused to stay allocation-free.
    layers: Vec<Layer>,
}

impl SkyRenderer {
    /// Creates the pipelines, the meshes and the fallback images for `format`.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky shader"),
            source: wgpu::ShaderSource::Wgsl(SKY_WGSL.into()),
        });
        let layout = gpu::bind_group_layout(device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipelines = [Blend::Opaque, Blend::Alpha, Blend::Additive]
            .map(|blend| gpu::pipeline(device, &pipeline_layout, &shader, format, blend));

        let (meshes, vertices) = MeshTable::build();
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky vertices"),
            size: std::mem::size_of_val(vertices.as_slice()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky uniforms"),
            size: (MAX_LAYERS * plan::SLOT_STRIDE) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let samplers = [
            gpu::sampler(device, wgpu::AddressMode::ClampToEdge),
            gpu::sampler(device, wgpu::AddressMode::Repeat),
        ];
        let bind_groups = Self::bind_groups(
            device,
            queue,
            &layout,
            &uniform_buffer,
            &samplers,
            &SkyTextures::default(),
        );
        Self {
            pipelines,
            layout,
            samplers,
            uniform_buffer,
            vertex_buffer,
            meshes,
            bind_groups,
            layers: Vec::with_capacity(MAX_LAYERS),
        }
    }

    /// Binds the white texel and the three images, each falling back to its
    /// original generated image when absent or malformed.
    fn bind_groups(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        uniforms: &wgpu::Buffer,
        samplers: &[wgpu::Sampler; 2],
        textures: &SkyTextures,
    ) -> [wgpu::BindGroup; 4] {
        let choose = |image: &Option<SkyImage>, fallback: fn() -> SkyImage| match image {
            Some(image) if image.is_valid() => image.clone(),
            _ => fallback(),
        };
        let white = SkyImage {
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        };
        let images = [
            (white, &samplers[0]),
            (choose(&textures.sun, textures::fallback_sun), &samplers[0]),
            (
                choose(&textures.moon_phases, textures::fallback_moon_phases),
                &samplers[0],
            ),
            (
                choose(&textures.end_sky, textures::fallback_end_sky),
                &samplers[1],
            ),
        ];
        images.map(|(image, sampler)| {
            let view = gpu::upload(device, queue, &image);
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("sky bind group"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: uniforms,
                            offset: 0,
                            size: wgpu::BufferSize::new(
                                std::mem::size_of::<plan::SlotUniform>() as u64
                            ),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            })
        })
    }

    /// Replaces the sun, moon and End images; absent or malformed ones keep
    /// their fallback.
    pub(crate) fn set_textures(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textures: &SkyTextures,
    ) {
        self.bind_groups = Self::bind_groups(
            device,
            queue,
            &self.layout,
            &self.uniform_buffer,
            &self.samplers,
            textures,
        );
    }

    /// Plans the frame and writes its uniforms and the sunrise fan.
    pub(crate) fn update(
        &mut self,
        queue: &wgpu::Queue,
        camera: &Camera,
        environment: RenderEnvironment,
        state: &SkyState,
    ) {
        let input = PlanInput {
            kind: state::SkyKind::of(environment.dimension),
            state,
            // The camera matrix without the eye translation: the sky is
            // drawn around the eye, so it never parallaxes.
            view_proj: camera.view_projection() * Mat4::from_translation(camera.position),
            eye_y: camera.position.y,
        };
        plan::build(&mut self.layers, &input);

        for (index, layer) in self.layers.iter().enumerate() {
            let offset = u64::from(gpu::slot_offset(index));
            queue.write_buffer(
                &self.uniform_buffer,
                offset,
                bytemuck::bytes_of(&layer.uniform),
            );
        }
        if let Some(glow) = state.sunrise {
            let fan = celestial::sunrise_fan(glow);
            let offset = u64::from(self.meshes.fan.start) * std::mem::size_of::<SkyVertex>() as u64;
            queue.write_buffer(&self.vertex_buffer, offset, bytemuck::cast_slice(&fan));
        }
    }

    /// Draws the planned layers inside the main render pass.
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.layers.is_empty() {
            return;
        }
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        for (index, layer) in self.layers.iter().enumerate() {
            // The enums are fieldless and declared in pipeline / bind group order.
            pass.set_pipeline(&self.pipelines[layer.blend as usize]);
            pass.set_bind_group(
                0,
                &self.bind_groups[layer.image as usize],
                &[gpu::slot_offset(index)],
            );
            pass.draw(self.meshes.range(layer.mesh), 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shader is only compiled on a GPU device; this parses and validates
    /// it without one.
    #[test]
    fn the_sky_shader_is_valid_wgsl() {
        let module = naga::front::wgsl::parse_str(SKY_WGSL).expect("the shader parses");
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
    fn the_static_meshes_are_laid_out_back_to_back() {
        let (table, vertices) = MeshTable::build();
        assert_eq!(table.grid.len(), dome::GRID_VERTICES);
        assert_eq!(table.moons.len(), celestial::MOON_PHASES * 6);
        assert_eq!(table.fan.end as usize, vertices.len());
        assert_eq!(table.range(Mesh::Moon(2)).len(), 6);
        assert_eq!(table.range(Mesh::Moon(2)).start, table.moons.start + 12);
        // Every vertex of the end cube is inside the table.
        assert!(table.end_cube.end <= table.fan.start);
    }
}

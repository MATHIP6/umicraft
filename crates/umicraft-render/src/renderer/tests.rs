// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the renderer.

use glam::DVec3;
use umicraft_world::ChunkPos;

use super::pipelines::*;
use super::surface::*;
use super::terrain::*;
use super::*;
use crate::camera::Camera;
use crate::lightmap::{self};
use crate::overlay::Overlay;

#[test]
fn the_vsync_option_selects_the_present_mode() {
    assert_eq!(present_mode(true), wgpu::PresentMode::AutoVsync);
    assert_eq!(present_mode(false), wgpu::PresentMode::AutoNoVsync);
}

#[test]
fn camera_uniform_is_pod_and_aligned() {
    let uniform = CameraUniform {
        view_proj: [[0.0; 4]; 4],
        camera_pos: [1.0, 2.0, 3.0],
        sun_brightness: 0.5,
        fog_color: [0.25, 0.5, 0.75],
        fog_start: 96.0,
        fog_end: 128.0,
        gamma: 0.5,
        fog_density: 0.1,
        fog_planar: 0.0,
        origin: [0.0; 3],
        light_count: 0,
        lightmap: lightmap::unlit(),
        lights: [GpuLight::NONE; MAX_DYNAMIC_LIGHTS],
    };
    let bytes = bytemuck::bytes_of(&uniform);
    assert_eq!(bytes.len(), 128 + 256 * 16 + 16 * 32);
    assert_eq!(bytes.len() % 16, 0);
    // These offsets must match the WGSL `Camera` struct in terrain.wgsl.
    assert_eq!(std::mem::offset_of!(CameraUniform, camera_pos), 64);
    assert_eq!(std::mem::offset_of!(CameraUniform, sun_brightness), 76);
    assert_eq!(std::mem::offset_of!(CameraUniform, fog_color), 80);
    assert_eq!(std::mem::offset_of!(CameraUniform, fog_start), 92);
    assert_eq!(std::mem::offset_of!(CameraUniform, fog_end), 96);
    assert_eq!(std::mem::offset_of!(CameraUniform, gamma), 100);
    assert_eq!(std::mem::offset_of!(CameraUniform, fog_density), 104);
    assert_eq!(std::mem::offset_of!(CameraUniform, origin), 112);
    assert_eq!(std::mem::offset_of!(CameraUniform, light_count), 124);
    assert_eq!(std::mem::offset_of!(CameraUniform, lightmap), 128);
    assert_eq!(std::mem::offset_of!(CameraUniform, lights), 128 + 256 * 16);
}

#[test]
fn transparent_sort_is_farthest_first() {
    let camera = DVec3::ZERO;
    let mut chunks = vec![
        ChunkDraw {
            pos: ChunkPos::new(0, 0),
            distance_sq: squared_distance_to_chunk(ChunkPos::new(0, 0), camera),
            sections: 1,
            has_solid: true,
            has_cutout: false,
        },
        ChunkDraw {
            pos: ChunkPos::new(2, 0),
            distance_sq: squared_distance_to_chunk(ChunkPos::new(2, 0), camera),
            sections: 1,
            has_solid: true,
            has_cutout: false,
        },
        ChunkDraw {
            pos: ChunkPos::new(1, 0),
            distance_sq: squared_distance_to_chunk(ChunkPos::new(1, 0), camera),
            sections: 1,
            has_solid: true,
            has_cutout: false,
        },
    ];
    sort_transparent_farthest_first(&mut chunks);
    assert_eq!(
        chunks.iter().map(|chunk| chunk.pos).collect::<Vec<_>>(),
        vec![
            ChunkPos::new(2, 0),
            ChunkPos::new(1, 0),
            ChunkPos::new(0, 0)
        ]
    );
}

#[test]
fn transparent_sort_ties_break_by_chunk_pos() {
    let camera = chunk_centre(ChunkPos::new(0, 0));
    let mut chunks = [
        ChunkPos::new(1, 0),
        ChunkPos::new(0, -1),
        ChunkPos::new(0, 1),
        ChunkPos::new(-1, 0),
    ]
    .map(|pos| ChunkDraw {
        pos,
        distance_sq: squared_distance_to_chunk(pos, camera),
        sections: 1,
        has_solid: true,
        has_cutout: false,
    });
    // Every chunk centre is exactly 16 blocks from the camera centre.
    assert!(chunks
        .iter()
        .all(|chunk| (chunk.distance_sq - 16.0 * 16.0).abs() < 0.001));
    sort_transparent_farthest_first(&mut chunks);
    assert_eq!(
        chunks.iter().map(|chunk| chunk.pos).collect::<Vec<_>>(),
        vec![
            ChunkPos::new(-1, 0),
            ChunkPos::new(0, -1),
            ChunkPos::new(0, 1),
            ChunkPos::new(1, 0),
        ]
    );
}

#[test]
fn transparent_sort_of_an_empty_list_is_a_no_op() {
    let mut chunks: Vec<ChunkDraw> = Vec::new();
    sort_transparent_farthest_first(&mut chunks);
    assert!(chunks.is_empty());
}

#[test]
fn chunk_centre_is_the_column_middle() {
    assert_eq!(
        chunk_centre(ChunkPos::new(-1, 2)),
        DVec3::new(-8.0, 128.0, 40.0)
    );
}

#[test]
fn image_is_tightly_packed() {
    let image = Image {
        width: 2,
        height: 1,
        rgba: vec![1, 2, 3, 4, 5, 6, 7, 8],
    };
    assert_eq!(image.rgba.len(), (image.width * image.height * 4) as usize);
}

#[test]
#[ignore = "requires a GPU adapter"]
fn offscreen_smoke_renders_sky_and_overlay() {
    // 70 px rows are 280 bytes, so the readback must strip WebGPU's
    // 256-byte row padding (512 bytes per row).
    let mut renderer = Renderer::new_offscreen(70, 48).expect("offscreen renderer");
    let camera = Camera::default();
    let mut overlay = Overlay::new(70, 48);
    overlay.text(2.0, 2.0, 1.0, [1.0; 4], "OK");
    renderer.render(&camera, &overlay).expect("render");
    let image = renderer.read_pixels().expect("readback");
    assert_eq!(image.rgba.len(), 70 * 48 * 4);
    let first = &image.rgba[..3];
    assert!(image.rgba.chunks_exact(4).any(|pixel| &pixel[..3] != first));
}

/// The terrain shader is only compiled on a GPU device; this parses and
/// validates it (all three fragment entry points) without one.
#[test]
fn the_terrain_shader_is_valid_wgsl() {
    let module = naga::front::wgsl::parse_str(TERRAIN_WGSL).expect("the shader parses");
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .expect("the shader validates");
    for entry in [
        "fs_main",
        "fs_solid",
        "fs_icon",
        "fs_transparent",
        "fs_crack",
    ] {
        assert!(
            module.entry_points.iter().any(|point| point.name == entry),
            "missing {entry}"
        );
    }
}

/// The byte offset of member `name` of the struct `Camera` of `source`.
fn camera_member_offset(source: &str, name: &str) -> u32 {
    let module = naga::front::wgsl::parse_str(source).expect("the shader parses");
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .expect("the shader validates");
    let (_, camera) = module
        .types
        .iter()
        .find(|(_, ty)| ty.name.as_deref() == Some("Camera"))
        .expect("a Camera struct");
    let naga::TypeInner::Struct { members, .. } = &camera.inner else {
        panic!("Camera is a struct");
    };
    members
        .iter()
        .find(|member| member.name.as_deref() == Some(name))
        .expect("the member exists")
        .offset
}

/// The terrain and entity shaders read the dynamic lights from the same
/// offsets the Rust uniform writes them at.
#[test]
fn the_shaders_read_the_dynamic_lights_where_the_uniform_puts_them() {
    for source in [TERRAIN_WGSL, crate::entity_pass::ENTITY_WGSL] {
        let count = camera_member_offset(source, "light_count");
        let lights = camera_member_offset(source, "lights");
        assert_eq!(
            count as usize,
            std::mem::offset_of!(CameraUniform, light_count)
        );
        assert_eq!(lights as usize, std::mem::offset_of!(CameraUniform, lights));
    }
}

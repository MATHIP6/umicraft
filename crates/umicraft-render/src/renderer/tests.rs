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
        _pad: 0.0,
        origin: [0.0; 3],
        _pad_origin: 0.0,
        lightmap: lightmap::unlit(),
    };
    let bytes = bytemuck::bytes_of(&uniform);
    assert_eq!(bytes.len(), 128 + 256 * 16);
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
    assert_eq!(std::mem::offset_of!(CameraUniform, lightmap), 128);
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

#[test]
#[ignore = "requires a GPU adapter"]
fn a_post_pass_filters_the_picture_and_a_bad_one_is_refused() {
    use crate::post_chain::{PostPassFrame, PostPassSource};

    // A pass that writes the red channel to all three, and one that is not WGSL.
    let grey = "struct UmiPost { resolution: vec2<f32>, time: f32, frame: u32, \
        params: array<vec4<f32>, 4>, };\n\
        @group(0) @binding(0) var umi_scene: texture_2d<f32>;\n\
        @group(0) @binding(1) var umi_sampler: sampler;\n\
        @group(0) @binding(2) var<uniform> umi: UmiPost;\n\
        struct UmiVertexOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, };\n\
        @vertex fn umi_vs(@builtin(vertex_index) index: u32) -> UmiVertexOut {\n\
            let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));\n\
            var out: UmiVertexOut;\n\
            out.position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);\n\
            out.uv = uv;\n\
            return out;\n\
        }\n\
        @fragment fn fs_main(in: UmiVertexOut) -> @location(0) vec4<f32> {\n\
            let c = textureSample(umi_scene, umi_sampler, in.uv);\n\
            return vec4<f32>(c.r, c.r, c.r, 1.0);\n\
        }\n";
    let sources = [
        PostPassSource {
            label: "grey".into(),
            wgsl: grey.into(),
        },
        PostPassSource {
            label: "broken".into(),
            wgsl: "not wgsl".into(),
        },
    ];
    let mut renderer = Renderer::new_offscreen(32, 32).expect("offscreen renderer");
    let results = renderer.set_post_passes(&sources);
    assert!(results[0].is_ok());
    assert!(results[1].is_err());
    let on = PostPassFrame {
        enabled: true,
        ..PostPassFrame::default()
    };
    renderer.update_post(&[on, on]);
    renderer
        .render(&Camera::default(), &Overlay::new(32, 32))
        .expect("render");
    let image = renderer.read_pixels().expect("readback");
    assert!(image
        .rgba
        .chunks_exact(4)
        .all(|pixel| pixel[0] == pixel[1] && pixel[1] == pixel[2]));
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_post_pass_can_read_the_scene_depth() {
    use crate::post_chain::{PostPassFrame, PostPassSource, POST_PRELUDE};

    // Shows the linear depth in blocks / 512 as grey: the empty sky reads FAR.
    let fragment = "@fragment fn fs_main(in: UmiVertexOut) -> @location(0) vec4<f32> {\n\
        let d = umi_linear_depth(in.uv) / umi.depth.y;\n\
        return vec4<f32>(d, d, d, 1.0);\n\
    }\n";
    let sources = [PostPassSource {
        label: "depth".into(),
        wgsl: format!("{POST_PRELUDE}{fragment}"),
    }];
    let mut renderer = Renderer::new_offscreen(32, 32).expect("offscreen renderer");
    assert!(renderer.set_post_passes(&sources)[0].is_ok());
    let on = PostPassFrame {
        enabled: true,
        ..PostPassFrame::default()
    };
    renderer.update_post(&[on]);
    renderer
        .render(&Camera::default(), &Overlay::new(32, 32))
        .expect("render");
    let image = renderer.read_pixels().expect("readback");
    // The sky dome is drawn at a finite depth in places, so the picture is not a
    // flat colour: depth was read, and part of it is the far plane.
    let reds: Vec<u8> = image.rgba.chunks_exact(4).map(|pixel| pixel[0]).collect();
    assert!(reds.iter().any(|&red| red >= 250), "far pixels read FAR");
    assert!(reds.iter().any(|&red| red < 250), "nearer pixels read less");
}

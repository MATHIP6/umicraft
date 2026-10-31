// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opt-in offscreen tests of the fog parity pass (P7g): the water, lava and
//! blindness fog on the sky dome and on terrain. The procedural fallback
//! images are used, so no official assets are required.
//!
//! Run with `cargo test -p umicraft-render --test offscreen_fog -- --ignored`
//! on a machine with a wgpu adapter.

use glam::Vec3;
use umicraft_mesh::{Mesh, Vertex, VertexLight};
use umicraft_render::{
    Camera, FogContext, FogMedium, Image, Overlay, RenderEnvironment, Renderer, SkyState,
};
use umicraft_world::ChunkPos;

const WIDTH: u32 = 128;
const HEIGHT: u32 = 96;

/// Renders one frame from a high vantage point, with the terrain `mesh` at
/// the origin chunk when there is one.
fn frame(environment: RenderEnvironment, mesh: Option<&Mesh>) -> Image {
    let mut renderer = Renderer::new_offscreen(WIDTH, HEIGHT).expect("offscreen renderer");
    renderer.set_environment(environment);
    if let Some(mesh) = mesh {
        renderer.upload_mesh(ChunkPos::new(0, 0), mesh);
    }
    let mut camera = Camera::new(Vec3::new(0.0, 80.0, 0.0), 0.0, 0.0);
    camera.set_aspect(WIDTH, HEIGHT);
    renderer
        .render(&camera, &Overlay::new(WIDTH, HEIGHT))
        .expect("render");
    renderer.read_pixels().expect("read pixels")
}

fn with_fog(fog: FogContext) -> RenderEnvironment {
    RenderEnvironment {
        fog,
        ..RenderEnvironment::DEFAULT
    }
}

fn medium(medium: FogMedium) -> RenderEnvironment {
    with_fog(FogContext {
        medium,
        ..FogContext::DEFAULT
    })
}

fn pixel(image: &Image, x: u32, y: u32) -> [i32; 3] {
    let at = ((y * image.width + x) * 4) as usize;
    [0, 1, 2].map(|i| i32::from(image.rgba[at + i]))
}

/// The linear clear colour as the sRGB target reads it back.
fn encoded(color: [f32; 3]) -> [i32; 3] {
    color.map(|c| (c.powf(1.0 / 2.2) * 255.0).round() as i32)
}

fn close(a: [i32; 3], b: [i32; 3], tolerance: i32) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() <= tolerance)
}

/// A wall of light-independent texels 20 blocks ahead, filling the view.
fn wall() -> Mesh {
    let light = VertexLight::new(15, 0, VertexLight::AO_OPEN, false).pack();
    let vertex = |x: f32, y: f32, uv: [f32; 2]| Vertex {
        position: [x, y, -20.0],
        normal: [0.0, 0.0, 1.0],
        uv,
        layer: 0,
        light,
        tint: umicraft_mesh::tint::WHITE,
    };
    Mesh {
        vertices: vec![
            vertex(-40.0, 40.0, [0.0, 1.0]),
            vertex(40.0, 40.0, [1.0, 1.0]),
            vertex(40.0, 120.0, [1.0, 0.0]),
            vertex(-40.0, 120.0, [0.0, 0.0]),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        ..Mesh::default()
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn water_and_lava_fill_the_view_with_their_fog_colour() {
    for liquid in [FogMedium::Water, FogMedium::Lava] {
        let environment = medium(liquid);
        let image = frame(environment, None);
        let fog = encoded(SkyState::at(environment).fog.color);
        for (x, y) in [(2, 2), (WIDTH / 2, HEIGHT / 2), (WIDTH - 3, HEIGHT - 3)] {
            let got = pixel(&image, x, y);
            assert!(close(got, fog, 6), "{liquid:?}: {got:?} vs {fog:?}");
        }
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn blindness_blackens_the_view() {
    let image = frame(
        with_fog(FogContext {
            blindness_ticks: Some(100),
            ..FogContext::DEFAULT
        }),
        Some(&wall()),
    );
    assert!(close(pixel(&image, WIDTH / 2, HEIGHT / 2), [0, 0, 0], 3));
}

#[test]
#[ignore = "requires a GPU adapter"]
fn terrain_twenty_blocks_away_is_clear_in_air_and_nearly_gone_under_water() {
    let wall = wall();
    let air = RenderEnvironment::DEFAULT;
    let water = medium(FogMedium::Water);
    let in_air = pixel(&frame(air, Some(&wall)), WIDTH / 2, HEIGHT / 2);
    let in_water = pixel(&frame(water, Some(&wall)), WIDTH / 2, HEIGHT / 2);
    let fog = encoded(SkyState::at(water).fog.color);
    let gap = |a: [i32; 3], b: [i32; 3]| (0..3).map(|i| (a[i] - b[i]).abs()).sum::<i32>();
    assert!(
        gap(in_air, encoded(SkyState::at(air).fog.color)) > 60,
        "the wall stands out in air: {in_air:?}"
    );
    assert!(
        gap(in_water, fog) < 60,
        "the wall fades into the water: {in_water:?} vs {fog:?}"
    );
}

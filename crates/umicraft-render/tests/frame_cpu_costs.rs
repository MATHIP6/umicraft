// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opt-in measurement of the per-frame CPU cost of the render calls a frame
//! makes every time (F2 phase 5): entity batches, the hand, name tags, and a
//! whole frame with a HUD-sized draw list.
//!
//! Run with
//! `cargo test -p umicraft-render --release --test frame_cpu_costs -- --ignored --nocapture`
//! on a machine with a wgpu adapter. The numbers are microseconds per call,
//! the mean of many calls; they are the baseline and the proof of the phase 5
//! changes in `docs/perf-log.md`.

use std::time::Instant;

use glam::Vec3;
use umicraft_render::{
    fallback_atlas, Camera, EntityVertex, Nametag, Overlay, RenderEnvironment, Renderer,
};
use umicraft_ui::{DrawList, Rect, TextStyle};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;
const ROUNDS: u32 = 400;

fn quad_vertices(count: usize) -> Vec<EntityVertex> {
    (0..count)
        .map(|index| EntityVertex {
            position: [index as f32 * 0.01, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 0.0],
            light: 0xFF,
        })
        .collect()
}

/// Mean microseconds of `call` over [`ROUNDS`] calls, after a warm-up.
fn micros(mut call: impl FnMut()) -> f64 {
    for _ in 0..20 {
        call();
    }
    let start = Instant::now();
    for _ in 0..ROUNDS {
        call();
    }
    start.elapsed().as_secs_f64() * 1.0e6 / f64::from(ROUNDS)
}

fn hud_list() -> DrawList {
    let mut list = DrawList::new();
    for index in 0..150 {
        let x = (index % 30) as f32 * 20.0;
        let y = (index / 30) as f32 * 20.0;
        list.rect(Rect::new(x, y, 18.0, 18.0), [0.2, 0.2, 0.2, 0.8]);
    }
    for index in 0..30 {
        list.text(
            [4.0, index as f32 * 9.0],
            "Health 20  Armor 10  Food 20",
            TextStyle::default(),
        );
    }
    list
}

#[test]
#[ignore = "requires a GPU adapter"]
fn per_frame_cpu_costs() {
    let mut renderer = Renderer::new_offscreen(WIDTH, HEIGHT).expect("offscreen renderer");
    renderer.set_environment(RenderEnvironment::new(0, 6_000, 0.0, 8));
    let skin = renderer.create_entity_texture(&[255; 64 * 64 * 4], 64, 64);
    let bind = skin.bind_group();

    // A busy frame: twelve textures of entities, a few shadows and glows.
    let big = quad_vertices(800);
    let small = quad_vertices(200);
    let twelve: Vec<(&[EntityVertex], &wgpu::BindGroup)> =
        (0..12).map(|_| (big.as_slice(), bind)).collect();
    let three: Vec<(&[EntityVertex], &wgpu::BindGroup)> =
        (0..3).map(|_| (small.as_slice(), bind)).collect();
    let two: Vec<(&[EntityVertex], &wgpu::BindGroup)> =
        (0..2).map(|_| (small.as_slice(), bind)).collect();

    let entities = micros(|| renderer.upload_entities(&twelve));
    let layers = micros(|| {
        renderer.upload_shadow_entities(&three);
        renderer.upload_additive_entities(&three);
        renderer.upload_ghost_entities(&three);
        renderer.upload_glint_entities(&three);
    });
    let hand = micros(|| renderer.set_hand_batches(&two));

    let atlas = fallback_atlas();
    renderer.set_font_atlas(&atlas).expect("font atlas");
    let mut tag_list = DrawList::new();
    tag_list.rect(Rect::new(10.0, 10.0, 60.0, 10.0), [0.0, 0.0, 0.0, 0.3]);
    tag_list.text([12.0, 11.0], "Steve", TextStyle::default());
    let tags: Vec<Nametag> = (0..8)
        .map(|index| Nametag {
            always: DrawList::new(),
            tested: tag_list.clone(),
            depth: 0.5 + index as f32 * 0.01,
        })
        .collect();
    let nametags = micros(|| renderer.set_nametags(&tags, (WIDTH, HEIGHT)));

    // A whole frame, without and with a HUD-sized draw list.
    let mut camera = Camera::new(Vec3::new(0.0, 80.0, 0.0), 0.0, -0.2);
    camera.set_aspect(WIDTH, HEIGHT);
    let overlay = Overlay::new(WIDTH, HEIGHT);
    renderer.upload_entities(&twelve);
    let plain = micros(|| {
        renderer.render(&camera, &overlay).expect("render");
    });
    let list = hud_list();
    let with_ui = micros(|| {
        renderer
            .render_with_ui(&camera, &overlay, &list, &mut umicraft_render::NoIcons)
            .expect("render");
    });

    // The tessellation alone, which a cache would save while the list is the
    // same as the last frame's.
    let mut pass = umicraft_render::UiPass::new(WIDTH, HEIGHT);
    let tessellation = micros(|| pass.rebuild(&list, &atlas));
    let unchanged = {
        let previous = list.clone();
        micros(|| {
            std::hint::black_box(previous == list);
        })
    };

    println!("UiPass::rebuild (180 commands)         {tessellation:8.1} us");
    println!("DrawList == DrawList (180 commands)    {unchanged:8.1} us");
    println!("upload_entities (12 x 800 vertices)     {entities:8.1} us");
    println!("shadow+additive+ghost+glint (4 x 3 x 200) {layers:6.1} us");
    println!("set_hand_batches (2 x 200)             {hand:8.1} us");
    println!("set_nametags (8 tags)                  {nametags:8.1} us");
    println!("render, no UI list                     {plain:8.1} us");
    println!("render_with_ui (180 commands)          {with_ui:8.1} us");
}

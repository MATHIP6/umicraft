// SPDX-License-Identifier: MIT OR Apache-2.0

//! Reproducible Criterion benchmarks for client hot paths identified in the
//! technical audit:
//! - Entity CPU meshing (`entity_render::mesh_entities` on 64 mixed mobs)
//! - HUD state cloning and immediate-mode draw list generation (`HudState::clone` + `HudCache::draw`)
//! - Rolling frametime percentile and 1% / 0.1% low FPS computation (`FrameDistribution::of`)

use std::collections::HashMap;
use std::hint::black_box;
use std::time::Instant;

use criterion::{criterion_group, criterion_main, Criterion};
use glam::{DVec3, Vec3};
use umicraft_client::entity_render::{mesh_entities, mesh_entities_into};
use umicraft_client::game::{AtomicLookSlot, Look};
use umicraft_client::hud_cache::HudCache;
use umicraft_client::input::{InputState, InputUpdate};
use umicraft_client::metrics::{FrameDistribution, InputLatencyMetrics};
use umicraft_client::ui_font;
use umicraft_game::entity::{EntityId, EntityKind, EntityRecord, EntityRenderSnapshot};
use umicraft_game::view::ViewEffects;
use umicraft_game::RenderState;
use umicraft_render::{ChatLineView, ChatRunView, HudSheets, HudState, TabEntryView, TabListView};
use umicraft_ui::DrawList;

fn sample_entities(count: usize) -> Vec<EntityRenderSnapshot> {
    let kinds = [
        EntityKind::Player,
        EntityKind::Zombie,
        EntityKind::Skeleton,
        EntityKind::Creeper,
        EntityKind::Spider,
        EntityKind::Cow,
        EntityKind::Pig,
        EntityKind::Chicken,
    ];
    (0..count)
        .map(|i| {
            let pos = DVec3::new((i % 8) as f64 * 3.0, 64.0, (i / 8) as f64 * 3.0);
            let mut record = EntityRecord::new(
                EntityId(i as i32 + 1),
                kinds[i % kinds.len()],
                pos,
                (i as f32 * 25.0) % 360.0,
                10.0,
            );
            record.previous_position = pos;
            let mut snapshot = record.frame_data().interpolate(1.0, DVec3::ZERO);
            snapshot.limb_swing = i as f32 * 0.8;
            snapshot.limb_swing_amount = 0.65;
            snapshot
        })
        .collect()
}

fn populated_hud_state() -> HudState {
    let now = Instant::now();
    let chat: Vec<ChatLineView> = (0..50)
        .map(|i| ChatLineView {
            runs: vec![
                ChatRunView {
                    text: format!("<Player{i}> "),
                    color: [255, 255, 255],
                    ..ChatRunView::default()
                },
                ChatRunView {
                    text: format!("Benchmark chat message number {i} with multiple words"),
                    color: [170, 255, 170],
                    ..ChatRunView::default()
                },
            ],
            received: now,
        })
        .collect();
    let entries: Vec<TabEntryView> = (0..40)
        .map(|i| TabEntryView {
            uuid: i as u128 + 1,
            name: vec![ChatRunView {
                text: format!("Player_{i:02}"),
                color: [255, 255, 255],
                ..ChatRunView::default()
            }],
            ping: 42,
            spectator: false,
            score: Some(i),
            has_skin: false,
            skin_url: None,
            face: None,
        })
        .collect();
    HudState {
        health: 18.0,
        food: 19.0,
        armor: 15.0,
        level: 27,
        chat,
        tab_list: Some(TabListView {
            header: None,
            footer: None,
            entries,
            value_style: None,
            show_heads: true,
        }),
        show_tab_list: true,
        now: Some(now),
        ..HudState::default()
    }
}

fn bench_entity_meshing(criterion: &mut Criterion) {
    let entities = sample_entities(64);
    criterion.bench_function("client/mesh_64_entities", |bencher| {
        bencher.iter(|| {
            let batches = mesh_entities(black_box(&entities));
            let total_verts: usize = batches.values().map(Vec::len).sum();
            black_box(total_verts)
        });
    });
    criterion.bench_function("client/mesh_64_entities_pooled", |bencher| {
        let mut pool = HashMap::new();
        mesh_entities_into(&entities, &mut pool);
        bencher.iter(|| {
            mesh_entities_into(black_box(&entities), &mut pool);
            let total_verts: usize = pool.values().map(Vec::len).sum();
            black_box(total_verts)
        });
    });
}

fn bench_hud_clone_and_cache(criterion: &mut Criterion) {
    let hud = populated_hud_state();
    let font = ui_font::load();
    let atlas = &font.atlas;
    let sheets = HudSheets::default();
    let mut group = criterion.benchmark_group("client/hud");

    group.bench_function("clone_populated_hud_state", |bencher| {
        bencher.iter(|| black_box(black_box(&hud).clone()));
    });

    group.bench_function("double_buffer_unchanged_revision", |bencher| {
        let mut frame_hud = hud.clone();
        let mut frame_rev = 1_u64;
        let world_rev = 1_u64;
        bencher.iter(|| {
            if black_box(world_rev) != frame_rev {
                frame_hud.clone_from(black_box(&hud));
                frame_rev = world_rev;
            }
            frame_hud.chat_open = black_box(false);
            frame_hud.chat_scroll = black_box(0);
            frame_hud.show_tab_list = black_box(false);
            black_box(frame_hud.health)
        });
    });

    group.bench_function("hud_cache_hit_1080p", |bencher| {
        let mut cache = HudCache::default();
        let mut list = DrawList::default();
        cache.draw(&mut list, &hud, (1920, 1080), (atlas, &sheets));
        bencher.iter(|| {
            list.clear();
            cache.draw(&mut list, black_box(&hud), (1920, 1080), (atlas, &sheets));
            black_box(list.commands().len())
        });
    });

    group.bench_function("hud_cache_miss_rebuild_1080p", |bencher| {
        let mut cache = HudCache::default();
        let mut list = DrawList::default();
        let mut varying = hud.clone();
        let mut tick = 0_u8;
        bencher.iter(|| {
            tick = tick.wrapping_add(1);
            varying.selected_slot = tick % 9;
            list.clear();
            cache.draw(
                &mut list,
                black_box(&varying),
                (1920, 1080),
                (atlas, &sheets),
            );
            black_box(list.commands().len())
        });
    });
    group.finish();
}

fn bench_frame_distribution_1000(criterion: &mut Criterion) {
    let samples: Vec<f32> = (0..1_000)
        .map(|i| 4.0 + ((i * 73) % 100) as f32 * 0.05)
        .collect();
    criterion.bench_function("client/frame_distribution_1000_samples", |bencher| {
        bencher.iter(|| black_box(FrameDistribution::of(black_box(&samples))));
    });
}

fn bench_atomic_look_slot(criterion: &mut Criterion) {
    let slot = AtomicLookSlot::default();
    let sample_look = Look {
        view: RenderState {
            position: DVec3::new(12.5, 65.0, -34.2),
            eye: DVec3::new(12.5, 66.62, -34.2),
            orbit_start: Vec3::ZERO,
            yaw: 1.25,
            pitch: -0.35,
            on_ground: true,
            effects: ViewEffects::default(),
        },
        body_pitch: -20.0,
    };
    let mut group = criterion.benchmark_group("client/look_slot");
    group.bench_function("store_and_take_lockfree", |bencher| {
        bencher.iter(|| {
            slot.store(black_box(sample_look));
            black_box(slot.take())
        });
    });
    group.bench_function("idle_poll_lockfree", |bencher| {
        let _ = slot.take();
        bencher.iter(|| black_box(slot.take()));
    });
    group.finish();
}

fn bench_input_queue(criterion: &mut Criterion) {
    let (tx, rx) = crossbeam_channel::bounded::<InputUpdate>(512);
    let sample_update = InputUpdate {
        created_at: Instant::now(),
        state: InputState::default(),
        look_yaw: 0.05,
        look_pitch: -0.02,
        select_slot: Some(2),
        hotbar_keys: [0; 9],
        break_block: true,
        place_block: false,
        pick_block: false,
        drop_item: None,
        toggle_perspective: false,
    };
    let mut group = criterion.benchmark_group("client/input_queue");
    group.bench_function("dedicated_input_send_recv_roundtrip", |bencher| {
        bencher.iter(|| {
            let _ = tx.try_send(black_box(sample_update));
            black_box(rx.try_recv())
        });
    });
    group.bench_function("burst_20_cps_clicks", |bencher| {
        bencher.iter(|| {
            for _ in 0..20 {
                let _ = tx.try_send(black_box(sample_update));
            }
            let mut count = 0;
            while rx.try_recv().is_ok() {
                count += 1;
            }
            black_box(count)
        });
    });
    let latency = InputLatencyMetrics {
        input_processing_us: 42,
        event_queue_latency_us: 15,
        input_to_frame_ms: 1.85,
        presentation_time_ms: 0.18,
    };
    group.bench_function("f3_line_format", |bencher| {
        bencher.iter(|| black_box(latency.f3_line()));
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_entity_meshing,
    bench_hud_clone_and_cache,
    bench_frame_distribution_1000,
    bench_atomic_look_slot,
    bench_input_queue
);
criterion_main!(benches);

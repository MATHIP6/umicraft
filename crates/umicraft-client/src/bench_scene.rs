// SPDX-License-Identifier: MIT OR Apache-2.0

//! The reproducible benchmark scene (`--bench-scene`, F2-0c).
//!
//! A deterministic synthetic world (hills, caves, a sea) is generated around
//! a chosen origin, meshed with the real mesher, uploaded to an offscreen
//! renderer and drawn along a scripted camera path. The report gives the
//! frame time (mean, p95, p99, max) and the GPU time per group when the
//! adapter supports timestamps.
//!
//! The terrain depends only on the position *relative to the origin chunk*,
//! so `--bench-origin 0` and `--bench-origin 1000000` draw the same geometry
//! and any difference between the two runs comes from the coordinates alone
//! (F2 phase 1). The path is a function of the frame index, never of the wall
//! clock, so two runs see the same frames. Nothing here needs a server.
//!
//! Frame time is measured around `render` plus a wait for the GPU, with no
//! vsync: it is the CPU cost and the GPU cost of one frame, whichever is
//! larger.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Instant;

use glam::{DVec3, Vec3};
use umicraft_core::profiling::{AllocDelta, AllocSnapshot};
use umicraft_game::entity::{EntityItem, EntityKind, EntityRenderSnapshot};
use umicraft_mesh::{ChunkSnapshot, Mesh};
use umicraft_render::{
    Camera, DrawCallStats, Overlay, ParticleSprite, RenderEnvironment, Renderer, VramStats, GROUP_NAMES,
};
use umicraft_world::{BlockState, ChunkColumn, ChunkPos};

use crate::args::Args;
use crate::entity_render::EntityTextures;
use crate::error::ClientError;
use crate::metrics::FrameDistribution;
use crate::process_memory::MemoryUse;
use crate::terrain_textures::{TerrainMesher, TerrainTextures};

/// The 10 reproducible benchmark scenarios for competitive comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchScenario {
    /// Default standard orbital benchmark scene.
    Standard,
    /// Scenario 1: Empty world without chunks.
    Empty,
    /// Scenario 2: World with 8 chunks view distance.
    Chunks8,
    /// Scenario 3: World with 16 chunks view distance.
    Chunks16,
    /// Scenario 4: World with 24 chunks view distance.
    Chunks24,
    /// Scenario: World with 32 chunks view distance.
    Chunks32,
    /// Scenario: World with 48 chunks view distance.
    Chunks48,
    /// Scenario: 20 animated entities.
    Entities20,
    /// Scenario 5: 50 animated entities.
    Entities50,
    /// Scenario 6: 100 animated entities.
    Entities100,
    /// Scenario 7: 200 animated entities.
    Entities200,
    /// Scenario 8: PvP combat with diamond armor, sword hits, and sparks.
    PvpCombat,
    /// Scenario 9: Heavy particle field with 4,000 active particles.
    ParticlesHeavy,
    /// Scenario 10: Mixed scene with chunks, entities, and particles.
    Mixed,
    /// Scenario A: Baseline sans météo (ciel dégagé, empty world).
    WeatherA,
    /// Scenario B: Pluie légère (strength = 0.2, fancy = true).
    WeatherB,
    /// Scenario C: Pluie normale (strength = 1.0, fancy = true).
    WeatherC,
    /// Scenario D: Pluie maximale vue dégagée (strength = 1.0, fancy = true, rayon 10).
    WeatherD,
    /// Scenario E: Neige (biome froid, temp < 0.15, wobble et flocons).
    WeatherE,
    /// Scenario F: Chunks chargés (8 chunks) + pluie normale.
    WeatherF,
    /// Scenario G: Scène mixte (chunks + 50 entités + particules + pluie normale).
    WeatherG,
}

impl BenchScenario {
    /// Parses a scenario string identifier.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "empty" | "1" => Self::Empty,
            "chunks8" | "chunks_8" | "2" => Self::Chunks8,
            "chunks16" | "chunks_16" | "3" => Self::Chunks16,
            "chunks24" | "chunks_24" | "4" => Self::Chunks24,
            "chunks32" | "chunks_32" | "32" => Self::Chunks32,
            "chunks48" | "chunks_48" | "48" => Self::Chunks48,
            "entities20" | "entities_20" | "20" => Self::Entities20,
            "entities50" | "entities_50" | "5" => Self::Entities50,
            "entities100" | "entities_100" | "6" => Self::Entities100,
            "entities200" | "entities_200" | "7" => Self::Entities200,
            "pvp" | "combat" | "pvp_combat" | "8" => Self::PvpCombat,
            "particles" | "particles_heavy" | "9" => Self::ParticlesHeavy,
            "mixed" | "10" => Self::Mixed,
            "weather_a" | "weather-a" | "weathera" | "a" => Self::WeatherA,
            "weather_b" | "weather-b" | "weatherb" | "b" => Self::WeatherB,
            "weather_c" | "weather-c" | "weatherc" | "c" => Self::WeatherC,
            "weather_d" | "weather-d" | "weatherd" | "d" => Self::WeatherD,
            "weather_e" | "weather-e" | "weathere" | "e" => Self::WeatherE,
            "weather_f" | "weather-f" | "weatherf" | "f" => Self::WeatherF,
            "weather_g" | "weather-g" | "weatherg" | "g" => Self::WeatherG,
            _ => Self::Standard,
        }
    }

    /// User-facing label of the scenario.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard Scene",
            Self::Empty => "1. Empty World",
            Self::Chunks8 => "2. World 8 Chunks",
            Self::Chunks16 => "3. World 16 Chunks",
            Self::Chunks24 => "4. World 24 Chunks",
            Self::Chunks32 => "World 32 Chunks",
            Self::Chunks48 => "World 48 Chunks",
            Self::Entities20 => "C. 20 Entities / Players",
            Self::Entities50 => "5. 50 Entities",
            Self::Entities100 => "6. 100 Entities",
            Self::Entities200 => "7. 200 Entities",
            Self::PvpCombat => "8. PvP Combat",
            Self::ParticlesHeavy => "9. Heavy Particles (4000)",
            Self::Mixed => "10. Mixed Scene",
            Self::WeatherA => "A. No Weather (Clear Baseline)",
            Self::WeatherB => "B. Light Rain (strength 0.2)",
            Self::WeatherC => "C. Normal Rain (strength 1.0, fancy)",
            Self::WeatherD => "D. Max Rain (fancy, radius 10)",
            Self::WeatherE => "E. Snow (cold biome, temp 0.0)",
            Self::WeatherF => "F. Chunks 8 + Normal Rain",
            Self::WeatherG => "G. Mixed Scene + Normal Rain",
        }
    }

    /// Returns the view distance (chunk radius) for this scenario.
    #[must_use]
    pub fn view_distance(self, default_vd: u8) -> u8 {
        match self {
            Self::Empty | Self::WeatherA | Self::WeatherB | Self::WeatherC | Self::WeatherD | Self::WeatherE => 0,
            Self::Chunks8 | Self::WeatherF | Self::WeatherG => 8,
            Self::Chunks16 => 16,
            Self::Chunks24 => 24,
            Self::Chunks32 => 32,
            Self::Chunks48 => 48,
            Self::Entities20
            | Self::Entities50
            | Self::Entities100
            | Self::Entities200
            | Self::PvpCombat
            | Self::ParticlesHeavy => 2,
            Self::Mixed => 8,
            Self::Standard => default_vd,
        }
    }
}

/// Frames drawn before the measurement starts (pipeline and driver warm-up).
const WARMUP_FRAMES: u32 = 30;
/// Frames of `--bench-stream` left out of the statistics (pipeline compilation).
const STREAM_WARMUP: usize = 5;
/// Sea level of the generated world.
const SEA_LEVEL: i32 = 62;
/// Plains biome id (grass and leaf tint).
const PLAINS: u8 = 1;
/// Height of the camera orbit above the origin plane.
const CAMERA_HEIGHT: f32 = 100.0;
/// Radius of the camera orbit in blocks.
const ORBIT_RADIUS: f32 = 40.0;
/// Full turns of the yaw during one run.
const TURNS: f32 = 2.0;

const STONE: u16 = 1;
const GRASS: u16 = 2;
const DIRT: u16 = 3;
const BEDROCK: u16 = 7;
const WATER: u16 = 9;
const SAND: u16 = 12;

/// The terrain height at `(x, z)` blocks from the origin.
///
/// Integer-valued and deterministic: smooth hills plus a small hash noise.
fn surface_height(x: i32, z: i32) -> i32 {
    let (xf, zf) = (x as f32, z as f32);
    let hills = 10.0 * (xf / 23.0).sin() * (zf / 17.0).cos() + 6.0 * ((xf + zf) / 9.0).sin();
    let noise = (x.wrapping_mul(73_856_093) ^ z.wrapping_mul(19_349_663)).rem_euclid(3);
    64 + hills.round() as i32 + noise
}

/// Whether the cell is a cave (a carved, connected pocket underground).
fn is_cave(x: i32, y: i32, z: i32, surface: i32) -> bool {
    (10..surface - 6).contains(&y)
        && (x as f32 / 7.0).sin() + (y as f32 / 5.0).sin() + (z as f32 / 9.0).sin() > 2.1
}

/// The block at a cell of the generated world.
fn block_at(x: i32, y: i32, z: i32, surface: i32) -> BlockState {
    let id = if y == 0 {
        BEDROCK
    } else if is_cave(x, y, z, surface) {
        return BlockState::AIR;
    } else if y < surface - 3 {
        STONE
    } else if y < surface {
        DIRT
    } else if y == surface {
        if surface <= SEA_LEVEL + 1 {
            SAND
        } else {
            GRASS
        }
    } else if y <= SEA_LEVEL {
        WATER
    } else {
        return BlockState::AIR;
    };
    BlockState::new(id, 0)
}

/// Generates the column at `pos`, with the terrain of the position relative
/// to `origin` (so every origin sees the same world).
fn generate_column(pos: ChunkPos, origin: ChunkPos) -> ChunkColumn {
    let mut column = ChunkColumn::new(pos);
    column.fill_biomes(PLAINS);
    let base_x = (pos.x - origin.x) * 16;
    let base_z = (pos.z - origin.z) * 16;
    for lz in 0..16_usize {
        for lx in 0..16_usize {
            let (x, z) = (base_x + lx as i32, base_z + lz as i32);
            let surface = surface_height(x, z);
            let top = surface.max(SEA_LEVEL);
            for y in 0..=top {
                column.set_block(lx, y as usize, lz, block_at(x, y, z, surface));
            }
            // Open air above the ground is lit; sections above the terrain
            // are absent and read as full sky.
            let lit_to = (top / 16 + 1) * 16;
            for y in top + 1..lit_to {
                column.set_sky_light(lx, y as usize, lz, 15);
            }
        }
    }
    column
}

/// One pose of the scripted camera.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pose {
    /// Offset from the origin, in blocks.
    offset: Vec3,
    yaw: f32,
    pitch: f32,
}

/// The pose of frame `frame` of `frames`: an orbit that turns the view twice
/// and nods slowly, so the frustum sweeps the whole scene.
fn pose_at(frame: u32, frames: u32, scenario: BenchScenario) -> Pose {
    let t = frame as f32 / frames.max(1) as f32;
    let angle = t * std::f32::consts::TAU;
    match scenario {
        BenchScenario::PvpCombat => Pose {
            offset: Vec3::new(4.5 * angle.cos(), 66.5, 4.5 * angle.sin()),
            yaw: angle.to_degrees() + 90.0,
            pitch: -0.15,
        },
        BenchScenario::Entities20
        | BenchScenario::Entities50
        | BenchScenario::Entities100
        | BenchScenario::Entities200 => Pose {
            offset: Vec3::new(24.0 * angle.cos(), 70.0, 24.0 * angle.sin()),
            yaw: angle.to_degrees() + 90.0,
            pitch: -0.22,
        },
        BenchScenario::WeatherA
        | BenchScenario::WeatherB
        | BenchScenario::WeatherC
        | BenchScenario::WeatherD
        | BenchScenario::WeatherE => Pose {
            offset: Vec3::new(0.5, 66.0, 0.5),
            yaw: (angle * 0.5).to_degrees(),
            pitch: -0.2,
        },
        BenchScenario::WeatherF | BenchScenario::WeatherG => Pose {
            offset: Vec3::new(8.0 * angle.cos(), 72.0, 8.0 * angle.sin()),
            yaw: angle.to_degrees() + 90.0,
            pitch: -0.2,
        },
        _ => Pose {
            offset: Vec3::new(
                ORBIT_RADIUS * angle.cos(),
                CAMERA_HEIGHT,
                ORBIT_RADIUS * angle.sin(),
            ),
            yaw: angle * TURNS,
            pitch: -0.3 + 0.15 * (angle * 3.0).sin(),
        },
    }
}

/// Frame-time statistics in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq)]
struct FrameStats {
    mean: f32,
    p50: f32,
    p95: f32,
    p99: f32,
    max: f32,
}

impl FrameStats {
    /// Computes the statistics of `samples`; `None` when empty.
    ///
    /// Percentiles use the nearest-rank method.
    fn of(samples: &[f32]) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }
        let mut sorted = samples.to_vec();
        sorted.sort_by(f32::total_cmp);
        let rank =
            |p: f32| sorted[((p * sorted.len() as f32).ceil() as usize).clamp(1, sorted.len()) - 1];
        Some(Self {
            mean: sorted.iter().sum::<f32>() / sorted.len() as f32,
            p50: rank(0.50),
            p95: rank(0.95),
            p99: rank(0.99),
            max: sorted[sorted.len() - 1],
        })
    }
}

/// The columns of the scene, keyed by position.
type World = HashMap<ChunkPos, Arc<ChunkColumn>>;

/// Generates the `(2 * reach + 3)²` columns around `origin`: the visible
/// ones plus the ring the mesher needs for its boundary queries.
fn generate_world(origin: ChunkPos, reach: i32) -> World {
    let span = reach + 1;
    let positions: Vec<ChunkPos> = (-span..=span)
        .flat_map(|dz| (-span..=span).map(move |dx| ChunkPos::new(origin.x + dx, origin.z + dz)))
        .collect();
    parallel_map(&positions, |pos| {
        (*pos, Arc::new(generate_column(*pos, origin)))
    })
    .into_iter()
    .collect()
}

/// Meshes every column whose 3×3 neighbourhood is generated, returning the
/// mesh and the individual build duration in microseconds.
fn mesh_world(world: &World, mesher: &TerrainMesher) -> Vec<(ChunkPos, Mesh, f32)> {
    let interior: Vec<ChunkPos> = world
        .keys()
        .copied()
        .filter(|pos| {
            (-1..=1).all(|dz| {
                (-1..=1).all(|dx| world.contains_key(&ChunkPos::new(pos.x + dx, pos.z + dz)))
            })
        })
        .collect();
    parallel_map(&interior, |pos| {
        let start = Instant::now();
        let columns = (-1..=1).flat_map(|dz| {
            (-1..=1).filter_map(move |dx| world.get(&ChunkPos::new(pos.x + dx, pos.z + dz)))
        });
        let snapshot = ChunkSnapshot::from_columns(*pos, columns.map(Arc::clone));
        let mesh = mesher.mesh(*pos, &snapshot);
        let build_us = start.elapsed().as_secs_f32() * 1_000_000.0;
        (*pos, mesh, build_us)
    })
}

/// Maps `items` across all cores using dynamic atomic work-claiming (work
/// stealing) and returns the results in input order.
///
/// Unlike static chunk slicing, dynamic index claiming prevents threads that
/// draw light outer chunks from sitting idle while another thread is stuck on
/// a cluster of dense cave/water chunks.
fn parallel_map<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use umicraft_core::CachePadded;

    if items.is_empty() {
        return Vec::new();
    }
    let threads = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(items.len());
    let next_index = CachePadded::new(AtomicUsize::new(0));
    let mut indexed: Vec<(usize, R)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                let next_ref = &next_index;
                let work_ref = &work;
                scope.spawn(move || {
                    let mut local = Vec::with_capacity(items.len().div_ceil(threads));
                    loop {
                        let idx = next_ref.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(idx) else {
                            break;
                        };
                        local.push((idx, work_ref(item)));
                    }
                    local
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap_or_default())
            .collect()
    });
    indexed.sort_unstable_by_key(|(idx, _)| *idx);
    indexed.into_iter().map(|(_, res)| res).collect()
}

/// The measurements of one run.
struct Measurement {
    frame_ms: Vec<f32>,
    /// The CPU part of each frame: `render` (encode and submit).
    render_ms: Vec<f32>,
    /// The rest of each frame: the wait for the GPU, which other programs
    /// using the GPU lengthen.
    wait_ms: Vec<f32>,
    adapter: String,
    gpu_ms: [f32; GROUP_NAMES.len()],
    gpu_samples: u32,
    alloc_delta: AllocDelta,
    draw_calls: DrawCallStats,
    vram: VramStats,
    ram: MemoryUse,
    weather_vertices: usize,
    active_particles: usize,
}

struct ScenarioContext {
    scenario: BenchScenario,
    entity_textures: Option<EntityTextures>,
    entities: Vec<EntityRenderSnapshot>,
    particles: Vec<ParticleSprite>,
    weather_strength: f32,
    weather_fancy: bool,
    weather_temp: f32,
}

fn create_bench_precipitation_field(cx: i32, cz: i32, temp: f32) -> Arc<umicraft_render::PrecipitationField> {
    let mut field = umicraft_render::PrecipitationField::new(cx, cz);
    for z in cz - 12..=cz + 12 {
        for x in cx - 12..=cx + 12 {
            let height = surface_height(x, z).min(64);
            field.set(
                x,
                z,
                umicraft_render::PrecipitationCell {
                    height,
                    temperature: temp,
                    sky_light: 15,
                    block_light: 0,
                },
            );
        }
    }
    Arc::new(field)
}

/// Draws `frames` frames along the path and times them.
fn measure(
    renderer: &mut Renderer,
    args: &Args,
    origin_block: DVec3,
    frames: u32,
    mut context: ScenarioContext,
) -> Result<Measurement, ClientError> {
    let mut camera = Camera::default();
    camera.set_aspect(args.width, args.height);
    let overlay = Overlay::new(args.width, args.height);
    let mut measurement = Measurement {
        frame_ms: Vec::with_capacity(frames as usize),
        render_ms: Vec::with_capacity(frames as usize),
        wait_ms: Vec::with_capacity(frames as usize),
        adapter: {
            let adapter = renderer.adapter();
            format!("{} ({}, {})", adapter.name, adapter.backend, adapter.vendor)
        },
        gpu_ms: [0.0; GROUP_NAMES.len()],
        gpu_samples: 0,
        alloc_delta: AllocDelta::default(),
        draw_calls: DrawCallStats::default(),
        vram: VramStats::default(),
        ram: MemoryUse::default(),
        weather_vertices: 0,
        active_particles: 0,
    };
    let mut alloc_start = AllocSnapshot::default();
    let mut last_field_pos: Option<(i32, i32)> = None;
    for frame in 0..WARMUP_FRAMES + frames {
        if frame == WARMUP_FRAMES {
            alloc_start = AllocSnapshot::now();
        }
        let pose = pose_at(frame.saturating_sub(WARMUP_FRAMES), frames, context.scenario);
        camera.set_eye(origin_block + pose.offset.as_dvec3());
        camera.yaw = pose.yaw;
        camera.pitch = pose.pitch;

        if context.weather_strength > 0.0 {
            let mut env = RenderEnvironment::new(
                0,
                args.time,
                (frame % 20) as f32 / 20.0,
                context.scenario.view_distance(args.view_distance),
            );
            env.rain_strength = context.weather_strength;
            renderer.set_environment(env);
            let (cx, cz) = (camera.eye().x.floor() as i32, camera.eye().z.floor() as i32);
            if last_field_pos != Some((cx, cz)) {
                let field = create_bench_precipitation_field(cx, cz, context.weather_temp);
                renderer.set_precipitation(field, context.weather_fancy);
                last_field_pos = Some((cx, cz));
            }
            renderer.set_cloud_ticks(frame as f64 * 0.25);
        }

        // Dynamic updates for scenarios with entities and particles
        match context.scenario {
            BenchScenario::Entities20
            | BenchScenario::Entities50
            | BenchScenario::Entities100
            | BenchScenario::Entities200
            | BenchScenario::Mixed
            | BenchScenario::WeatherG => {
                if let Some(ref entity_textures) = context.entity_textures {
                    for (i, ent) in context.entities.iter_mut().enumerate() {
                        let t = (frame as f32 * 0.15) + (i as f32 * 0.4);
                        ent.limb_swing = t;
                        ent.limb_swing_amount = 0.6 + 0.3 * (t * 0.7).sin();
                        ent.yaw = (ent.yaw + 1.0) % 360.0;
                        ent.head_yaw = ent.yaw;
                        ent.body_yaw = ent.yaw;
                    }
                    entity_textures.mesh_and_upload(&context.entities, renderer);
                }
                if !context.particles.is_empty() {
                    let t = frame as f32 * 0.05;
                    for (i, p) in context.particles.iter_mut().enumerate() {
                        let angle = (i as f32) * 0.15 + t;
                        let r = 6.0 + ((i % 50) as f32) * 0.25;
                        let y = 64.5 + (((i * 5) as f32 + t * 30.0) % 20.0);
                        p.center = Vec3::new(r * angle.cos(), y, r * angle.sin());
                    }
                    renderer.set_particles(&context.particles);
                }
            }
            BenchScenario::PvpCombat => {
                if let Some(ref entity_textures) = context.entity_textures {
                    let t = frame as f32 * 0.2;
                    for (i, ent) in context.entities.iter_mut().enumerate() {
                        ent.swing_progress = (t + i as f32 * 0.5) % 1.0;
                        ent.hurt = (frame / 6) % 2 == (i as u32);
                        let dir = if i == 0 { -1.0 } else { 1.0 };
                        ent.position.x = dir * (1.8 + 0.3 * (t * 2.0).sin().abs());
                    }
                    entity_textures.mesh_and_upload(&context.entities, renderer);

                    context.particles.clear();
                    for p in 0..24 {
                        let angle = (p as f32 / 24.0) * std::f32::consts::TAU + t;
                        let speed = 0.5 + (p % 3) as f32 * 0.3;
                        context.particles.push(ParticleSprite {
                            center: Vec3::new(speed * angle.cos(), 65.5 + 0.2 * (p as f32).sin(), speed * angle.sin()),
                            half_size: 0.12,
                            uv_min: [0.0, 0.0],
                            uv_max: [1.0, 1.0],
                            atlas_layer: None,
                            color: [1.0, 0.85, 0.1, 1.0],
                            light: [15.0, 15.0],
                            flat: false,
                        });
                    }
                    renderer.set_particles(&context.particles);
                }
            }
            BenchScenario::ParticlesHeavy => {
                let t = frame as f32 * 0.05;
                for (i, p) in context.particles.iter_mut().enumerate() {
                    let angle = (i as f32) * 0.1 + t;
                    let r = 5.0 + ((i % 100) as f32) * 0.2;
                    let y = 64.5 + (((i * 7) as f32 + t * 40.0) % 25.0);
                    p.center = Vec3::new(r * angle.cos(), y, r * angle.sin());
                }
                renderer.set_particles(&context.particles);
            }
            _ => {}
        }

        let start = Instant::now();
        renderer.render(&camera, &overlay)?;
        let render_ms = start.elapsed().as_secs_f32() * 1000.0;
        renderer.wait_idle();
        let elapsed = start.elapsed().as_secs_f32() * 1000.0;
        if frame >= WARMUP_FRAMES {
            measurement.frame_ms.push(elapsed);
            measurement.render_ms.push(render_ms);
            measurement.wait_ms.push(elapsed - render_ms);
            if let Some(timings) = renderer.gpu_timings() {
                for (sum, ms) in measurement.gpu_ms.iter_mut().zip(timings.ms) {
                    *sum += ms;
                }
                measurement.gpu_samples += 1;
            }
        }
    }
    measurement.alloc_delta = AllocSnapshot::now().delta_since(alloc_start);
    measurement.draw_calls = renderer.draw_calls();
    measurement.vram = renderer.vram_stats();
    measurement.ram = crate::process_memory::read();
    measurement.weather_vertices = renderer.weather_vertex_count();
    measurement.active_particles = context.particles.len();
    Ok(measurement)
}

/// Formats the report printed by [`run`].
fn report(
    args: &Args,
    scene: &SceneFacts,
    measurement: &Measurement,
    scenario: BenchScenario,
) -> String {
    let mut text = String::new();
    let _ = writeln!(
        text,
        "bench scene: scenario {}, origin {}, view distance {}, {}x{}, {} frames",
        scenario.label(),
        args.bench_origin,
        scenario.view_distance(args.view_distance),
        args.width,
        args.height,
        measurement.frame_ms.len()
    );
    let _ = writeln!(
        text,
        "world: {} columns, {} meshes, {} faces (gen {:.1} ms, mesh wall {:.1} ms on {} threads)",
        scene.columns,
        scene.meshes,
        scene.faces,
        scene.gen_wall_ms,
        scene.mesh_wall_ms,
        scene.threads,
    );
    if let Some(mesh_stats) = FrameStats::of(&scene.mesh_build_us) {
        let _ = writeln!(
            text,
            "chunk mesh us: mean {:.0}  p50 {:.0}  p95 {:.0}  p99 {:.0}  max {:.0}",
            mesh_stats.mean, mesh_stats.p50, mesh_stats.p95, mesh_stats.p99, mesh_stats.max
        );
    }
    if let Some(dist) = FrameDistribution::of(&measurement.frame_ms) {
        let _ = writeln!(
            text,
            "frame ms: mean {:.2}  p50 {:.2}  p95 {:.2}  p99 {:.2}  p99.9 {:.2}  max {:.2}",
            dist.mean_ms, dist.p50_ms, dist.p95_ms, dist.p99_ms, dist.p99_9_ms, dist.max_ms
        );
        let _ = writeln!(
            text,
            "fps: mean {:.1}  1% low {:.1}  0.1% low {:.1}",
            dist.mean_fps, dist.fps_1_low, dist.fps_0_1_low
        );
    }
    let _ = writeln!(text, "adapter: {}", measurement.adapter);
    for (name, samples) in [
        ("render cpu ms", &measurement.render_ms),
        ("gpu wait ms", &measurement.wait_ms),
    ] {
        if let Some(stats) = FrameStats::of(samples) {
            let _ = writeln!(
                text,
                "{name}: mean {:.2}  p50 {:.2}  p95 {:.2}  p99 {:.2}  max {:.2}",
                stats.mean, stats.p50, stats.p95, stats.p99, stats.max
            );
        }
    }
    if measurement.gpu_samples == 0 {
        text.push_str("gpu ms: n/a\n");
    } else {
        let samples = measurement.gpu_samples as f32;
        let groups: Vec<String> = GROUP_NAMES
            .iter()
            .zip(measurement.gpu_ms)
            .map(|(name, sum)| format!("{name} {:.2}", sum / samples))
            .collect();
        let total: f32 = measurement.gpu_ms.iter().sum::<f32>() / samples;
        let _ = writeln!(
            text,
            "gpu ms (mean): total {total:.2}  {}",
            groups.join("  ")
        );
    }
    let dc = measurement.draw_calls;
    let _ = writeln!(
        text,
        "draw calls/frame: total {}  (sky {}, opaque {}, entities {}, translucent {}, ui {})",
        dc.total(),
        dc.sky,
        dc.opaque,
        dc.entities,
        dc.translucent,
        dc.ui
    );
    if measurement.weather_vertices > 0 || measurement.active_particles > 0 {
        let _ = writeln!(
            text,
            "weather vertices/frame: {}  |  active particles: {}",
            measurement.weather_vertices, measurement.active_particles
        );
    }
    let frames_f = measurement.frame_ms.len().max(1) as f64;
    let _ = writeln!(
        text,
        "allocs/frame: {:.1} allocs ({:.1} B), {:.1} reallocs",
        measurement.alloc_delta.allocs as f64 / frames_f,
        measurement.alloc_delta.bytes_allocated as f64 / frames_f,
        measurement.alloc_delta.reallocs as f64 / frames_f,
    );
    let vram = measurement.vram;
    let mib = 1024.0 * 1024.0;
    let _ = writeln!(
        text,
        "memory: RAM {:.1} MiB / {:.1} MiB | VRAM {:.2} MiB (chunks {:.2} MiB, dyn {:.2} MiB, tex {:.2} MiB)",
        measurement.ram.used as f64 / mib,
        measurement.ram.max as f64 / mib,
        vram.total_bytes() as f64 / mib,
        vram.chunk_buffers as f64 / mib,
        vram.dynamic_buffers as f64 / mib,
        vram.textures as f64 / mib,
    );
    text
}

/// What the generated scene contains.
struct SceneFacts {
    columns: usize,
    meshes: usize,
    faces: usize,
    gen_wall_ms: f32,
    mesh_wall_ms: f32,
    threads: usize,
    mesh_build_us: Vec<f32>,
}

/// Loads the scene as a burst of chunks through the real [`WorldView`] (job
/// queue, mesh workers, uploads) while drawing frames, and reports how the
/// frames fare and how fast the chunks near the camera appear (F2-4).
///
/// The chunks are sent in column order (x, then z), a bad order for a camera in
/// the middle, so a nearest-first queue shows.
fn run_stream(args: &Args) -> Result<(), ClientError> {
    use std::sync::mpsc::sync_channel;

    use crate::game::{ChunkEvent, RenderEvent};
    use crate::mesher::worker_count;
    use crate::world_view::WorldView;

    let origin = ChunkPos::new(
        args.bench_origin.div_euclid(16),
        args.bench_origin.div_euclid(16),
    );
    let reach = i32::from(args.view_distance);
    let mut renderer = Renderer::new_offscreen(args.width, args.height)?;
    let installed = TerrainTextures::load().install(&mut renderer);
    renderer.set_environment(RenderEnvironment::new(
        0,
        args.time,
        0.0,
        args.view_distance,
    ));
    let mut world_view = WorldView::new(&installed, worker_count(args.bench_workers))?;
    let columns = generate_world(origin, reach);
    let mut positions: Vec<ChunkPos> = columns
        .keys()
        .copied()
        .filter(|pos| {
            (-reach..=reach).contains(&(pos.x - origin.x))
                && (-reach..=reach).contains(&(pos.z - origin.z))
        })
        .collect();
    positions.sort_unstable_by_key(|pos| (pos.x, pos.z));
    let (frames_tx, frames_rx) = sync_channel::<RenderEvent>(1);
    let (chunks_tx, chunks_rx) = sync_channel::<ChunkEvent>(positions.len().max(1));
    for pos in &positions {
        let neighbours = (-1..=1)
            .flat_map(|dz| (-1..=1).map(move |dx| ChunkPos::new(pos.x + dx, pos.z + dz)))
            .filter_map(|neighbour| columns.get(&neighbour));
        let snapshot = ChunkSnapshot::from_columns(*pos, neighbours.map(Arc::clone));
        chunks_tx
            .send(ChunkEvent {
                pos: *pos,
                snapshot: Some(snapshot),
            })
            .map_err(|_| ClientError::Worker("chunk channel closed".into()))?;
    }
    drop(frames_tx);

    let origin_block = DVec3::new(
        f64::from(origin.x * 16) + 8.0,
        0.0,
        f64::from(origin.z * 16) + 8.0,
    );
    let mut camera = Camera::default();
    camera.set_aspect(args.width, args.height);
    camera.set_eye(origin_block + DVec3::new(0.0, f64::from(CAMERA_HEIGHT), 0.0));
    camera.pitch = -0.4;
    let overlay = Overlay::new(args.width, args.height);
    let near: Vec<ChunkPos> = positions
        .iter()
        .copied()
        .filter(|pos| (pos.x - origin.x).abs() <= 1 && (pos.z - origin.z).abs() <= 1)
        .collect();

    let start = Instant::now();
    let mut frame_ms = Vec::new();
    let mut sync_ms = Vec::new();
    let mut near_ms = None;
    let mut total_ms = None;
    for frame in 0..100_000_u32 {
        let begin = Instant::now();
        camera.yaw = frame as f32 * 0.02;
        world_view.focus_on(&camera);
        let sync_start = Instant::now();
        world_view.sync(&frames_rx, &chunks_rx, &mut renderer);
        sync_ms.push(sync_start.elapsed().as_secs_f32() * 1000.0);
        renderer.render(&camera, &overlay)?;
        renderer.wait_idle();
        frame_ms.push(begin.elapsed().as_secs_f32() * 1000.0);
        let elapsed = start.elapsed().as_secs_f32() * 1000.0;
        if near_ms.is_none() && near.iter().all(|pos| world_view.is_meshed(*pos)) {
            near_ms = Some(elapsed);
        }
        if world_view.chunk_count() >= positions.len() {
            total_ms = Some(elapsed);
            break;
        }
    }
    let mut text = format!(
        "bench stream: {} chunks, {} workers\n",
        positions.len(),
        worker_count(args.bench_workers)
    );
    // The first frames compile pipelines and start the workers: leave them out.
    let steady = frame_ms.get(STREAM_WARMUP..).unwrap_or(&[]);
    if let Some(dist) = FrameDistribution::of(steady) {
        let _ = writeln!(
            text,
            "frames while loading: {}  mean {:.2} ms  p95 {:.2}  p99 {:.2}  max {:.2}  (1% low {:.1} fps, 0.1% low {:.1} fps)",
            steady.len(),
            dist.mean_ms,
            dist.p95_ms,
            dist.p99_ms,
            dist.max_ms,
            dist.fps_1_low,
            dist.fps_0_1_low,
        );
    }
    if let Some(sync) = FrameStats::of(sync_ms.get(STREAM_WARMUP..).unwrap_or(&[])) {
        let _ = writeln!(
            text,
            "sync (collect, upload, dispatch) per frame: mean {:.2} ms  p99 {:.2}  max {:.2}",
            sync.mean, sync.p99, sync.max
        );
    }
    let pool = world_view.mesh_pool_stats();
    let _ = writeln!(
        text,
        "mesh workers: {} workers, {} jobs completed, mean {:.0} us, max {} us",
        pool.worker_count,
        pool.completed_jobs,
        pool.mean_build_micros(),
        pool.max_build_micros,
    );
    let show = |value: Option<f32>| value.map_or("never".to_string(), |ms| format!("{ms:.0} ms"));
    let _ = writeln!(
        text,
        "nearest 9 chunks shown after {}, all after {}",
        show(near_ms),
        show(total_ms)
    );
    print!("{text}");
    Ok(())
}

/// Runs the benchmark scene and prints its report.
///
/// # Errors
/// Renderer creation, texture installation or a draw failure; with
/// `--screenshot PATH`, also a readback or PNG error.
pub fn run(args: &Args) -> Result<(), ClientError> {
    umicraft_core::profiling::set_enabled(true);
    if args.bench_stream {
        return run_stream(args);
    }
    let scenario = args
        .bench_scenario
        .as_deref()
        .map(BenchScenario::parse)
        .unwrap_or(BenchScenario::Standard);

    let origin = ChunkPos::new(
        args.bench_origin.div_euclid(16),
        args.bench_origin.div_euclid(16),
    );
    let reach = scenario.view_distance(args.view_distance) as i32;
    let mut renderer = Renderer::new_offscreen(args.width, args.height)?;
    let installed = TerrainTextures::load().install(&mut renderer);
    renderer.set_cave_culling(!args.bench_no_cave_culling);
    renderer.set_environment(RenderEnvironment::new(
        0,
        args.time,
        0.0,
        reach as u8,
    ));

    let (world, meshes, gen_wall_ms, mesh_wall_ms) = if matches!(
        scenario,
        BenchScenario::Empty
            | BenchScenario::WeatherA
            | BenchScenario::WeatherB
            | BenchScenario::WeatherC
            | BenchScenario::WeatherD
            | BenchScenario::WeatherE
    ) {
        (HashMap::new(), Vec::new(), 0.0, 0.0)
    } else {
        let gen_start = Instant::now();
        let world = generate_world(origin, reach);
        let gen_wall_ms = gen_start.elapsed().as_secs_f32() * 1000.0;
        let mesh_start = Instant::now();
        let meshes = mesh_world(&world, &installed.mesher());
        let mesh_wall_ms = mesh_start.elapsed().as_secs_f32() * 1000.0;
        (world, meshes, gen_wall_ms, mesh_wall_ms)
    };

    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let facts = SceneFacts {
        columns: world.len(),
        meshes: meshes.len(),
        faces: meshes.iter().map(|(_, mesh, _)| mesh.face_count()).sum(),
        gen_wall_ms,
        mesh_wall_ms,
        threads,
        mesh_build_us: meshes.iter().map(|(_, _, us)| *us).collect(),
    };
    for (pos, mesh, _) in &meshes {
        renderer.upload_mesh(*pos, mesh);
    }

    let mut context = ScenarioContext {
        scenario,
        entity_textures: None,
        entities: Vec::new(),
        particles: Vec::new(),
        weather_strength: 0.0,
        weather_fancy: true,
        weather_temp: 0.8,
    };

    match scenario {
        BenchScenario::Entities20
        | BenchScenario::Entities50
        | BenchScenario::Entities100
        | BenchScenario::Entities200 => {
            let count = match scenario {
                BenchScenario::Entities20 => 20,
                BenchScenario::Entities50 => 50,
                BenchScenario::Entities100 => 100,
                _ => 200,
            };
            context.entity_textures = Some(EntityTextures::load(&renderer));
            let kinds = [
                EntityKind::Player,
                EntityKind::Zombie,
                EntityKind::Skeleton,
                EntityKind::Creeper,
                EntityKind::Cow,
                EntityKind::Sheep,
                EntityKind::Pig,
                EntityKind::Spider,
            ];
            for i in 0..count {
                let angle = (i as f32 / count as f32) * std::f32::consts::TAU;
                let radius = 6.0 + (i % 6) as f32 * 3.5;
                let pos = Vec3::new(radius * angle.cos(), 65.0, radius * angle.sin());
                let mut snap = crate::entity_demo::make_snapshot(i as i32 + 1, kinds[i % kinds.len()], pos, None);
                snap.yaw = angle.to_degrees() + 90.0;
                snap.body_yaw = snap.yaw;
                snap.head_yaw = snap.yaw;
                context.entities.push(snap);
            }
        }
        BenchScenario::PvpCombat => {
            context.entity_textures = Some(EntityTextures::load(&renderer));
            let p1 = {
                let mut snap = crate::entity_demo::make_snapshot(1, EntityKind::Player, Vec3::new(-1.6, 65.0, 0.0), None);
                snap.yaw = 90.0;
                snap.body_yaw = 90.0;
                snap.head_yaw = 90.0;
                snap.armor = [
                    Some(EntityItem { item_id: 310, damage: 0, count: 1, nbt: None }),
                    Some(EntityItem { item_id: 311, damage: 0, count: 1, nbt: None }),
                    Some(EntityItem { item_id: 312, damage: 0, count: 1, nbt: None }),
                    Some(EntityItem { item_id: 313, damage: 0, count: 1, nbt: None }),
                ];
                snap.held = Some(EntityItem { item_id: 276, damage: 0, count: 1, nbt: None });
                snap
            };
            let p2 = {
                let mut snap = crate::entity_demo::make_snapshot(2, EntityKind::Player, Vec3::new(1.6, 65.0, 0.0), None);
                snap.yaw = 270.0;
                snap.body_yaw = 270.0;
                snap.head_yaw = 270.0;
                snap.armor = [
                    Some(EntityItem { item_id: 310, damage: 0, count: 1, nbt: None }),
                    Some(EntityItem { item_id: 311, damage: 0, count: 1, nbt: None }),
                    Some(EntityItem { item_id: 312, damage: 0, count: 1, nbt: None }),
                    Some(EntityItem { item_id: 313, damage: 0, count: 1, nbt: None }),
                ];
                snap.held = Some(EntityItem { item_id: 276, damage: 0, count: 1, nbt: None });
                snap
            };
            context.entities.push(p1);
            context.entities.push(p2);
        }
        BenchScenario::ParticlesHeavy => {
            for i in 0..4000 {
                let angle = (i as f32) * 0.1;
                let r = 5.0 + ((i % 100) as f32) * 0.2;
                let y = 64.5 + ((i % 30) as f32) * 0.5;
                context.particles.push(ParticleSprite {
                    center: Vec3::new(r * angle.cos(), y, r * angle.sin()),
                    half_size: 0.15,
                    uv_min: [0.0, 0.0],
                    uv_max: [1.0, 1.0],
                    atlas_layer: None,
                    color: [1.0, 0.8, 0.2, 0.9],
                    light: [15.0, 15.0],
                    flat: false,
                });
            }
        }
        BenchScenario::Mixed => {
            context.entity_textures = Some(EntityTextures::load(&renderer));
            let kinds = [
                EntityKind::Player,
                EntityKind::Zombie,
                EntityKind::Skeleton,
                EntityKind::Cow,
            ];
            for i in 0..100 {
                let angle = (i as f32 / 100.0) * std::f32::consts::TAU;
                let radius = 8.0 + (i % 5) as f32 * 5.0;
                let pos = Vec3::new(radius * angle.cos(), 65.0, radius * angle.sin());
                let snap = crate::entity_demo::make_snapshot(i as i32 + 1, kinds[i % kinds.len()], pos, None);
                context.entities.push(snap);
            }
            for i in 0..1000 {
                let angle = (i as f32) * 0.15;
                let r = 6.0 + ((i % 50) as f32) * 0.25;
                let y = 64.5 + ((i % 25) as f32) * 0.4;
                context.particles.push(ParticleSprite {
                    center: Vec3::new(r * angle.cos(), y, r * angle.sin()),
                    half_size: 0.12,
                    uv_min: [0.0, 0.0],
                    uv_max: [1.0, 1.0],
                    atlas_layer: None,
                    color: [0.8, 0.9, 1.0, 0.8],
                    light: [15.0, 15.0],
                    flat: false,
                });
            }
        }
        BenchScenario::WeatherA => {
            context.weather_strength = 0.0;
            context.weather_fancy = true;
            context.weather_temp = 0.8;
        }
        BenchScenario::WeatherB => {
            context.weather_strength = 0.2;
            context.weather_fancy = true;
            context.weather_temp = 0.8;
        }
        BenchScenario::WeatherC => {
            context.weather_strength = 1.0;
            context.weather_fancy = true;
            context.weather_temp = 0.8;
        }
        BenchScenario::WeatherD => {
            context.weather_strength = 1.0;
            context.weather_fancy = true;
            context.weather_temp = 0.8;
        }
        BenchScenario::WeatherE => {
            context.weather_strength = 1.0;
            context.weather_fancy = true;
            context.weather_temp = 0.0;
        }
        BenchScenario::WeatherF => {
            context.weather_strength = 1.0;
            context.weather_fancy = true;
            context.weather_temp = 0.8;
        }
        BenchScenario::WeatherG => {
            context.weather_strength = 1.0;
            context.weather_fancy = true;
            context.weather_temp = 0.8;
            context.entity_textures = Some(EntityTextures::load(&renderer));
            let kinds = [
                EntityKind::Player,
                EntityKind::Zombie,
                EntityKind::Skeleton,
                EntityKind::Cow,
            ];
            for i in 0..50 {
                let angle = (i as f32 / 50.0) * std::f32::consts::TAU;
                let radius = 6.0 + (i % 6) as f32 * 3.5;
                let pos = Vec3::new(radius * angle.cos(), 65.0, radius * angle.sin());
                let snap = crate::entity_demo::make_snapshot(i as i32 + 1, kinds[i % kinds.len()], pos, None);
                context.entities.push(snap);
            }
            for i in 0..500 {
                let angle = (i as f32) * 0.15;
                let r = 6.0 + ((i % 50) as f32) * 0.25;
                let y = 64.5 + (((i * 5) as f32) % 20.0);
                context.particles.push(ParticleSprite {
                    center: Vec3::new(r * angle.cos(), y, r * angle.sin()),
                    half_size: 0.12,
                    uv_min: [0.0, 0.0],
                    uv_max: [1.0, 1.0],
                    atlas_layer: None,
                    color: [0.2, 0.6, 1.0, 0.8],
                    light: [15.0, 15.0],
                    flat: false,
                });
            }
        }
        _ => {}
    }

    let origin_block = DVec3::new(
        f64::from(origin.x * 16) + 8.0,
        0.0,
        f64::from(origin.z * 16) + 8.0,
    );
    let measurement = measure(&mut renderer, args, origin_block, args.bench_frames, context)?;
    print!("{}", report(args, &facts, &measurement, scenario));
    if let Some(path) = args.screenshot.as_deref() {
        let image = renderer.read_pixels()?;
        crate::screenshot::write_png(path, &image.rgba, image.width, image.height)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_terrain_is_the_same_at_every_origin() {
        let near = generate_column(ChunkPos::new(3, -2), ChunkPos::new(0, 0));
        let far = generate_column(ChunkPos::new(62_503, 62_498), ChunkPos::new(62_500, 62_500));
        for (x, y, z) in [(0, 40, 0), (5, 64, 9), (15, 70, 15), (8, 62, 3)] {
            assert_eq!(near.block(x, y, z), far.block(x, y, z), "{x} {y} {z}");
        }
    }

    #[test]
    fn the_column_has_bedrock_ground_and_open_sky() {
        let column = generate_column(ChunkPos::new(0, 0), ChunkPos::new(0, 0));
        assert_eq!(column.block(4, 0, 4), BlockState::new(BEDROCK, 0));
        let surface = surface_height(4, 4);
        let top = column.block(4, surface as usize, 4);
        assert!(top == BlockState::new(GRASS, 0) || top == BlockState::new(SAND, 0));
        assert_eq!(column.block(4, 200, 4), BlockState::AIR);
        let above = (surface.max(SEA_LEVEL) + 1) as usize;
        assert_eq!(column.light(4, above, 4).sky, 15);
    }

    #[test]
    fn the_pose_is_a_function_of_the_frame_only() {
        assert_eq!(
            pose_at(10, 600, BenchScenario::Standard),
            pose_at(10, 600, BenchScenario::Standard)
        );
        assert_ne!(
            pose_at(10, 600, BenchScenario::Standard),
            pose_at(11, 600, BenchScenario::Standard)
        );
        let start = pose_at(0, 600, BenchScenario::Standard);
        assert_eq!(start.offset, Vec3::new(ORBIT_RADIUS, CAMERA_HEIGHT, 0.0));
    }

    #[test]
    fn the_percentiles_use_the_nearest_rank() {
        let samples: Vec<f32> = (1..=100).map(|n| n as f32).collect();
        let stats = FrameStats::of(&samples).expect("samples");
        assert_eq!(stats.p50, 50.0);
        assert_eq!(stats.p95, 95.0);
        assert_eq!(stats.p99, 99.0);
        assert_eq!(stats.max, 100.0);
        assert!((stats.mean - 50.5).abs() < 1e-4);
        assert_eq!(FrameStats::of(&[]), None);
    }

    #[test]
    fn a_single_sample_is_every_percentile() {
        let stats = FrameStats::of(&[7.0]).expect("one sample");
        assert_eq!(
            (stats.mean, stats.p50, stats.p95, stats.p99),
            (7.0, 7.0, 7.0, 7.0)
        );
    }
}

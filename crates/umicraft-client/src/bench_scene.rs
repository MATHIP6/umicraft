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
use umicraft_mesh::{ChunkSnapshot, Mesh};
use umicraft_render::{Camera, Overlay, RenderEnvironment, Renderer, GROUP_NAMES};
use umicraft_world::{BlockState, ChunkColumn, ChunkPos};

use crate::args::Args;
use crate::error::ClientError;
use crate::terrain_textures::{TerrainMesher, TerrainTextures};

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
fn pose_at(frame: u32, frames: u32) -> Pose {
    let t = frame as f32 / frames.max(1) as f32;
    let angle = t * std::f32::consts::TAU;
    Pose {
        offset: Vec3::new(
            ORBIT_RADIUS * angle.cos(),
            CAMERA_HEIGHT,
            ORBIT_RADIUS * angle.sin(),
        ),
        yaw: angle * TURNS,
        pitch: -0.3 + 0.15 * (angle * 3.0).sin(),
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

/// Meshes every column whose 3×3 neighbourhood is generated.
fn mesh_world(world: &World, mesher: &TerrainMesher) -> Vec<(ChunkPos, Mesh)> {
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
        let columns = (-1..=1).flat_map(|dz| {
            (-1..=1).filter_map(move |dx| world.get(&ChunkPos::new(pos.x + dx, pos.z + dz)))
        });
        let snapshot = ChunkSnapshot::from_columns(*pos, columns.map(Arc::clone));
        (*pos, mesher.mesh(*pos, &snapshot))
    })
}

/// Maps `items` on all cores and returns the results in input order.
fn parallel_map<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let size = items.len().div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        let handles: Vec<_> = items
            .chunks(size)
            .map(|part| scope.spawn(|| part.iter().map(&work).collect::<Vec<R>>()))
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap_or_default())
            .collect()
    })
}

/// The measurements of one run.
struct Measurement {
    frame_ms: Vec<f32>,
    gpu_ms: [f32; GROUP_NAMES.len()],
    gpu_samples: u32,
}

/// Draws `frames` frames along the path and times them.
fn measure(
    renderer: &mut Renderer,
    args: &Args,
    origin_block: DVec3,
    frames: u32,
) -> Result<Measurement, ClientError> {
    let mut camera = Camera::default();
    camera.set_aspect(args.width, args.height);
    let overlay = Overlay::new(args.width, args.height);
    let mut measurement = Measurement {
        frame_ms: Vec::with_capacity(frames as usize),
        gpu_ms: [0.0; GROUP_NAMES.len()],
        gpu_samples: 0,
    };
    for frame in 0..WARMUP_FRAMES + frames {
        let pose = pose_at(frame.saturating_sub(WARMUP_FRAMES), frames);
        camera.set_eye(origin_block + pose.offset.as_dvec3());
        camera.yaw = pose.yaw;
        camera.pitch = pose.pitch;
        let start = Instant::now();
        renderer.render(&camera, &overlay)?;
        renderer.wait_idle();
        let elapsed = start.elapsed().as_secs_f32() * 1000.0;
        if frame >= WARMUP_FRAMES {
            measurement.frame_ms.push(elapsed);
            if let Some(timings) = renderer.gpu_timings() {
                for (sum, ms) in measurement.gpu_ms.iter_mut().zip(timings.ms) {
                    *sum += ms;
                }
                measurement.gpu_samples += 1;
            }
        }
    }
    Ok(measurement)
}

/// Formats the report printed by [`run`].
fn report(args: &Args, scene: &SceneFacts, measurement: &Measurement) -> String {
    let mut text = String::new();
    let _ = writeln!(
        text,
        "bench scene: origin {}, view distance {}, {}x{}, {} frames",
        args.bench_origin,
        args.view_distance,
        args.width,
        args.height,
        measurement.frame_ms.len()
    );
    let _ = writeln!(
        text,
        "world: {} columns, {} meshes, {} faces",
        scene.columns, scene.meshes, scene.faces
    );
    if let Some(stats) = FrameStats::of(&measurement.frame_ms) {
        let _ = writeln!(
            text,
            "frame ms: mean {:.2}  p50 {:.2}  p95 {:.2}  p99 {:.2}  max {:.2}  ({:.0} fps mean)",
            stats.mean,
            stats.p50,
            stats.p95,
            stats.p99,
            stats.max,
            1000.0 / stats.mean.max(f32::EPSILON)
        );
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
    text
}

/// What the generated scene contains.
struct SceneFacts {
    columns: usize,
    meshes: usize,
    faces: usize,
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
                snapshot,
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
    if let Some(stats) = FrameStats::of(steady) {
        let _ = writeln!(
            text,
            "frames while loading: {}  mean {:.2} ms  p95 {:.2}  p99 {:.2}  max {:.2}",
            steady.len(),
            stats.mean,
            stats.p95,
            stats.p99,
            stats.max
        );
    }
    if let Some(sync) = FrameStats::of(sync_ms.get(STREAM_WARMUP..).unwrap_or(&[])) {
        let _ = writeln!(
            text,
            "sync (collect, upload, dispatch) per frame: mean {:.2} ms  p99 {:.2}  max {:.2}",
            sync.mean, sync.p99, sync.max
        );
    }
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
    if args.bench_stream {
        return run_stream(args);
    }
    let origin = ChunkPos::new(
        args.bench_origin.div_euclid(16),
        args.bench_origin.div_euclid(16),
    );
    let reach = i32::from(args.view_distance);
    let mut renderer = Renderer::new_offscreen(args.width, args.height)?;
    let installed = TerrainTextures::load().install(&mut renderer);
    renderer.set_cave_culling(!args.bench_no_cave_culling);
    renderer.set_environment(RenderEnvironment::new(
        0,
        args.time,
        0.0,
        args.view_distance,
    ));

    let world = generate_world(origin, reach);
    let meshes = mesh_world(&world, &installed.mesher());
    let facts = SceneFacts {
        columns: world.len(),
        meshes: meshes.len(),
        faces: meshes.iter().map(|(_, mesh)| mesh.face_count()).sum(),
    };
    for (pos, mesh) in &meshes {
        renderer.upload_mesh(*pos, mesh);
    }

    let origin_block = DVec3::new(
        f64::from(origin.x * 16) + 8.0,
        0.0,
        f64::from(origin.z * 16) + 8.0,
    );
    let measurement = measure(&mut renderer, args, origin_block, args.bench_frames)?;
    print!("{}", report(args, &facts, &measurement));
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
        assert_eq!(pose_at(10, 600), pose_at(10, 600));
        assert_ne!(pose_at(10, 600), pose_at(11, 600));
        let start = pose_at(0, 600);
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

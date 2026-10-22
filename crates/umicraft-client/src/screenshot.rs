// SPDX-License-Identifier: MIT OR Apache-2.0

//! Offscreen screenshot pipeline: collect the world through the game thread,
//! mesh the chunk snapshots, render with wgpu and write a PNG.
//!
//! The path shares the network thread, the game thread and the mesh pool
//! with the windowed client, so a screenshot shows exactly what the player
//! would see — including the HUD of the real session.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::time::Instant;

use umicraft_mesh::ChunkSnapshot;
use umicraft_render::{draw_hud, Camera, HudState, Overlay, Renderer};

use crate::args::Args;
use crate::error::ClientError;
use crate::game::{camera_angles, ChunkEvent, GameStatus, RenderEvent, Session};
use crate::mesher::{default_worker_count, MeshPool};
use crate::network::NetworkConfig;

/// Runs the screenshot path and writes `args.screenshot`.
///
/// # Errors
/// Network failure, no world/chunks before the deadline, renderer creation,
/// readback, or PNG output failure.
pub fn run(args: &Args) -> Result<(), ClientError> {
    let path = args
        .screenshot
        .as_deref()
        .ok_or_else(|| ClientError::Screenshot("no output path".to_string()))?;
    let (host, port) = args
        .server()
        .ok_or_else(|| ClientError::Network("--screenshot needs a host and port".to_string()))?;
    let session = Session::start(NetworkConfig {
        host: host.to_string(),
        port,
        name: args.name.clone(),
        view_distance: args.view_distance,
    })?;

    let deadline = Instant::now() + args.collect;
    let mut renderer: Option<Renderer> = None;
    let pool = MeshPool::new(default_worker_count())?;
    let mut queued: Vec<(umicraft_world::ChunkPos, ChunkSnapshot)> = Vec::new();
    let mut pending = std::collections::HashSet::new();
    let mut camera = Camera::default();
    let mut hud = HudState::default();
    let mut status = GameStatus::default();
    let mut meshes = 0_usize;
    let mut faces = 0_usize;
    let mut frames = 0_usize;
    let mut chunks = 0_usize;

    while Instant::now() < deadline {
        while let Ok(event) = session.frames().try_recv() {
            match event {
                RenderEvent::Frame(frame) => {
                    frames += 1;
                    camera.position = frame.state.position;
                    let (yaw, pitch) = camera_angles(&frame.state);
                    camera.yaw = yaw;
                    camera.pitch = pitch;
                }
                RenderEvent::Hud(new_hud) => hud = *new_hud,
                RenderEvent::Status(new_status) => status = *new_status,
                RenderEvent::Reset => {
                    if let Some(renderer) = renderer.as_mut() {
                        renderer.clear_meshes();
                    }
                }
                RenderEvent::Ended(reason) => {
                    return Err(ClientError::Network(reason));
                }
            }
        }
        while let Ok(ChunkEvent { pos, snapshot }) = session.chunks().try_recv() {
            queued.push((pos, snapshot));
        }

        // Mesh the snapshots the game thread delivered.
        let mut index = 0;
        while index < queued.len() {
            let (pos, snapshot) = queued[index].clone();
            if pending.contains(&pos) {
                index += 1;
                continue;
            }
            match pool.submit(pos, snapshot) {
                Ok(()) => {
                    pending.insert(pos);
                    queued.remove(index);
                }
                Err(_) => break,
            }
        }
        while let Some(result) = pool.try_recv() {
            pending.remove(&result.pos);
            if result.mesh.is_empty() {
                continue;
            }
            faces += result.mesh.face_count();
            meshes += 1;
            chunks += 1;
            if renderer.is_none() {
                renderer = Some(Renderer::new_offscreen(args.width, args.height)?);
            }
            if let Some(renderer) = renderer.as_mut() {
                renderer.upload_mesh(result.pos, &result.mesh);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    drop(session);

    if status.status.starts_with("ERROR ") {
        return Err(ClientError::Network(
            status.status["ERROR ".len()..].to_string(),
        ));
    }
    if chunks == 0 {
        return Err(ClientError::NoChunks);
    }

    let mut renderer = match renderer {
        Some(renderer) => renderer,
        None => Renderer::new_offscreen(args.width, args.height)?,
    };
    camera.set_aspect(args.width, args.height);
    let mut overlay = Overlay::new(args.width, args.height);
    let lines = [
        "UMICRAFT M3 SCREENSHOT".to_string(),
        format!("CHUNKS {}  MESHES {}  FACES {}", chunks, meshes, faces),
        format!(
            "XYZ {:.1} {:.1} {:.1}",
            camera.position.x, camera.position.y, camera.position.z
        ),
        format!("SERVER {host}:{port}  VIEW {}", args.view_distance),
        format!("COLLECTED {:.1}S", args.collect.as_secs_f32()),
    ];
    // The offscreen path draws the same HUD as the window, so the acceptance
    // screenshot shows the in-game layout.
    let hud = HudState {
        debug_lines: lines.to_vec(),
        ..hud
    };
    draw_hud(&mut overlay, &hud, args.width, args.height);

    renderer.render(&camera, &overlay)?;
    let image = renderer.read_pixels()?;
    write_png(path, &image.rgba, image.width, image.height)?;

    println!(
        "screenshot written: {} ({}x{}, {} chunks, {} meshes, {} faces, {} frames, collected {:.1}s)",
        path.display(),
        image.width,
        image.height,
        chunks,
        meshes,
        faces,
        frames,
        args.collect.as_secs_f32()
    );
    Ok(())
}

/// Writes one tightly packed RGBA8 frame to `path` as a PNG.
///
/// Shared by the terrain screenshot and the server-less UI demo so both paths
/// use the same encoder and the same output-path handling.
pub(crate) fn write_png(
    path: &Path,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<(), ClientError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|error| ClientError::Screenshot(error.to_string()))?;
    writer
        .write_image_data(rgba)
        .map_err(|error| ClientError::Screenshot(error.to_string()))?;
    writer
        .finish()
        .map_err(|error| ClientError::Screenshot(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_round_trip_dimensions() {
        let dir = std::env::temp_dir().join("umicraft-client-test");
        let path = dir.join("pixels.png");
        let pixels = vec![255_u8; 4 * 4 * 4];
        write_png(&path, &pixels, 4, 4).expect("write png");
        let decoder = png::Decoder::new(File::open(&path).expect("open png"));
        let reader = decoder.read_info().expect("png info");
        assert_eq!((reader.info().width, reader.info().height), (4, 4));
        std::fs::remove_file(&path).ok();
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Offscreen screenshot pipeline: collect world packets, mesh chunks, render
//! with wgpu, and write a PNG. Used by M2 acceptance and by CI-like smoke
//! checks where no window is available.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::time::Instant;

use umicraft_mesh::ChunkSnapshot;
use umicraft_render::{Camera, Overlay, Renderer};
use umicraft_world::ChunkPos;

use crate::args::Args;
use crate::error::ClientError;
use crate::mesher::{default_worker_count, MeshPool};
use crate::network::{spawn, NetEvent, NetworkConfig};
use crate::scene::SessionWorld;

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
    let (_network, receiver) = spawn(NetworkConfig {
        host: args.host.clone(),
        port: args.port,
        name: args.name.clone(),
        view_distance: args.view_distance,
    })?;

    let mut scene = SessionWorld::new();
    let deadline = Instant::now() + args.collect;
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(remaining) {
            Ok(event) => {
                let error = matches!(event, NetEvent::Error(_));
                scene.apply(event);
                if error {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    drop(receiver);

    if let Some(message) = scene.status.strip_prefix("ERROR ") {
        return Err(ClientError::Network(message.to_string()));
    }
    let world = scene.world.as_ref().ok_or(ClientError::NoWorld)?;
    if world.chunk_count() == 0 {
        return Err(ClientError::NoChunks);
    }

    let mut renderer = Renderer::new_offscreen(args.width, args.height)?;
    let positions: Vec<ChunkPos> = world.chunks().map(|(pos, _)| pos).collect();
    let pool = MeshPool::new(default_worker_count())?;
    let mut submitted = 0_usize;
    for pos in &positions {
        if pool.submit(*pos, ChunkSnapshot::from_world(world, *pos)) {
            submitted += 1;
        }
    }

    let mut meshes = 0_usize;
    let mut faces = 0_usize;
    for _ in 0..submitted {
        let Some(result) = pool.recv() else {
            break;
        };
        if result.mesh.is_empty() {
            continue;
        }
        faces += result.mesh.face_count();
        meshes += 1;
        renderer.upload_mesh(result.pos, &result.mesh);
    }

    let mut camera = camera_for(&scene, args.width, args.height);
    camera.set_aspect(args.width, args.height);
    let mut overlay = Overlay::new(args.width, args.height);
    let scale = ((args.height as f32 / 360.0).floor()).clamp(1.0, 3.0);
    let line_height = 8.0 * scale;
    let lines = [
        "UMICRAFT M2 SCREENSHOT".to_string(),
        format!(
            "CHUNKS {}  MESHES {}  FACES {}",
            world.chunk_count(),
            meshes,
            faces
        ),
        format!(
            "XYZ {:.1} {:.1} {:.1}",
            camera.position.x, camera.position.y, camera.position.z
        ),
        format!(
            "SERVER {}:{}  VIEW {}",
            args.host, args.port, args.view_distance
        ),
        format!("COLLECTED {:.1}S", args.collect.as_secs_f32()),
    ];
    let panel_width = lines
        .iter()
        .map(|line| Overlay::text_width(line, scale))
        .fold(0.0_f32, f32::max)
        + 20.0;
    let panel_height = lines.len() as f32 * line_height + 12.0;
    overlay.rect(4.0, 4.0, panel_width, panel_height, [0.0, 0.0, 0.0, 0.55]);
    for (index, line) in lines.iter().enumerate() {
        overlay.text(
            10.0,
            10.0 + index as f32 * line_height,
            scale,
            [1.0, 1.0, 1.0, 1.0],
            line,
        );
    }

    renderer.render(&camera, &overlay)?;
    let image = renderer.read_pixels()?;
    write_png(path, &image.rgba, image.width, image.height)?;

    println!(
        "screenshot written: {} ({}x{}, {} chunks, {} meshes, {} faces, collected {:.1}s)",
        path.display(),
        image.width,
        image.height,
        world.chunk_count(),
        meshes,
        faces,
        args.collect.as_secs_f32()
    );
    Ok(())
}

fn camera_for(scene: &SessionWorld, width: u32, height: u32) -> Camera {
    let mut camera = scene.placement.as_ref().map_or_else(Camera::default, |p| {
        Camera::from_minecraft(p.x, p.y, p.z, p.yaw, p.pitch)
    });
    if scene.placement.is_some() {
        // A slightly elevated, downward view shows the spawn terrain rather
        // than only the horizon at eye level.
        camera.position.y += 8.0;
        camera.pitch = (-18.0_f32).to_radians();
    }
    camera.set_aspect(width, height);
    camera
}

fn write_png(path: &Path, rgba: &[u8], width: u32, height: u32) -> Result<(), ClientError> {
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
    use umicraft_net::client::PlayerPlacement;
    use umicraft_world::World;

    #[test]
    fn camera_uses_placement_and_eye_height() {
        let mut scene = SessionWorld::new();
        scene.world = Some(World::new(0));
        scene.placement = Some(PlayerPlacement {
            x: 10.0,
            y: 4.0,
            z: -3.0,
            yaw: 0.0,
            pitch: 0.0,
        });
        let camera = camera_for(&scene, 800, 600);
        assert!((camera.position.x - 10.0).abs() < 1e-5);
        assert!((camera.position.y - 13.62).abs() < 1e-5);
        assert!((camera.position.z + 3.0).abs() < 1e-5);
        assert!((camera.aspect() - 800.0 / 600.0).abs() < 1e-6);
    }

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

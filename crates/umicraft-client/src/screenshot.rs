// SPDX-License-Identifier: MIT OR Apache-2.0

//! Offscreen screenshot pipeline: collect the world through the game thread,
//! mesh the chunk snapshots, render with wgpu and write a PNG.
//!
//! The path shares the network thread, the game thread and the
//! [`WorldView`] with the windowed client, so a screenshot shows exactly what
//! the player would see — including the HUD of the real session.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::time::Instant;

use umicraft_render::{draw_hud_gui, Camera, NoIcons, Overlay, Renderer};
use umicraft_ui::DrawList;

use crate::args::Args;
use crate::error::ClientError;
use crate::game::Session;
use crate::mesher::default_worker_count;
use crate::network::NetworkConfig;
use crate::terrain_textures::TerrainTextures;
use crate::ui_font;
use crate::world_view::WorldView;

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
        chat_mode: 0,
        chat_colors: true,
        handshake_host: args.handshake_host.clone(),
    })?;

    let deadline = Instant::now() + args.collect;
    let mut renderer = Renderer::new_offscreen(args.width, args.height)?;
    let terrain = TerrainTextures::load().install(&mut renderer);
    let mut world = WorldView::new(&terrain, default_worker_count())?;

    while Instant::now() < deadline {
        world.sync(session.frames(), session.chunks(), &mut renderer);
        if let Some(reason) = world.ended() {
            return Err(ClientError::Network(reason.to_string()));
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    drop(session);

    let status = world.status();
    if let Some(error) = status.status.strip_prefix("ERROR ") {
        return Err(ClientError::Network(error.to_string()));
    }
    let chunks = world.chunk_count();
    if chunks == 0 {
        return Err(ClientError::NoChunks);
    }
    let meshes = renderer.mesh_count();
    let faces = world.face_count();
    let frames = world.frame_count();

    let now = Instant::now();
    let mut camera = Camera::default();
    world.update_camera(&mut camera, now);
    renderer.set_environment(world.environment(now, args.view_distance));
    camera.set_aspect(args.width, args.height);
    // The offscreen path draws the same textured HUD as the window, so the
    // acceptance screenshot shows the in-game layout. Without the GUI atlas
    // only the sprite-free parts (chat, titles, sidebar) appear.
    let installed = ui_font::install(&mut renderer, &ui_font::load().atlas)?;
    let gui_atlas = crate::official::load_gui_atlas();
    if let Some(atlas) = gui_atlas.as_ref().filter(|atlas| !atlas.is_empty()) {
        renderer.set_gui_atlas(atlas.width(), atlas.height(), atlas.pixels());
    }
    let sheets = crate::official::hud_sheets(gui_atlas.as_ref());
    let mut list = DrawList::new();
    draw_hud_gui(
        &mut list,
        world.hud(),
        (args.width, args.height),
        &installed.atlas,
        &sheets,
    );
    renderer.render_with_ui(
        &camera,
        &Overlay::new(args.width, args.height),
        &list,
        &mut NoIcons,
    )?;
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
    use std::time::Duration;

    use super::*;
    use crate::game::TICK;

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

    #[test]
    fn tick_is_one_twentieth_of_a_second() {
        assert_eq!(TICK, Duration::from_millis(50));
    }
}

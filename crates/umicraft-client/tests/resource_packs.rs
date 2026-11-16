// SPDX-License-Identifier: MIT OR Apache-2.0

//! The whole resource pack chain on real packs: archive, layering, atlas,
//! models, items and the GPU upload.
//!
//! Ignored by default: it needs the official 1.8.8 installation and a folder
//! of packs. Run it with
//! `UMICRAFT_PACKS_DIR=<folder> cargo test -p umicraft-client --release --test
//! resource_packs -- --ignored --nocapture`; `WGPU_BACKEND=dx12` or `vulkan`
//! picks the backend of the upload.

use std::path::PathBuf;
use std::time::Instant;

use umicraft_assets::pack_discovery::discover_packs;
use umicraft_assets::ResourcePack;
use umicraft_client::model_resources::ModelResources;
use umicraft_client::terrain_textures::TerrainTextures;
use umicraft_render::Renderer;

#[test]
#[ignore = "requires the official 1.8.8 installation and UMICRAFT_PACKS_DIR"]
fn every_real_pack_loads_and_uploads() {
    let Some(folder) = std::env::var_os("UMICRAFT_PACKS_DIR").map(PathBuf::from) else {
        eprintln!("skipping: UMICRAFT_PACKS_DIR is not set");
        return;
    };
    let base = ModelResources::get_or_load_base().expect("the official 1.8.8 resources");
    let mut renderer = Renderer::new_offscreen(64, 64).expect("an offscreen renderer");
    println!("adapter: {:?}", renderer.adapter());

    let found = discover_packs(&folder);
    assert!(!found.is_empty(), "no pack in {}", folder.display());
    let mut failures = Vec::new();
    for entry in &found {
        let path = folder.join(&entry.name);
        let start = Instant::now();
        let pack = match ResourcePack::from_path(&path) {
            Ok(pack) => pack,
            Err(error) => {
                failures.push(format!("{}: {error}", entry.name));
                continue;
            }
        };
        let read_ms = start.elapsed().as_millis();
        let files = pack.file_count();
        let start = Instant::now();
        let resources = match ModelResources::from_resources_and_packs(base.clone(), &[pack]) {
            Ok(resources) => resources,
            Err(error) => {
                failures.push(format!("{}: {error}", entry.name));
                continue;
            }
        };
        let build_ms = start.elapsed().as_millis();
        let atlas = resources.atlas();
        let gui = resources.resources().gui_atlas();
        let (gui_size, block_bytes) = ((gui.width(), gui.height()), atlas.pixels().len());

        let start = Instant::now();
        let installed = TerrainTextures::official(&resources).install(&mut renderer);
        if !installed.is_official() {
            failures.push(format!("{}: the GPU rejected the block array", entry.name));
        }
        if let Err(error) = renderer.set_gui_atlas(gui.width(), gui.height(), gui.pixels()) {
            failures.push(format!("{}: GUI atlas: {error}", entry.name));
        }
        if let Some(glint) = installed.glint_texture() {
            renderer.set_glint_texture(glint.width, glint.height, &glint.pixels);
        }
        renderer.wait_idle();
        println!(
            "{:<48} files {files:>5}  read {read_ms:>4} ms  build {build_ms:>4} ms  gpu {:>3} ms  \
             blocks {}x{}x{} ({} MiB, {} hires at {})  gui {}x{} ({} MiB)",
            entry.name,
            start.elapsed().as_millis(),
            atlas.width(),
            atlas.height(),
            atlas.layers(),
            block_bytes / (1024 * 1024),
            atlas.hires_layers().len(),
            atlas.hires_edge(),
            gui_size.0,
            gui_size.1,
            gui.pixels().len() / (1024 * 1024),
        );
    }
    assert!(failures.is_empty(), "packs that failed:\n{}", failures.join("\n"));
}

const STONE_MODEL: &str = "assets/minecraft/models/block/stone.json";
const DIRT_MODEL: &str = "assets/minecraft/models/block/dirt.json";
const STONE_TEXTURE: &str = "assets/minecraft/textures/blocks/stone.png";
/// A model of the pack that bakes: dirt drawn with the stone texture.
const CUSTOM_DIRT: &[u8] = br#"{"parent": "block/cube_all", "textures": {"all": "blocks/stone"}}"#;
const RED: [u8; 4] = [255, 0, 0, 255];

fn png(edge: u32, color: [u8; 4]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, edge, edge);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("header");
    writer
        .write_image_data(&color.repeat((edge * edge) as usize))
        .expect("pixels");
    writer.finish().expect("finish");
    bytes
}

fn pack_of(files: &[(&str, &[u8])]) -> ResourcePack {
    let files = files
        .iter()
        .map(|(path, bytes)| ((*path).to_string(), bytes.to_vec()))
        .collect();
    ResourcePack::from_parts("test", None, None, files)
}

/// The first texel of the stone layer.
fn stone_texel(resources: &ModelResources) -> [u8; 4] {
    let atlas = resources.atlas();
    let layer = atlas.layout().layer("blocks/stone").expect("stone") as usize;
    let at = layer * (atlas.width() * atlas.height() * 4) as usize;
    atlas.pixels()[at..at + 4].try_into().expect("four bytes")
}

/// The reference logs a model that does not load and goes on (`ModelBakery`).
/// One such file used to fail the whole load, and the client fell back to its
/// procedural textures: no official atlas, no GUI sheets, no items.
#[test]
#[ignore = "requires the official 1.8.8 installation"]
fn a_broken_model_of_a_pack_gives_way_to_the_official_one() {
    let base = ModelResources::get_or_load_base().expect("the official 1.8.8 resources");
    let pack = pack_of(&[
        (STONE_MODEL, b"{ not json"),
        (DIRT_MODEL, CUSTOM_DIRT),
        (STONE_TEXTURE, &png(16, RED)),
    ]);
    let resources = ModelResources::from_resources_and_packs(base.clone(), &[pack])
        .expect("a broken model is not fatal");
    let files = resources.resources();
    assert_eq!(files.get(STONE_MODEL), base.get(STONE_MODEL), "the official model");
    assert_eq!(files.get(DIRT_MODEL), Some(CUSTOM_DIRT), "the valid model stays");
    assert_eq!(stone_texel(&resources), RED, "and so do the textures");
    assert_eq!(resources.stats().block_states, 2912, "every block state bakes");
}

/// A pack made for a later version: its models name a parent 1.8 does not
/// have. The error names the parent, not a file of the pack, so every model
/// of the packs gives way; the textures stay.
#[test]
#[ignore = "requires the official 1.8.8 installation"]
fn a_model_with_a_missing_parent_costs_the_pack_its_models_not_the_load() {
    let base = ModelResources::get_or_load_base().expect("the official 1.8.8 resources");
    let pack = pack_of(&[
        (STONE_MODEL, br#"{"parent": "block/block"}"#),
        (DIRT_MODEL, CUSTOM_DIRT),
        (STONE_TEXTURE, &png(16, RED)),
    ]);
    let resources = ModelResources::from_resources_and_packs(base.clone(), &[pack])
        .expect("a missing parent is not fatal");
    let files = resources.resources();
    assert_eq!(files.get(STONE_MODEL), base.get(STONE_MODEL));
    assert_eq!(files.get(DIRT_MODEL), base.get(DIRT_MODEL));
    assert_eq!(stone_texel(&resources), RED);
}

/// Switching packs on a live renderer: the arrays of the previous pack are
/// released, and going back and forth gives the same memory every time.
#[test]
#[ignore = "requires the official 1.8.8 installation and a GPU adapter"]
fn switching_packs_on_the_gpu_releases_the_previous_textures() {
    let base = ModelResources::get_or_load_base().expect("the official 1.8.8 resources");
    let mut renderer = Renderer::new_offscreen(64, 64).expect("an offscreen renderer");
    let large = png(256, RED);
    let mut install = |packs: &[ResourcePack]| {
        let resources =
            ModelResources::from_resources_and_packs(base.clone(), packs).expect("resources");
        let installed = TerrainTextures::official(&resources).install(&mut renderer);
        assert!(installed.is_official());
        let gui = resources.resources().gui_atlas();
        renderer
            .set_gui_atlas(gui.width(), gui.height(), gui.pixels())
            .expect("the GUI atlas fits the device");
        renderer.wait_idle();
        renderer.vram_stats().textures
    };
    let plain = install(&[]);
    let heavy = install(&[pack_of(&[(STONE_TEXTURE, &large)])]);
    assert!(heavy > plain, "256 pixel layers take more than 32: {heavy} > {plain}");
    assert_eq!(install(&[]), plain, "back to the default pack");
    assert_eq!(install(&[pack_of(&[(STONE_TEXTURE, &large)])]), heavy);
    assert_eq!(install(&[]), plain, "and back again");
}

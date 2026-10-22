// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests for the M5 client-resource bulk read and the deterministic block
//! atlas.
//!
//! Every fixture is a tiny synthetic zip and tiny synthetic PNG. The official
//! installation is only touched by the opt-in `official_1_8_8_counts` test;
//! no jar or extracted texture is written into the repository.

use std::path::PathBuf;

use umicraft_assets::client_jar::ClientJar;
use umicraft_assets::hash;
use umicraft_assets::{AssetsError, ClientResources};
use umicraft_model::{ResourceSource, MISSING_TEXTURE};

/// The virtual prefix of the blockstate documents.
const BLOCKSTATES: &str = "assets/minecraft/blockstates/";
/// The virtual prefix of the block model documents.
const MODELS: &str = "assets/minecraft/models/block/";
/// The virtual prefix of the block textures.
const BLOCK_TEXTURES: &str = "assets/minecraft/textures/blocks/";
/// The virtual prefix of the item textures.
const ITEM_TEXTURES: &str = "assets/minecraft/textures/items/";

/// A temp directory removed when the test ends.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates a uniquely named directory below the system temp directory.
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "umicraft-assets-resources-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("the temp directory is created");
        Self { path }
    }

    /// The path of a file inside the directory.
    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Builds a zip archive of *stored* entries.
fn stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::with_capacity(entries.len());

    for (name, payload) in entries {
        let name_bytes = name.as_bytes();
        let offset = out.len() as u32;
        out.extend_from_slice(&0x0403_4b50_u32.to_le_bytes()); // local header
        out.extend_from_slice(&10_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes()); // stored
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u32.to_le_bytes()); // crc
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(payload);
        central.push((name, payload.len(), offset));
    }

    let central_offset = out.len() as u32;
    for (name, size, offset) in &central {
        let name_bytes = name.as_bytes();
        out.extend_from_slice(&0x0201_4b50_u32.to_le_bytes()); // central header
        out.extend_from_slice(&20_u16.to_le_bytes());
        out.extend_from_slice(&10_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u32.to_le_bytes());
        out.extend_from_slice(&(*size as u32).to_le_bytes());
        out.extend_from_slice(&(*size as u32).to_le_bytes());
        out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u32.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(name_bytes);
    }
    let central_size = out.len() as u32 - central_offset;

    out.extend_from_slice(&0x0605_4b50_u32.to_le_bytes()); // eocd
    out.extend_from_slice(&0_u16.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out.extend_from_slice(&(central.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u16).to_le_bytes());
    out.extend_from_slice(&central_size.to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out
}

/// Writes a synthetic jar and opens it with its computed digest.
fn jar_with(dir: &TempDir, entries: &[(&str, &[u8])]) -> ClientJar {
    let bytes = stored_zip(entries);
    let path = dir.join("client.jar");
    std::fs::write(&path, &bytes).expect("the synthetic jar is written");
    ClientJar::open(&path, &hash::sha1_hex(&bytes), bytes.len() as u64)
        .expect("the synthetic jar verifies")
}

/// Encodes a tightly packed RGBA8 image as a PNG.
fn png_rgba(width: u32, height: u32, pixels: &[[u8; 4]]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("the PNG header is written");
        let flat: Vec<u8> = pixels.iter().flatten().copied().collect();
        writer
            .write_image_data(&flat)
            .expect("the PNG pixels are written");
    }
    bytes
}

/// A solid image of `width`×`height` pixels.
fn solid(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
    png_rgba(width, height, &vec![color; (width * height) as usize])
}

/// Returns one RGBA texel of a layer-major atlas.
fn texel(atlas: &umicraft_assets::BlockAtlas, layer: u32, x: u32, y: u32) -> [u8; 4] {
    let index = (((layer * atlas.height() + y) * atlas.width() + x) * 4) as usize;
    atlas.pixels()[index..index + 4]
        .try_into()
        .expect("a texel is four bytes")
}

/// A full virtual texture path below `blocks/`.
fn block_path(name: &str) -> String {
    format!("{BLOCK_TEXTURES}{name}.png")
}

#[test]
fn the_bulk_read_returns_only_the_requested_prefixes() {
    let dir = TempDir::new("bulk");
    let blockstate = b"{\"variants\":{}}";
    let model = b"{\"parent\":\"block/cube_all\"}";
    let block_png = b"block texture";
    let item_png = b"item texture";
    let meta = b"{\"animation\":{}}";
    let jar = jar_with(
        &dir,
        &[
            ("assets/minecraft/blockstates/stone.json", blockstate),
            ("assets/minecraft/models/block/stone.json", model),
            (block_path("stone").as_str(), block_png),
            ("assets/minecraft/textures/items/apple.png", item_png),
            ("assets/minecraft/textures/blocks/stone.png.mcmeta", meta),
            ("assets/minecraft/textures/entity/steve.png", b"unrelated"),
            ("pack.mcmeta", b"unrelated"),
        ],
    );

    let entries = jar
        .read_prefixes(&[BLOCKSTATES, MODELS])
        .expect("the bulk read succeeds");
    let names: Vec<&str> = entries.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "assets/minecraft/blockstates/stone.json",
            "assets/minecraft/models/block/stone.json",
        ]
    );
    assert_eq!(entries[0].1, blockstate);
    assert_eq!(entries[1].1, model);

    let textures = jar
        .read_prefixes(&[BLOCK_TEXTURES, ITEM_TEXTURES])
        .expect("the texture bulk read succeeds");
    let names: Vec<&str> = textures.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "assets/minecraft/textures/blocks/stone.png",
            "assets/minecraft/textures/blocks/stone.png.mcmeta",
            "assets/minecraft/textures/items/apple.png",
        ]
    );
}

#[test]
fn two_pngs_decode_into_the_expected_sorted_layers() {
    let dir = TempDir::new("layers");
    let dirt = solid(2, 2, [20, 200, 40, 255]);
    let stone = solid(2, 2, [120, 120, 120, 255]);
    let jar = jar_with(
        &dir,
        &[
            (
                "assets/minecraft/blockstates/stone.json",
                b"{\"variants\":{}}",
            ),
            (
                "assets/minecraft/models/block/stone.json",
                b"{\"parent\":\"block/cube_all\"}",
            ),
            (block_path("stone").as_str(), &stone),
            (block_path("dirt").as_str(), &dirt),
            (
                "assets/minecraft/textures/blocks/dirt.png.mcmeta",
                b"{\"animation\":{}}",
            ),
        ],
    );

    let resources = ClientResources::load(&jar).expect("the synthetic resources load");
    let atlas = resources.atlas();
    assert_eq!(atlas.width(), 2);
    assert_eq!(atlas.height(), 2);
    assert_eq!(atlas.layers(), 3);
    assert_eq!(atlas.pixels().len(), 2 * 2 * 4 * 3);
    assert_eq!(
        atlas.layout().keys(),
        [MISSING_TEXTURE, "blocks/dirt", "blocks/stone"]
    );
    assert_eq!(atlas.layout().layer("blocks/dirt"), Some(1));
    assert_eq!(atlas.layout().layer("blocks/stone"), Some(2));
    assert_eq!(texel(atlas, 1, 0, 0), [20, 200, 40, 255]);
    assert_eq!(texel(atlas, 2, 0, 0), [120, 120, 120, 255]);

    assert_eq!(resources.file_count(), 5);
    assert_eq!(
        resources.get("assets/minecraft/textures/blocks/dirt.png.mcmeta"),
        Some(&b"{\"animation\":{}}"[..])
    );
    assert!(resources
        .get("assets/minecraft/textures/entity/steve.png")
        .is_none());
}

#[test]
fn a_tall_animation_strip_contributes_only_its_first_frame() {
    let dir = TempDir::new("animated");
    // Two 2x2 frames stacked vertically: red then green.
    let mut animated_pixels = vec![[255, 0, 0, 255]; 4];
    animated_pixels.extend_from_slice(&[[0, 255, 0, 255]; 4]);
    let animated = png_rgba(2, 4, &animated_pixels);
    let solid_png = solid(2, 2, [0, 0, 255, 255]);
    let jar = jar_with(
        &dir,
        &[
            (block_path("animated").as_str(), &animated),
            (block_path("solid").as_str(), &solid_png),
        ],
    );

    let resources = ClientResources::load(&jar).expect("the animation strip loads");
    let atlas = resources.atlas();
    assert_eq!((atlas.width(), atlas.height()), (2, 2));
    assert_eq!(atlas.layers(), 3);
    let animated_layer = atlas
        .layout()
        .layer("blocks/animated")
        .expect("the animated texture has a layer");
    assert_eq!(texel(atlas, animated_layer, 0, 0), [255, 0, 0, 255]);
    assert_eq!(texel(atlas, animated_layer, 1, 1), [255, 0, 0, 255]);
    assert!(!atlas
        .pixels()
        .windows(4)
        .any(|pixel| pixel == [0, 255, 0, 255]));
}

#[test]
fn the_missing_layer_is_opaque_checker_like_and_not_empty() {
    let dir = TempDir::new("missing");
    let texture = solid(2, 2, [1, 2, 3, 255]);
    let jar = jar_with(&dir, &[(block_path("stone").as_str(), &texture)]);
    let resources = ClientResources::load(&jar).expect("the synthetic resources load");
    let atlas = resources.atlas();
    assert_eq!(atlas.layout().layer(MISSING_TEXTURE), Some(0));
    assert_eq!(atlas.pixels().len(), 2 * 2 * 4 * 2);

    for y in 0..2 {
        for x in 0..2 {
            assert_eq!(texel(atlas, 0, x, y)[3], 255);
        }
    }
    assert_ne!(texel(atlas, 0, 0, 0), texel(atlas, 0, 1, 0));
    assert_eq!(texel(atlas, 0, 0, 0), texel(atlas, 0, 1, 1));
}

#[test]
fn a_wrong_image_size_is_a_typed_image_error() {
    let dir = TempDir::new("wrong-size");
    let small = solid(2, 2, [1, 2, 3, 255]);
    let large = solid(4, 4, [4, 5, 6, 255]);
    let jar = jar_with(
        &dir,
        &[
            (block_path("small").as_str(), &small),
            (block_path("large").as_str(), &large),
        ],
    );

    let err = ClientResources::load(&jar).expect_err("mixed sizes must fail");
    match err {
        AssetsError::Image { path, .. } => assert!(
            path.ends_with("blocks/large.png"),
            "the error names the offending entry, got {path}"
        ),
        other => panic!("expected an image error, got {other:?}"),
    }
}

#[test]
fn a_truncated_png_is_a_typed_image_error() {
    let dir = TempDir::new("truncated");
    let texture = solid(2, 2, [1, 2, 3, 255]);
    let truncated = &texture[..texture.len() / 2];
    let jar = jar_with(&dir, &[(block_path("broken").as_str(), truncated)]);

    let err = ClientResources::load(&jar).expect_err("a truncated PNG must fail");
    match err {
        AssetsError::Image { path, .. } => {
            assert!(path.ends_with("blocks/broken.png"), "got {path}")
        }
        other => panic!("expected an image error, got {other:?}"),
    }
}

#[test]
fn resource_source_reads_and_lists_the_loaded_files() {
    let dir = TempDir::new("resource-source");
    let blockstate = b"{\"variants\":{}}";
    let texture = solid(2, 2, [9, 8, 7, 255]);
    let jar = jar_with(
        &dir,
        &[
            ("assets/minecraft/blockstates/stone.json", blockstate),
            (block_path("stone").as_str(), &texture),
            (
                "assets/minecraft/textures/blocks/stone.png.mcmeta",
                b"{\"animation\":{}}",
            ),
        ],
    );
    let resources = ClientResources::load(&jar).expect("the synthetic resources load");

    assert_eq!(
        resources
            .read("assets/minecraft/blockstates/stone.json")
            .expect("the blockstate is readable"),
        blockstate
    );
    assert_eq!(
        resources.list(BLOCK_TEXTURES),
        vec![
            block_path("stone"),
            "assets/minecraft/textures/blocks/stone.png.mcmeta".to_string(),
        ]
    );
    assert!(resources.list("assets/minecraft/textures/nope/").is_empty());

    let err = resources
        .read("assets/minecraft/models/block/missing.json")
        .expect_err("a missing resource must fail");
    match err {
        umicraft_model::ModelError::Resource { path, .. } => {
            assert_eq!(path, "assets/minecraft/models/block/missing.json")
        }
        other => panic!("expected a resource error, got {other:?}"),
    }
}

/// Reads the local official 1.8.8 installation and reports its exact counts.
///
/// This test is ignored by default because it depends on a user-owned
/// installation. It never writes an asset into the repository.
#[test]
#[ignore = "requires the official 1.8.8 installation in ~/.minecraft"]
fn official_1_8_8_counts() {
    let Some(install) = umicraft_assets::install::OfficialInstall::from_env() else {
        eprintln!("skipping: no official installation found");
        return;
    };
    let version_dir = install.versions_dir().join("1.8.8");
    let json_path = version_dir.join("1.8.8.json");
    let Ok(json) = std::fs::read(&json_path) else {
        eprintln!("skipping: {} is absent", json_path.display());
        return;
    };
    let version = umicraft_assets::manifest::VersionJson::parse(&json)
        .expect("the official version JSON parses");
    let descriptor = version
        .require_client_jar()
        .expect("the version has a client jar descriptor");
    let jar_path = install
        .client_jar_path("1.8.8")
        .expect("the official client jar is present");
    let jar = ClientJar::open(jar_path, &descriptor.sha1, descriptor.size)
        .expect("the official jar verifies");
    let resources = ClientResources::load(&jar).expect("the official resources load");

    let blockstates = resources
        .list(BLOCKSTATES)
        .iter()
        .filter(|path| path.ends_with(".json"))
        .count();
    let models = resources
        .list(MODELS)
        .iter()
        .filter(|path| path.ends_with(".json"))
        .count();
    let block_entries = resources.list(BLOCK_TEXTURES);
    let item_entries = resources.list(ITEM_TEXTURES);
    let block_pngs = block_entries
        .iter()
        .filter(|path| path.ends_with(".png"))
        .count();
    let item_pngs = item_entries
        .iter()
        .filter(|path| path.ends_with(".png"))
        .count();
    let block_meta = block_entries
        .iter()
        .filter(|path| path.ends_with(".mcmeta"))
        .count();
    let item_meta = item_entries
        .iter()
        .filter(|path| path.ends_with(".mcmeta"))
        .count();

    println!(
        "blockstates       : {blockstates}\n\
         source models     : {models}\n\
         block textures    : {} ({block_pngs} png + {block_meta} mcmeta)\n\
         item textures     : {} ({item_pngs} png + {item_meta} mcmeta)\n\
         resource files    : {}\n\
         texture layers    : {}",
        block_entries.len(),
        item_entries.len(),
        resources.file_count(),
        resources.atlas().layers()
    );
    println!(
        "atlas             : {}x{} RGBA8, {} bytes",
        resources.atlas().width(),
        resources.atlas().height(),
        resources.atlas().pixels().len()
    );

    assert_eq!(blockstates, 340);
    assert_eq!(models, 1080);
    assert_eq!(block_entries.len(), 382);
    assert_eq!(item_entries.len(), 229);
    assert_eq!(block_pngs, 373);
    assert_eq!(item_pngs, 227);
    assert_eq!(block_meta, 9);
    assert_eq!(item_meta, 2);
    assert_eq!(resources.file_count(), 2031);
    assert_eq!(resources.atlas().layers(), 601);
    assert_eq!(
        (resources.atlas().width(), resources.atlas().height()),
        (16, 16)
    );
    assert_eq!(resources.atlas().pixels().len(), 16 * 16 * 4 * 601);
}

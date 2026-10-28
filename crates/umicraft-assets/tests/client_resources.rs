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

/// An animation strip with `.mcmeta` animates its layer (water, lava, fire),
/// and one without metadata stays on its first frame.
#[test]
fn a_strip_with_mcmeta_becomes_an_animated_layer() {
    let dir = TempDir::new("animated-layer");
    let mut strip = vec![[255, 0, 0, 255]; 4];
    strip.extend_from_slice(&[[0, 255, 0, 255]; 4]);
    let strip_png = png_rgba(2, 4, &strip);
    let meta_path = format!("{}.mcmeta", block_path("water"));
    let jar = jar_with(
        &dir,
        &[
            (block_path("water").as_str(), &strip_png),
            (meta_path.as_str(), br#"{"animation": {"frametime": 2}}"#),
            (block_path("plain").as_str(), &strip_png),
        ],
    );

    let resources = ClientResources::load(&jar).expect("the animation loads");
    let atlas = resources.atlas();
    let water = atlas.layout().layer("blocks/water").expect("water layer");
    let mut animations = atlas.animations().to_vec();
    assert_eq!(animations.len(), 1, "only the strip with metadata animates");
    assert_eq!(animations[0].layer(), water);
    // The layer starts on frame 0 and the second frame arrives after 2 ticks.
    assert_eq!(texel(atlas, water, 0, 0), [255, 0, 0, 255]);
    assert!(animations[0].tick().is_none());
    let second = animations[0].tick().expect("the frame changes");
    assert_eq!(second.len(), 2 * 2 * 4);
    assert!(second
        .chunks_exact(4)
        .all(|pixel| pixel == [0, 255, 0, 255]));
}

/// The compass and the clock keep every frame of their strip for the game
/// state to choose from; the layer starts on the first one and no timeline
/// animation is created for them.
#[test]
fn the_compass_and_the_clock_become_dial_sprites() {
    let dir = TempDir::new("dials");
    let mut strip = vec![[255, 0, 0, 255]; 4];
    strip.extend_from_slice(&[[0, 255, 0, 255]; 4]);
    let strip_png = png_rgba(2, 4, &strip);
    let item = |name: &str| format!("assets/minecraft/textures/items/{name}.png");
    let compass_meta = format!("{}.mcmeta", item("compass"));
    let jar = jar_with(
        &dir,
        &[
            (item("compass").as_str(), &strip_png),
            (compass_meta.as_str(), br#"{"animation": {}}"#),
            (item("clock").as_str(), &strip_png),
            (
                item("apple").as_str(),
                &png_rgba(2, 2, &[[1, 2, 3, 255]; 4]),
            ),
        ],
    );

    let resources = ClientResources::load(&jar).expect("the dials load");
    let atlas = resources.atlas();
    assert!(atlas.animations().is_empty());
    let dials = atlas.dials();
    assert_eq!(dials.len(), 2);
    let compass = dials
        .iter()
        .find(|dial| dial.kind() == umicraft_assets::DialKind::Compass)
        .expect("compass");
    assert_eq!(
        compass.layer(),
        atlas.layout().layer("items/compass").unwrap()
    );
    assert_eq!(compass.frame_count(), 2);
    assert_eq!(texel(atlas, compass.layer(), 0, 0), [255, 0, 0, 255]);
    assert!(compass
        .frame(1)
        .unwrap()
        .chunks_exact(4)
        .all(|pixel| pixel == [0, 255, 0, 255]));
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
fn mixed_square_sizes_upscale_to_the_largest_frame() {
    let dir = TempDir::new("mixed-sizes");
    let small = solid(16, 16, [10, 20, 30, 255]);
    let large = solid(32, 32, [40, 50, 60, 255]);
    let jar = jar_with(
        &dir,
        &[
            (block_path("small").as_str(), &small),
            (block_path("large").as_str(), &large),
        ],
    );

    let resources = ClientResources::load(&jar).expect("mixed square sizes are normalised");
    let atlas = resources.atlas();
    assert_eq!((atlas.width(), atlas.height()), (32, 32));
    assert_eq!(atlas.layers(), 3);
    assert_eq!(atlas.pixels().len(), 32 * 32 * 4 * 3);
    let small_layer = atlas
        .layout()
        .layer("blocks/small")
        .expect("the small texture has a layer");
    let large_layer = atlas
        .layout()
        .layer("blocks/large")
        .expect("the large texture has a layer");
    assert_eq!(texel(atlas, small_layer, 0, 0), [10, 20, 30, 255]);
    assert_eq!(texel(atlas, small_layer, 31, 31), [10, 20, 30, 255]);
    assert_eq!(texel(atlas, large_layer, 31, 31), [40, 50, 60, 255]);
}

#[test]
fn a_32_wide_animation_strip_keeps_the_full_first_frame() {
    let dir = TempDir::new("wide-animation");
    let red = [255, 0, 0, 255];
    let green = [0, 255, 0, 255];
    let blue = [0, 0, 255, 255];
    let yellow = [255, 255, 0, 255];
    let black = [0, 0, 0, 255];
    // Two 32x32 frames. The first has one distinct colour per 16x16 quadrant;
    // the second is black and must not leak into the atlas layer.
    let mut pixels = vec![black; 32 * 64];
    for y in 0..16 {
        for x in 0..16 {
            pixels[y * 32 + x] = red;
            pixels[y * 32 + x + 16] = green;
            pixels[(y + 16) * 32 + x] = blue;
            pixels[(y + 16) * 32 + x + 16] = yellow;
        }
    }
    let flow = png_rgba(32, 64, &pixels);
    // A 16x16 texture would make the most-common-size heuristic choose 16;
    // the largest-frame rule must still choose 32.
    let stone = solid(16, 16, [90, 80, 70, 255]);
    let jar = jar_with(
        &dir,
        &[
            (block_path("flow").as_str(), &flow),
            (block_path("stone").as_str(), &stone),
        ],
    );

    let resources = ClientResources::load(&jar).expect("the wide animation strip loads");
    let atlas = resources.atlas();
    assert_eq!((atlas.width(), atlas.height()), (32, 32));
    let layer = atlas
        .layout()
        .layer("blocks/flow")
        .expect("the flow texture has a layer");
    assert_eq!(texel(atlas, layer, 0, 0), red);
    assert_eq!(texel(atlas, layer, 16, 0), green);
    assert_eq!(texel(atlas, layer, 0, 16), blue);
    assert_eq!(texel(atlas, layer, 16, 16), yellow);
    assert_eq!(texel(atlas, layer, 31, 31), yellow);
    assert!(!atlas.pixels().windows(4).any(|pixel| pixel == black));
}

#[test]
fn a_16px_texture_upscales_with_2x2_pixel_replication() {
    let dir = TempDir::new("upscale");
    let mut source = Vec::with_capacity(16 * 16);
    for y in 0..16_u8 {
        for x in 0..16_u8 {
            source.push([x * 16 + y, 255 - (x * 16 + y), x * 16, 255 - x]);
        }
    }
    let small = png_rgba(16, 16, &source);
    let large = solid(32, 32, [1, 2, 3, 255]);
    let jar = jar_with(
        &dir,
        &[
            (block_path("small").as_str(), &small),
            (block_path("large").as_str(), &large),
        ],
    );

    let resources = ClientResources::load(&jar).expect("the small texture upscales");
    let atlas = resources.atlas();
    assert_eq!((atlas.width(), atlas.height()), (32, 32));
    let layer = atlas
        .layout()
        .layer("blocks/small")
        .expect("the small texture has a layer");
    for y in 0..16 {
        for x in 0..16 {
            let color = source[y * 16 + x];
            for (dx, dy) in [(0_u32, 0_u32), (1, 0), (0, 1), (1, 1)] {
                assert_eq!(
                    texel(atlas, layer, 2 * x as u32 + dx, 2 * y as u32 + dy),
                    color,
                    "source pixel ({x}, {y}) is replicated at ({}, {})",
                    2 * x + dx as usize,
                    2 * y + dy as usize
                );
            }
        }
    }
}

#[test]
fn an_image_that_is_neither_square_nor_an_animation_strip_is_rejected() {
    for (name, width, height) in [("tall", 2, 3), ("wide", 4, 2)] {
        let dir = TempDir::new(&format!("invalid-{name}"));
        let normal = solid(2, 2, [1, 2, 3, 255]);
        let invalid = solid(width, height, [4, 5, 6, 255]);
        let jar = jar_with(
            &dir,
            &[
                (block_path("normal").as_str(), &normal),
                (block_path(name).as_str(), &invalid),
            ],
        );

        let err = ClientResources::load(&jar).expect_err("an irregular image must fail");
        match err {
            AssetsError::Image { path, .. } => assert!(
                path.ends_with(&format!("blocks/{name}.png")),
                "the error names the offending entry, got {path}"
            ),
            other => panic!("expected an image error, got {other:?}"),
        }
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

    let colormaps = resources
        .colormaps()
        .expect("the official 1.8.8 jar provides both colormaps");
    assert_ne!(
        colormaps.grass.sample(0.8, 0.4),
        [0xFF; 3],
        "plains grass must not sample the white corner"
    );
    assert_ne!(
        colormaps.foliage.sample(0.8, 0.4),
        [0xFF; 3],
        "plains foliage must not sample the white corner"
    );

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

    // Water, lava, fire and portal animate (the live-game regression: they
    // showed a frozen first frame).
    let animated: Vec<_> = resources
        .atlas()
        .animations()
        .iter()
        .filter_map(|animation| {
            resources
                .atlas()
                .layout()
                .keys()
                .get(animation.layer() as usize)
        })
        .collect();
    println!("animated layers   : {animated:?}");
    for key in [
        "blocks/water_still",
        "blocks/water_flow",
        "blocks/lava_still",
        "blocks/lava_flow",
    ] {
        assert!(animated.iter().any(|animated| *animated == key), "{key}");
    }
    assert!(
        animated.iter().all(|key| key.starts_with("blocks/")),
        "the clock and the compass are not timeline-driven"
    );

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
        (32, 32)
    );
    assert_eq!(resources.atlas().pixels().len(), 32 * 32 * 4 * 601);
}

/// A 256×256 colormap whose pixel `(x, y)` is `[x, y, 7, 255]`.
fn coordinate_colormap() -> Vec<u8> {
    let mut pixels = Vec::with_capacity(256 * 256);
    for y in 0..256 {
        for x in 0..256 {
            pixels.push([x as u8, y as u8, 7, 255]);
        }
    }
    png_rgba(256, 256, &pixels)
}

const GRASS_COLORMAP: &str = "assets/minecraft/textures/colormap/grass.png";
const FOLIAGE_COLORMAP: &str = "assets/minecraft/textures/colormap/foliage.png";

#[test]
fn both_colormaps_are_decoded_outside_the_atlas() {
    let dir = TempDir::new("colormaps");
    let foliage = solid(256, 256, [9, 8, 7, 255]);
    let jar = jar_with(
        &dir,
        &[
            (GRASS_COLORMAP, &coordinate_colormap()),
            (FOLIAGE_COLORMAP, &foliage),
        ],
    );
    let resources = ClientResources::load(&jar).expect("the colormaps load");

    let colormaps = resources.colormaps().expect("both colormaps are present");
    assert_eq!(colormaps.grass.sample(1.0, 1.0), [0, 0, 7]);
    assert_eq!(colormaps.grass.sample(0.0, 0.0), [255, 255, 7]);
    assert_eq!(colormaps.grass.sample(0.5, 0.5), [127, 191, 7]);
    assert_eq!(colormaps.foliage.sample(0.5, 0.5), [9, 8, 7]);

    // The maps stay out of the texture array and the resource map.
    assert!(resources.atlas().layout().layer("colormap/grass").is_none());
    assert!(resources.get(GRASS_COLORMAP).is_none());
    assert_eq!(resources.file_count(), 0);
}

#[test]
fn one_missing_colormap_disables_tinting() {
    let dir = TempDir::new("colormap-missing");
    let jar = jar_with(&dir, &[(GRASS_COLORMAP, &coordinate_colormap())]);
    let resources = ClientResources::load(&jar).expect("a partial colormap set loads");
    assert!(resources.colormaps().is_none());
}

#[test]
fn a_wrong_sized_colormap_is_rejected() {
    let dir = TempDir::new("colormap-size");
    let jar = jar_with(
        &dir,
        &[
            (GRASS_COLORMAP, &solid(16, 16, [1, 2, 3, 255])),
            (FOLIAGE_COLORMAP, &solid(256, 256, [1, 2, 3, 255])),
        ],
    );
    let Err(AssetsError::Image { path, reason }) = ClientResources::load(&jar) else {
        panic!("a 16x16 colormap must be rejected");
    };
    assert_eq!(path, GRASS_COLORMAP);
    assert!(reason.contains("256x256"), "reason: {reason}");
}

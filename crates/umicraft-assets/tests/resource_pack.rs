// SPDX-License-Identifier: MIT OR Apache-2.0

//! Acceptance and unit tests for 1.8 resource packs and resource layering.

use std::path::PathBuf;

use umicraft_assets::client_jar::ClientJar;
use umicraft_assets::hash;
use umicraft_assets::{ClientResources, PackMetadata, ResourcePack, ResourceStack};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "umicraft-assets-pack-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("the temp directory is created");
        Self { path }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::with_capacity(entries.len());

    for (name, payload) in entries {
        let name_bytes = name.as_bytes();
        let offset = out.len() as u32;
        out.extend_from_slice(&0x0403_4b50_u32.to_le_bytes());
        out.extend_from_slice(&10_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u32.to_le_bytes());
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
        out.extend_from_slice(&0x0201_4b50_u32.to_le_bytes());
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

    out.extend_from_slice(&0x0605_4b50_u32.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out.extend_from_slice(&(central.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u16).to_le_bytes());
    out.extend_from_slice(&central_size.to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out
}

fn png_rgba(width: u32, height: u32, pixels: &[[u8; 4]]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("the PNG header is written");
        let raw: Vec<u8> = pixels.iter().flatten().copied().collect();
        writer.write_image_data(&raw).expect("the pixels are written");
    }
    bytes
}

fn solid(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
    let count = (width * height) as usize;
    png_rgba(width, height, &vec![color; count])
}

fn jar_with(dir: &TempDir, entries: &[(&str, &[u8])]) -> ClientJar {
    let bytes = stored_zip(entries);
    let path = dir.join("client.jar");
    std::fs::write(&path, &bytes).expect("the synthetic jar is written");
    ClientJar::open(&path, &hash::sha1_hex(&bytes), bytes.len() as u64)
        .expect("the synthetic jar verifies")
}

#[test]
fn pack_metadata_parses_simple_and_formatted_descriptions() {
    let simple = br#"{
        "pack": {
            "pack_format": 1,
            "description": "My custom pack"
        }
    }"#;
    let parsed = PackMetadata::parse(simple).expect("simple metadata parses");
    assert_eq!(parsed.pack_format, 1);
    assert_eq!(parsed.description, "My custom pack");

    let formatted = br#"{
        "pack": {
            "pack_format": 1,
            "description": {
                "text": "Color text description",
                "color": "yellow"
            }
        }
    }"#;
    let parsed = PackMetadata::parse(formatted).expect("formatted metadata parses");
    assert_eq!(parsed.pack_format, 1);
    assert_eq!(parsed.description, "Color text description");
}

#[test]
fn resource_pack_loads_from_directory() {
    let dir = TempDir::new("dir-pack");
    let pack_dir = dir.join("cool_pack");
    std::fs::create_dir_all(pack_dir.join("assets/minecraft/textures/blocks"))
        .expect("directories created");

    std::fs::write(
        pack_dir.join("pack.mcmeta"),
        br#"{"pack": {"pack_format": 1, "description": "Test pack"}}"#,
    )
    .expect("mcmeta written");

    let stone_png = solid(16, 16, [100, 100, 100, 255]);
    std::fs::write(
        pack_dir.join("assets/minecraft/textures/blocks/stone.png"),
        &stone_png,
    )
    .expect("stone written");

    let pack = ResourcePack::from_directory(&pack_dir).expect("directory pack loads");
    assert_eq!(pack.id(), "cool_pack");
    assert_eq!(
        pack.metadata().map(|meta| meta.description.as_str()),
        Some("Test pack")
    );
    assert_eq!(pack.file_count(), 1);
    assert!(pack
        .get("assets/minecraft/textures/blocks/stone.png")
        .is_some());
}

#[test]
fn resource_pack_loads_from_zip_bytes() {
    let stone_png = solid(16, 16, [50, 50, 50, 255]);
    let mcmeta = br#"{"pack": {"pack_format": 1, "description": "Zip pack"}}"#;
    let zip_data = stored_zip(&[
        ("pack.mcmeta", mcmeta),
        ("assets/minecraft/textures/blocks/dirt.png", &stone_png),
    ]);

    let pack = ResourcePack::from_zip_bytes("zip_pack", zip_data).expect("zip pack loads");
    assert_eq!(pack.id(), "zip_pack");
    assert_eq!(
        pack.metadata().map(|meta| meta.description.as_str()),
        Some("Zip pack")
    );
    assert_eq!(pack.file_count(), 1);
    assert!(pack
        .get("assets/minecraft/textures/blocks/dirt.png")
        .is_some());
}

#[test]
fn resource_stack_layers_and_overrides_client_resources() {
    let dir = TempDir::new("stack-layering");
    let base_stone = solid(16, 16, [10, 10, 10, 255]);
    let base_dirt = solid(16, 16, [20, 20, 20, 255]);
    let base_jar = jar_with(
        &dir,
        &[
            ("assets/minecraft/textures/blocks/stone.png", &base_stone),
            ("assets/minecraft/textures/blocks/dirt.png", &base_dirt),
            (
                "assets/minecraft/models/block/stone.json",
                b"{\"parent\": \"block/cube_all\"}",
            ),
        ],
    );

    let base_resources = ClientResources::load(&base_jar).expect("base jar loads");
    assert_eq!(base_resources.atlas().layout().layer_count(), 3); // missing + stone + dirt

    // Pack 1 overrides stone texture with 16x16 red
    let red_stone = solid(16, 16, [255, 0, 0, 255]);
    let pack1 = ResourcePack::from_parts(
        "pack1",
        None,
        None,
        [
            (
                "assets/minecraft/textures/blocks/stone.png".to_string(),
                red_stone,
            ),
            (
                "assets/minecraft/models/block/stone.json".to_string(),
                b"{\"parent\": \"block/custom_stone\"}".to_vec(),
            ),
        ]
        .into_iter()
        .collect(),
    );

    let mut stack = ResourceStack::new();
    stack.push(pack1);

    let layered = stack
        .apply_to_resources(&base_resources)
        .expect("stack applies to resources");

    // Check overridden model json
    assert_eq!(
        layered.get("assets/minecraft/models/block/stone.json"),
        Some(b"{\"parent\": \"block/custom_stone\"}".as_slice())
    );

    // Dirt remains untouched
    assert!(layered
        .get("assets/minecraft/textures/blocks/dirt.png")
        .is_some());

    // Layer 1 is stone
    let stone_layer = layered
        .atlas()
        .layout()
        .layer("blocks/stone")
        .expect("stone layer exists");
    let layer_size = (layered.atlas().width() * layered.atlas().height() * 4) as usize;
    let offset = stone_layer as usize * layer_size;
    let layer_bytes = &layered.atlas().pixels()[offset..offset + layer_size];
    assert_eq!(&layer_bytes[0..4], &[255, 0, 0, 255]); // top-left pixel is red!
}

#[test]
fn resource_pack_ignores_macos_and_hidden_metadata_files() {
    let mcmeta = br#"{"pack": {"pack_format": 1, "description": "Mac pack"}}"#;
    let stone_png = solid(16, 16, [100, 100, 100, 255]);
    let fake_metadata = b"AppleDouble binary junk";

    let zip_data = stored_zip(&[
        ("pack.mcmeta", mcmeta),
        ("__MACOSX/pack.mcmeta", fake_metadata),
        (
            "assets/minecraft/textures/gui/container/creative_inventory/._tab_inventory.png",
            fake_metadata,
        ),
        (
            "assets/minecraft/textures/blocks/.DS_Store",
            fake_metadata,
        ),
        ("assets/minecraft/textures/blocks/stone.png", &stone_png),
    ]);

    let pack = ResourcePack::from_zip_bytes("mac_pack", zip_data).expect("zip pack loads");
    assert_eq!(pack.file_count(), 1);
    assert!(pack
        .get("assets/minecraft/textures/blocks/stone.png")
        .is_some());
    assert!(pack
        .get("assets/minecraft/textures/gui/container/creative_inventory/._tab_inventory.png")
        .is_none());
    assert!(pack
        .get("assets/minecraft/textures/blocks/.DS_Store")
        .is_none());
}

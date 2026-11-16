// SPDX-License-Identifier: MIT OR Apache-2.0

//! Criterion benchmarks for asset management and loading hot paths:
//! - Zip archive opening, index construction and prefix search.
//! - Parallel PNG decoding and texture atlas construction.
//! - Resource pack parsing from zip bytes and layering overrides.

use std::hint::black_box;
use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, Criterion};
use umicraft_assets::client_jar::ClientJar;
use umicraft_assets::hash;
use umicraft_assets::resource_pack::ResourcePack;
use umicraft_assets::ClientResources;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "umicraft-bench-assets-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("bench temp dir created");
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

fn png_rgba(width: u32, height: u32, pixels: &[[u8; 4]]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("PNG header");
        let flat: Vec<u8> = pixels.iter().flatten().copied().collect();
        writer.write_image_data(&flat).expect("PNG pixels");
    }
    bytes
}

fn solid_png(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
    png_rgba(width, height, &vec![color; (width * height) as usize])
}

fn bench_assets(c: &mut Criterion) {
    let png_16 = solid_png(16, 16, [100, 150, 200, 255]);
    let png_32 = solid_png(32, 32, [200, 100, 50, 255]);

    // Build synthetic jar with 200 files (100 block textures, 50 item textures, 50 other files)
    let temp = TempDir::new("jar");
    let mut names: Vec<String> = Vec::new();
    for i in 0..100 {
        names.push(format!("assets/minecraft/textures/blocks/block_{i}.png"));
    }
    for i in 0..50 {
        names.push(format!("assets/minecraft/textures/items/item_{i}.png"));
    }
    for i in 0..50 {
        names.push(format!("net/minecraft/server/Class_{i}.class"));
    }

    let entries: Vec<(&str, &[u8])> = names
        .iter()
        .map(|name| {
            let bytes: &[u8] = if name.contains("blocks/") {
                &png_16
            } else if name.contains("items/") {
                &png_32
            } else {
                b"class_dummy_bytecode"
            };
            (name.as_str(), bytes)
        })
        .collect();

    let jar_bytes = stored_zip(&entries);
    let jar_path = temp.join("test.jar");
    std::fs::write(&jar_path, &jar_bytes).expect("synthetic jar written");
    let jar = ClientJar::open(
        &jar_path,
        &hash::sha1_hex(&jar_bytes),
        jar_bytes.len() as u64,
    )
    .expect("jar opens");

    c.bench_function("assets/zip_read_prefixes_200_entries", |b| {
        b.iter(|| {
            let res = jar
                .read_prefixes(&[
                    "assets/minecraft/textures/blocks/",
                    "assets/minecraft/textures/items/",
                ])
                .expect("prefixes read");
            black_box(res.len())
        });
    });

    // Benchmark ClientResources load on synthetic jar
    c.bench_function("assets/client_resources_load_synthetic", |b| {
        b.iter(|| {
            let res = ClientResources::load(&jar).expect("client resources load");
            black_box(res.file_count())
        });
    });

    // Benchmark resource pack loading from zip bytes
    let pack_png = solid_png(16, 16, [255, 0, 0, 255]);
    let pack_entries: Vec<(&str, &[u8])> = vec![
        ("pack.mcmeta", br#"{"pack":{"pack_format":1,"description":"Bench pack"}}"#),
        ("assets/minecraft/textures/blocks/block_0.png", &pack_png),
        ("assets/minecraft/textures/blocks/block_1.png", &pack_png),
    ];
    let pack_zip_bytes = stored_zip(&pack_entries);

    c.bench_function("assets/resource_pack_from_zip_bytes", |b| {
        b.iter(|| {
            let pack = ResourcePack::from_zip_bytes("bench_pack", pack_zip_bytes.clone())
                .expect("pack loads");
            black_box(pack.id().to_string())
        });
    });

    // Benchmark resource pack layering override
    let base_res = ClientResources::load(&jar).expect("base res");
    let pack = ResourcePack::from_zip_bytes("bench_pack", pack_zip_bytes).expect("pack loads");
    let packs = [pack];

    c.bench_function("assets/client_resources_apply_packs", |b| {
        b.iter(|| {
            let layered = base_res.apply_packs(&packs).expect("apply packs");
            black_box(layered.file_count())
        });
    });
}

criterion_group!(benches, bench_assets);
criterion_main!(benches);

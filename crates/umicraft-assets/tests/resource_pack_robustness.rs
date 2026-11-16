// SPDX-License-Identifier: MIT OR Apache-2.0

//! Resource packs that are unusual, broken or hostile, through the real
//! loading path: archive bytes, [`ResourcePack`], then
//! [`ClientResources::apply_packs`] over a synthetic client jar.

use std::io::Write as _;
use std::path::PathBuf;

use umicraft_assets::client_jar::ClientJar;
use umicraft_assets::{hash, AssetsError, ClientResources, ResourcePack, ResourceStack};

const STONE: &str = "assets/minecraft/textures/blocks/stone.png";
const DIRT: &str = "assets/minecraft/textures/blocks/dirt.png";
const WIDGETS: &str = "assets/minecraft/textures/gui/widgets.png";
const GRASS_MAP: &str = "assets/minecraft/textures/colormap/grass.png";
const FOLIAGE_MAP: &str = "assets/minecraft/textures/colormap/foliage.png";
const GLINT: &str = "assets/minecraft/textures/misc/enchanted_item_glint.png";
const MCMETA: &[u8] = br#"{"pack": {"pack_format": 1, "description": "Test"}}"#;

/// One entry of a hand-built archive.
struct Entry {
    name: Vec<u8>,
    /// The bytes as they sit in the archive.
    data: Vec<u8>,
    method: u16,
    /// The uncompressed size the headers declare.
    declared: u32,
}

fn stored(name: &str, payload: &[u8]) -> Entry {
    Entry {
        name: name.as_bytes().to_vec(),
        data: payload.to_vec(),
        method: 0,
        declared: payload.len() as u32,
    }
}

fn deflated(name: &str, payload: &[u8]) -> Entry {
    let mut encoder =
        flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(payload).expect("deflate");
    Entry {
        name: name.as_bytes().to_vec(),
        data: encoder.finish().expect("deflate"),
        method: 8,
        declared: payload.len() as u32,
    }
}

fn zip(entries: &[Entry]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut offsets = Vec::with_capacity(entries.len());
    let sizes = |out: &mut Vec<u8>, entry: &Entry| {
        out.extend_from_slice(&0_u32.to_le_bytes()); // crc, not checked
        out.extend_from_slice(&(entry.data.len() as u32).to_le_bytes());
        out.extend_from_slice(&entry.declared.to_le_bytes());
        out.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
    };
    for entry in entries {
        offsets.push(out.len() as u32);
        out.extend_from_slice(&0x0403_4b50_u32.to_le_bytes());
        out.extend_from_slice(&[20, 0, 0, 0]); // version, flags
        out.extend_from_slice(&entry.method.to_le_bytes());
        out.extend_from_slice(&[0; 4]); // time, date
        sizes(&mut out, entry);
        out.extend_from_slice(&0_u16.to_le_bytes()); // extra
        out.extend_from_slice(&entry.name);
        out.extend_from_slice(&entry.data);
    }
    let central_offset = out.len() as u32;
    for (entry, offset) in entries.iter().zip(&offsets) {
        out.extend_from_slice(&0x0201_4b50_u32.to_le_bytes());
        out.extend_from_slice(&[20, 0, 20, 0, 0, 0]); // made by, version, flags
        out.extend_from_slice(&entry.method.to_le_bytes());
        out.extend_from_slice(&[0; 4]); // time, date
        sizes(&mut out, entry);
        out.extend_from_slice(&[0; 12]); // extra, comment, disk, attributes
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&entry.name);
    }
    let central_size = out.len() as u32 - central_offset;
    out.extend_from_slice(&0x0605_4b50_u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&central_size.to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out
}

fn png(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("header");
    writer
        .write_image_data(&color.repeat((width * height) as usize))
        .expect("pixels");
    writer.finish().expect("finish");
    bytes
}

/// A PNG whose header is right and whose pixels are cut off.
fn broken_png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = png(width, height, [1, 2, 3, 255]);
    bytes.truncate(40);
    bytes
}

const BASE_STONE: [u8; 4] = [10, 10, 10, 255];
const BASE_DIRT: [u8; 4] = [20, 20, 20, 255];

/// The resources of a small synthetic client jar: two blocks, one GUI sheet,
/// the two colormaps and the glint.
fn base(name: &str) -> ClientResources {
    let bytes = zip(&[
        stored(STONE, &png(16, 16, BASE_STONE)),
        stored(DIRT, &png(16, 16, BASE_DIRT)),
        stored(WIDGETS, &png(256, 256, [1, 1, 1, 255])),
        stored(GRASS_MAP, &png(256, 256, [0, 200, 0, 255])),
        stored(FOLIAGE_MAP, &png(256, 256, [0, 100, 0, 255])),
        stored(GLINT, &png(64, 64, [128, 0, 128, 255])),
    ]);
    let path: PathBuf = std::env::temp_dir().join(format!(
        "umicraft-pack-robustness-{name}-{}.jar",
        std::process::id()
    ));
    std::fs::write(&path, &bytes).expect("the synthetic jar is written");
    let jar = ClientJar::open(&path, &hash::sha1_hex(&bytes), bytes.len() as u64)
        .expect("the synthetic jar verifies");
    let resources = ClientResources::load(&jar).expect("the synthetic jar loads");
    let _ = std::fs::remove_file(&path);
    resources
}

/// The first texel of the layer `key`.
fn texel(resources: &ClientResources, key: &str) -> [u8; 4] {
    let atlas = resources.atlas();
    let layer = atlas
        .layout()
        .layer(key)
        .unwrap_or_else(|| panic!("no layer {key}")) as usize;
    let at = layer * (atlas.width() * atlas.height() * 4) as usize;
    atlas.pixels()[at..at + 4].try_into().expect("four bytes")
}

fn pack(entries: &[Entry]) -> ResourcePack {
    ResourcePack::from_zip_bytes("pack", zip(entries)).expect("the pack loads")
}

#[test]
fn a_classic_deflated_pack_overrides_textures_with_their_transparency() {
    let deep = "assets/minecraft/textures/blocks/a/b/c/deep.png";
    let pack = pack(&[
        deflated("pack.mcmeta", MCMETA),
        deflated(STONE, &png(16, 16, [200, 100, 50, 128])),
        deflated(deep, &png(16, 16, [1, 2, 3, 0])),
        deflated("assets/minecraft/sounds/unused.ogg", b"not read by anything"),
        deflated("README.txt", b"not an asset"),
    ]);
    assert_eq!(pack.metadata().map(|meta| meta.pack_format), Some(1));
    assert_eq!(pack.file_count(), 3, "the assets, not the readme");

    let layered = base("classic")
        .apply_packs(&[pack])
        .expect("the pack applies");
    assert_eq!(texel(&layered, "blocks/stone"), [200, 100, 50, 128]);
    assert_eq!(texel(&layered, "blocks/a/b/c/deep"), [1, 2, 3, 0]);
    assert_eq!(texel(&layered, "blocks/dirt"), BASE_DIRT);
}

#[test]
fn an_archive_of_many_entries_loads_them_all() {
    let entries: Vec<Entry> = (0..5000)
        .map(|index| {
            stored(
                &format!("assets/minecraft/models/block/m{index}.json"),
                b"{}",
            )
        })
        .collect();
    assert_eq!(pack(&entries).file_count(), 5000);
}

#[test]
fn a_texture_that_is_not_square_gives_way_to_the_one_underneath() {
    let pack = pack(&[
        stored(STONE, &png(32, 16, [255, 0, 0, 255])),
        stored(DIRT, &png(16, 16, [0, 0, 255, 255])),
    ]);
    let layered = base("wide").apply_packs(&[pack]).expect("the pack applies");
    assert_eq!(texel(&layered, "blocks/stone"), BASE_STONE);
    assert_eq!(texel(&layered, "blocks/dirt"), [0, 0, 255, 255]);
}

#[test]
fn a_large_texture_is_capped_in_the_array_and_kept_whole_beside_it() {
    let apple = "assets/minecraft/textures/items/apple.png";
    let pack = pack(&[
        deflated(STONE, &png(512, 512, [9, 9, 9, 255])),
        deflated(apple, &png(512, 512, [7, 7, 7, 255])),
    ]);
    let layered = base("large").apply_packs(&[pack]).expect("the pack applies");
    let atlas = layered.atlas();
    assert_eq!(
        (atlas.width(), atlas.height()),
        (256, 256),
        "layers are capped"
    );
    assert_eq!(texel(&layered, "blocks/stone"), [9, 9, 9, 255]);
    assert_eq!(texel(&layered, "blocks/dirt"), BASE_DIRT, "16 pixels scaled up");
    assert_eq!(atlas.hires_edge(), 512);
    assert_eq!(atlas.hires_layers().len(), 2, "the stone and the apple");
}

#[test]
fn an_animated_strip_with_its_mcmeta_becomes_an_animation() {
    let water = "assets/minecraft/textures/blocks/water_still.png";
    let pack = pack(&[
        stored(water, &png(16, 64, [0, 0, 255, 200])),
        stored(
            &format!("{water}.mcmeta"),
            br#"{"animation": {"frametime": 2}}"#,
        ),
        // A strip without metadata stays on its first frame.
        stored(DIRT, &png(16, 48, [5, 5, 5, 255])),
    ]);
    let layered = base("animated")
        .apply_packs(&[pack])
        .expect("the pack applies");
    assert_eq!(layered.atlas().animations().len(), 1);
    assert_eq!(texel(&layered, "blocks/water_still"), [0, 0, 255, 200]);
    assert_eq!(texel(&layered, "blocks/dirt"), [5, 5, 5, 255]);
}

#[test]
fn a_missing_or_broken_pack_mcmeta_still_loads_the_pack() {
    for mcmeta in [
        None,
        Some(&b"{ not json"[..]),
        Some(&br#"{"nothing": 1}"#[..]),
    ] {
        let mut entries = vec![stored(STONE, &png(16, 16, [3, 3, 3, 255]))];
        entries.extend(mcmeta.map(|bytes| stored("pack.mcmeta", bytes)));
        let pack = pack(&entries);
        assert!(pack.metadata().is_none());
        assert_eq!(pack.file_count(), 1);
    }
}

#[test]
fn a_truncated_archive_is_an_error_and_a_corrupt_entry_costs_only_itself() {
    let mut bytes = zip(&[stored(STONE, &png(16, 16, [3, 3, 3, 255]))]);
    bytes.truncate(bytes.len() - 30);
    let error = ResourcePack::from_zip_bytes("cut", bytes).expect_err("a cut archive fails");
    assert!(matches!(error, AssetsError::Zip { .. }), "{error}");
    let error = ResourcePack::from_zip_bytes("empty", Vec::new()).expect_err("no archive");
    assert!(matches!(error, AssetsError::Zip { .. }), "{error}");

    let mut corrupt = deflated(DIRT, &png(16, 16, [4, 4, 4, 255]));
    corrupt.data = vec![0xff; 40];
    let unknown_method = Entry {
        method: 99,
        ..stored("assets/minecraft/textures/blocks/sand.png", b"lzma")
    };
    let pack = pack(&[
        corrupt,
        unknown_method,
        stored(STONE, &png(16, 16, [3, 3, 3, 255])),
    ]);
    assert_eq!(pack.file_count(), 1);
    assert!(pack.get(STONE).is_some());
}

#[test]
fn unicode_and_legacy_code_page_names_do_not_cost_the_archive() {
    let ice = "assets/minecraft/textures/blocks/лёд.png";
    // "café" in Latin-1: not valid UTF-8.
    let legacy = Entry {
        name: b"assets/minecraft/textures/blocks/caf\xe9.png".to_vec(),
        ..stored("", &png(16, 16, [8, 8, 8, 255]))
    };
    let pack = pack(&[
        stored(ice, &png(16, 16, [6, 6, 6, 255])),
        legacy,
        stored(STONE, &png(16, 16, [3, 3, 3, 255])),
    ]);
    assert_eq!(pack.file_count(), 3);
    let layered = base("unicode")
        .apply_packs(&[pack])
        .expect("the pack applies");
    assert_eq!(texel(&layered, "blocks/лёд"), [6, 6, 6, 255]);
    assert_eq!(texel(&layered, "blocks/stone"), [3, 3, 3, 255]);
}

#[test]
fn the_last_of_two_entries_with_one_name_wins() {
    let pack = pack(&[
        stored(STONE, &png(16, 16, [1, 1, 1, 255])),
        stored(STONE, &png(16, 16, [2, 2, 2, 255])),
    ]);
    assert_eq!(pack.file_count(), 1);
    let layered = base("twice").apply_packs(&[pack]).expect("the pack applies");
    assert_eq!(texel(&layered, "blocks/stone"), [2, 2, 2, 255]);
}

#[test]
fn swapping_packs_leaves_nothing_of_the_previous_one() {
    let base = base("swap");
    let extra = "assets/minecraft/textures/blocks/extra.png";
    let first = || {
        pack(&[
            stored(STONE, &png(16, 16, [255, 0, 0, 255])),
            stored(extra, &png(16, 16, [9, 9, 9, 255])),
        ])
    };
    let second = pack(&[stored(DIRT, &png(16, 16, [0, 0, 255, 255]))]);

    let with_first = base.apply_packs(&[first()]).expect("first");
    assert_eq!(texel(&with_first, "blocks/stone"), [255, 0, 0, 255]);
    assert!(with_first.atlas().layout().layer("blocks/extra").is_some());

    // Hot swap: the second pack, built from the same base.
    let with_second = base.apply_packs(&[second]).expect("second");
    assert_eq!(texel(&with_second, "blocks/stone"), BASE_STONE);
    assert_eq!(texel(&with_second, "blocks/dirt"), [0, 0, 255, 255]);
    assert!(with_second.atlas().layout().layer("blocks/extra").is_none());
    assert!(with_second.get(extra).is_none());

    // Back to the default pack: the base, texel for texel.
    let none = ResourceStack::new()
        .apply_to_resources(&base)
        .expect("none");
    assert_eq!(none.atlas().pixels(), base.atlas().pixels());
    assert_eq!(none.gui_atlas().pixels(), base.gui_atlas().pixels());

    // The same pack again gives the same atlas.
    let again = base.apply_packs(&[first()]).expect("again");
    assert_eq!(again.atlas().pixels(), with_first.atlas().pixels());
    assert_eq!(
        again.atlas().layout().keys(),
        with_first.atlas().layout().keys()
    );
}

/// The reference logs a texture that does not load and goes on
/// (`TextureMap.loadTextureAtlas`); one such file used to fail the whole
/// load, which left the client with its procedural textures.
#[test]
fn a_broken_image_in_a_pack_gives_way_to_the_one_underneath() {
    let base = base("broken");
    let new_item = "assets/minecraft/textures/items/new_item.png";
    let pack = pack(&[
        stored(STONE, &broken_png(16, 16)),
        stored(DIRT, &png(16, 16, [0, 0, 255, 255])),
        stored(new_item, &broken_png(16, 16)),
        stored(WIDGETS, b"not a png"),
        stored(GRASS_MAP, &png(16, 16, [255, 255, 255, 255])),
        stored(GLINT, &broken_png(64, 64)),
    ]);
    let layered = base
        .apply_packs(&[pack])
        .expect("a broken image is not fatal");

    assert_eq!(texel(&layered, "blocks/stone"), BASE_STONE);
    assert_eq!(texel(&layered, "blocks/dirt"), [0, 0, 255, 255]);
    assert!(layered.atlas().layout().layer("items/new_item").is_none());
    assert!(layered.get(new_item).is_none());
    assert_eq!(layered.get(STONE), base.get(STONE), "the base file again");
    assert_eq!(
        layered.gui_atlas().region("widgets").map(|r| r.pixel_width),
        Some(256),
        "the sheet of the base"
    );
    assert_eq!(layered.get(WIDGETS), None, "GUI sheets are not model resources");
    let maps = layered.colormaps().expect("the colormaps of the base");
    assert_eq!(maps.grass.sample(0.5, 0.5), [0, 200, 0]);
    assert_eq!(layered.glint_texture(), base.glint_texture());
}

/// The client jar itself stays strict: a broken image there is an error.
#[test]
fn a_broken_image_in_the_jar_is_still_an_error() {
    let bytes = zip(&[stored(STONE, &broken_png(16, 16))]);
    let path =
        std::env::temp_dir().join(format!("umicraft-broken-jar-{}.jar", std::process::id()));
    std::fs::write(&path, &bytes).expect("written");
    let jar = ClientJar::open(&path, &hash::sha1_hex(&bytes), bytes.len() as u64).expect("jar");
    let result = ClientResources::load(&jar);
    let _ = std::fs::remove_file(&path);
    assert!(matches!(result, Err(AssetsError::Image { .. })));
}

#[test]
fn a_pack_document_can_be_put_back_to_the_base_one() {
    let stone_model = "assets/minecraft/models/block/stone.json";
    let new_model = "assets/minecraft/models/block/new.json";
    let base = base("documents");
    let pack = pack(&[
        stored(stone_model, b"{ broken"),
        stored(new_model, b"{}"),
        stored(STONE, &png(16, 16, [3, 3, 3, 255])),
    ]);
    let mut layered = base.apply_packs(&[pack]).expect("the pack applies");
    assert!(layered.restore_document(&base, new_model));
    assert!(layered.get(new_model).is_none(), "the base has none");
    assert!(!layered.restore_document(&base, new_model), "already back");
    assert!(
        !layered.restore_document(&base, STONE),
        "textures are not documents"
    );
    assert_eq!(layered.restore_documents(&base), 1);
    assert!(layered.get(stone_model).is_none());
    assert_eq!(
        texel(&layered, "blocks/stone"),
        [3, 3, 3, 255],
        "textures stay"
    );
}

#[test]
fn an_entry_that_inflates_past_its_declared_size_is_dropped() {
    let mut bomb = deflated(DIRT, &vec![0_u8; 4 * 1024 * 1024]);
    bomb.declared = 1024;
    let pack = pack(&[bomb, stored(STONE, &png(16, 16, [3, 3, 3, 255]))]);
    assert_eq!(pack.file_count(), 1, "only the honest entry");
}

#[test]
fn a_huge_declared_size_does_not_size_the_allocation() {
    let mut small = deflated(DIRT, b"hello");
    small.declared = 1024 * 1024 * 1024;
    let pack = pack(&[small]);
    assert_eq!(pack.get(DIRT), Some(&b"hello"[..]));
}

#[test]
fn a_pack_that_declares_more_than_the_budget_is_refused() {
    let huge = |name: &str| Entry {
        declared: 0x7fff_ffff,
        ..deflated(name, b"tiny")
    };
    let error = ResourcePack::from_zip_bytes("bomb", zip(&[huge(STONE), huge(DIRT)]))
        .expect_err("more than the budget");
    assert!(matches!(error, AssetsError::Pack { .. }), "{error}");
}

/// Nothing is extracted to disk, so a path that climbs out of the pack is
/// only a key no asset lookup ever asks for.
#[test]
fn a_path_that_climbs_out_of_the_pack_is_only_an_unused_key() {
    let pack = pack(&[
        stored("../../evil.png", &png(16, 16, [1, 1, 1, 255])),
        stored(
            "assets/minecraft/../../evil.png",
            &png(16, 16, [1, 1, 1, 255]),
        ),
        stored(
            "/assets/minecraft/textures/blocks/stone.png",
            &png(16, 16, [4, 4, 4, 255]),
        ),
    ]);
    let before = base("slip");
    let layered = before.apply_packs(&[pack]).expect("the pack applies");
    assert_eq!(
        texel(&layered, "blocks/stone"),
        [4, 4, 4, 255],
        "a leading slash is dropped"
    );
    assert_eq!(
        layered.atlas().layout().layer_count(),
        before.atlas().layout().layer_count()
    );
}

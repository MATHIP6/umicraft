// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the colour formats. Every picture is built in code.

use super::lightmap::{dimension_of, Lightmap, LightmapInput, LightmapPack};
use super::*;
use crate::Properties;

fn rgba(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let [r, g, b] = pixel(x, y);
            bytes.extend([r, g, b, 255]);
        }
    }
    bytes
}

fn ids(name: &str) -> Option<u16> {
    match name {
        "minecraft:grass" => Some(2),
        "minecraft:stone" => Some(1),
        _ => None,
    }
}

fn spec(text: &str, path: &str) -> PaletteSpec {
    let props = Properties::parse(text.as_bytes()).expect("valid properties");
    PaletteSpec::parse(&props, path, Format::Vanilla, &ids).expect("valid spec")
}

fn climate(temperature: f32, rainfall: f32) -> Climate {
    Climate {
        temperature,
        rainfall,
    }
}

#[test]
fn color_properties_read_fixed_colours_and_palette_links() {
    let text = b"fog.nether=330808\nsky.end = 101010\nlilypad=zz\nparticle.water=ff0000\n\
text.code.3=00aa00\ntext.code.40=ffffff\npalette.block.~/colormap/blocks/a.png=stone grass\n\
palette.format=grid\n";
    let colors = ColorProperties::parse(text).expect("parses");
    assert_eq!(colors.fog_nether, Some([0x33, 8, 8]));
    assert_eq!(colors.sky_end, Some([0x10; 3]));
    assert_eq!(colors.lilypad, None, "not hexadecimal");
    assert_eq!(colors.particle_water, Some([255, 0, 0]));
    assert_eq!(colors.text_codes[3], Some([0, 0xaa, 0]));
    assert_eq!(
        colors.text_codes.iter().flatten().count(),
        1,
        "index 40 is out of range"
    );
    assert_eq!(colors.palette_format.as_deref(), Some("grid"));
    assert_eq!(
        colors.palette_blocks,
        [(
            "colormap/blocks/a.png".to_string(),
            "stone grass".to_string()
        )]
    );
}

#[test]
fn an_unknown_palette_format_is_ignored_in_color_properties() {
    let colors = ColorProperties::parse(b"palette.format=weird").expect("parses");
    assert_eq!(colors.palette_format, None);
}

#[test]
fn paths_resolve_against_the_feature_root() {
    assert_eq!(resolve_path("~/colormap/x.png", ""), "colormap/x.png");
    assert_eq!(resolve_path("/~/colormap/x.png", ""), "colormap/x.png");
    assert_eq!(
        resolve_path("./x.png", "colormap/custom"),
        "colormap/custom/x.png"
    );
    assert_eq!(
        resolve_path("assets/minecraft/mcpatcher/colormap/x.png", ""),
        "colormap/x.png"
    );
    assert_eq!(resolve_path("colormap/x.png", "a"), "colormap/x.png");
}

#[test]
fn a_spec_names_its_blocks_by_key_or_by_file_name() {
    let explicit = spec(
        "blocks=stone\nformat=grid\nyOffset=3\nyVariance=2",
        "colormap/custom/a.properties",
    );
    assert_eq!(explicit.format, Format::Grid);
    assert_eq!(explicit.blocks.as_ref().map(|b| b[0].id), Some(1));
    assert_eq!((explicit.y_offset, explicit.y_variance), (3, 2));
    assert_eq!(explicit.source.as_deref(), Some("colormap/custom/a"));

    let by_name = spec("", "colormap/custom/grass.properties");
    assert_eq!(by_name.blocks.as_ref().map(|b| b[0].id), Some(2));
    let by_id = spec("", "colormap/custom/block57.properties");
    assert_eq!(by_id.blocks.as_ref().map(|b| b[0].id), Some(57));
    assert!(spec("", "colormap/custom/nothing.properties")
        .blocks
        .is_none());

    let source = spec("source=./tint.png", "colormap/custom/a.properties");
    assert_eq!(source.source.as_deref(), Some("colormap/custom/tint"));
}

#[test]
fn a_bad_format_is_a_typed_error() {
    let props = Properties::parse(b"format=zigzag").expect("parses");
    assert_eq!(
        PaletteSpec::parse(&props, "a.properties", Format::Vanilla, &ids),
        Err(PaletteError::UnknownFormat("zigzag".to_string()))
    );
}

#[test]
fn the_vanilla_format_reads_temperature_across_and_rainfall_down() {
    let picture = rgba(256, 256, |x, y| [x as u8, y as u8, 7]);
    let palette =
        Palette::from_rgba(&spec("", "x.properties"), 256, 256, &picture).expect("palette");
    // temperature 1 is the left edge, 0 the right; rainfall scales by temperature.
    assert_eq!(palette.sample(climate(1.0, 1.0), 0, 64, [0, 0]), [0, 0, 7]);
    assert_eq!(
        palette.sample(climate(0.0, 1.0), 0, 64, [0, 0]),
        [255, 255, 7]
    );
    // 0.8, 0.4 (plains): x = 0.2 * 255 truncates to 50 (the float 0.8 is a little over), y = (1 - 0.32) * 255 = 173.
    assert_eq!(
        palette.sample(climate(0.8, 0.4), 0, 64, [0, 0]),
        [50, 173, 7]
    );
    // Out of range climates clamp.
    assert_eq!(palette.sample(climate(2.0, 5.0), 0, 64, [0, 0]), [0, 0, 7]);
}

#[test]
fn the_grid_format_reads_the_biome_across_and_the_height_down() {
    let picture = rgba(4, 128, |x, y| [x as u8, y as u8, 0]);
    let grid = spec("format=grid\nyOffset=10", "x.properties");
    let palette = Palette::from_rgba(&grid, 4, 128, &picture).expect("palette");
    assert_eq!(palette.sample(climate(0.0, 0.0), 2, 74, [0, 0]), [2, 64, 0]);
    // Both axes clamp into the picture.
    assert_eq!(
        palette.sample(climate(0.0, 0.0), 200, 1000, [0, 0]),
        [3, 127, 0]
    );
    assert_eq!(palette.sample(climate(0.0, 0.0), 0, -50, [0, 0]), [0, 0, 0]);
}

#[test]
fn y_variance_jitters_the_row_by_position_within_its_bounds() {
    let picture = rgba(2, 256, |_, y| [y as u8, 0, 0]);
    let grid = spec("format=grid\nyVariance=3", "x.properties");
    let palette = Palette::from_rgba(&grid, 2, 256, &picture).expect("palette");
    let mut rows = std::collections::BTreeSet::new();
    for x in -20..20 {
        let [row, ..] = palette.sample(climate(0.0, 0.0), 0, 100, [x, 5]);
        assert!((97..=103).contains(&row), "{row}");
        rows.insert(row);
    }
    assert!(rows.len() > 1, "the jitter must vary with the position");
    assert_eq!(
        palette.sample(climate(0.0, 0.0), 0, 100, [7, 9]),
        palette.sample(climate(0.0, 0.0), 0, 100, [7, 9])
    );
}

#[test]
fn the_fixed_format_ignores_position_and_the_picture() {
    let spec = spec("format=fixed\ncolor=336699", "x.properties");
    let palette = Palette::from_rgba(&spec, 0, 0, &[]).expect("a fixed palette needs no picture");
    assert!(palette.is_constant());
    assert_eq!(
        palette.sample(climate(0.1, 0.2), 9, -4, [3, 3]),
        [0x33, 0x66, 0x99]
    );
    assert_eq!(Palette::fixed([1, 2, 3]).at_index(5), [1, 2, 3]);
}

#[test]
fn a_picture_that_does_not_fit_is_a_typed_error() {
    let spec = spec("", "x.properties");
    assert_eq!(
        Palette::from_rgba(&spec, 0, 0, &[]),
        Err(PaletteError::InvalidPicture {
            width: 0,
            height: 0
        })
    );
    assert!(Palette::from_rgba(&spec, 4, 4, &[0; 10]).is_err());
}

#[test]
fn narrow_palettes_index_linearly_and_clamp() {
    let picture = rgba(16, 1, |x, _| [x as u8 * 10, 0, 0]);
    let palette = Palette::from_rgba(&spec("", "x.properties"), 16, 1, &picture).expect("palette");
    assert_eq!(palette.at_index(3), [30, 0, 0]);
    assert_eq!(palette.at_index(99), [150, 0, 0]);
}

fn input() -> LightmapInput {
    LightmapInput {
        sun_brightness: 1.0,
        lightning: false,
        torch_flicker: 0.0,
        gamma: 0.0,
        night_vision: false,
    }
}

/// Sky rows are `(column, row, 0)` and block rows `(0, 0, column)`, so the
/// output tells which columns were read.
fn column_picture(width: u32) -> Lightmap {
    let picture = rgba(width, 32, |x, y| {
        if y < 16 {
            [x as u8, y as u8, 0]
        } else {
            [0, 0, x as u8]
        }
    });
    Lightmap::from_rgba(width, 32, &picture).expect("lightmap")
}

#[test]
fn lightning_reads_the_last_sky_column_and_calm_torches_the_middle_block_column() {
    let map = column_picture(17);
    let colors = map
        .colors(&LightmapInput {
            lightning: true,
            ..input()
        })
        .expect("colors");
    let [r, g, b] = colors[5 * 16 + 9];
    // Sky column 16, row 5; block column 8 (flicker 0 -> 0.5 * 16), row 9.
    assert!((r - 16.0 / 255.0).abs() < 1e-6, "{r}");
    assert!((g - 5.0 / 255.0).abs() < 1e-6, "{g}");
    assert!((b - 8.0 / 255.0).abs() < 1e-6, "{b}");
}

#[test]
fn the_sun_brightness_ramp_interpolates_between_columns() {
    let map = column_picture(17);
    // 0.2 is the darkest sun: column 0.
    let dark = LightmapInput {
        sun_brightness: 0.2,
        ..input()
    };
    assert!(map.colors(&dark).expect("colors")[0][0].abs() < 1e-6);
    // Midway between two columns: 1.1666666 * (sun - 0.2) * 16 = 8.5.
    let sun = 0.2 + 8.5 / 16.0 / 1.166_666_6;
    let mid = LightmapInput {
        sun_brightness: sun,
        ..input()
    };
    let red = map.colors(&mid).expect("colors")[0][0];
    assert!((red - 8.5 / 255.0).abs() < 1e-4, "{red}");
    // Lightning forces full sun.
    let flash = LightmapInput {
        sun_brightness: 0.2,
        lightning: true,
        ..input()
    };
    assert!((map.colors(&flash).expect("colors")[0][0] - 16.0 / 255.0).abs() < 1e-6);
}

#[test]
fn channels_add_and_saturate_and_gamma_lifts() {
    let bright =
        Lightmap::from_rgba(16, 32, &rgba(16, 32, |_, _| [200, 200, 200])).expect("lightmap");
    let colors = bright.colors(&input()).expect("colors");
    assert_eq!(colors[0], [1.0; 3], "200 + 200 saturates");

    let dim = Lightmap::from_rgba(16, 32, &rgba(16, 32, |_, _| [30, 30, 30])).expect("lightmap");
    let plain = dim.colors(&input()).expect("colors")[0][0];
    let lifted = dim
        .colors(&LightmapInput {
            gamma: 1.0,
            ..input()
        })
        .expect("colors")[0][0];
    assert!(lifted > plain);
}

#[test]
fn night_vision_needs_a_tall_picture_and_reads_the_lower_half() {
    let short = column_picture(16);
    let seeing = LightmapInput {
        night_vision: true,
        ..input()
    };
    assert!(short.colors(&seeing).is_none());

    let picture = rgba(
        16,
        64,
        |_, y| if y >= 32 { [250, 250, 250] } else { [1, 1, 1] },
    );
    let tall = Lightmap::from_rgba(16, 64, &picture).expect("lightmap");
    assert_eq!(tall.colors(&seeing).expect("colors")[0][0], 1.0);
    assert!(tall.colors(&input()).expect("colors")[0][0] < 0.1);
}

#[test]
fn a_lightmap_must_be_wide_and_tall_enough() {
    assert!(Lightmap::from_rgba(15, 32, &vec![0; 15 * 32 * 4]).is_err());
    assert!(Lightmap::from_rgba(16, 31, &vec![0; 16 * 31 * 4]).is_err());
    assert!(Lightmap::from_rgba(16, 32, &[0; 12]).is_err());
}

#[test]
fn weather_blends_the_variants_by_strength() {
    let flat = |value: u8| {
        Lightmap::from_rgba(16, 32, &rgba(16, 32, |_, _| [value, value, value])).expect("lightmap")
    };
    let pack = LightmapPack::new(flat(100), Some(flat(40)), None);
    let dry = pack.colors(&input(), Some((0.0, 0.0))).expect("colors")[0][0];
    // Two flat pictures: sky + block = twice the pixel.
    assert!((dry - 200.0 / 255.0).abs() < 1e-5, "{dry}");
    let wet = pack.colors(&input(), Some((1.0, 0.0))).expect("colors")[0][0];
    assert!((wet - 80.0 / 255.0).abs() < 1e-5, "{wet}");
    let half = pack.colors(&input(), Some((0.5, 0.0))).expect("colors")[0][0];
    assert!((half - (200.0 + 80.0) / 2.0 / 255.0).abs() < 1e-5, "{half}");
    // No weather in this dimension.
    assert_eq!(
        pack.colors(&input(), None),
        pack.colors(&input(), Some((0.0, 0.0)))
    );
}

#[test]
fn dimension_ids_come_from_the_file_name() {
    assert_eq!(dimension_of("world0.png"), Some(0));
    assert_eq!(dimension_of("world-1.png"), Some(-1));
    assert_eq!(dimension_of("world1_rain.png"), None);
    assert_eq!(dimension_of("nether.png"), None);
}

proptest::proptest! {
    #[test]
    fn color_properties_never_panic(bytes in proptest::collection::vec(proptest::num::u8::ANY, 0..512)) {
        let _ = ColorProperties::parse(&bytes);
    }

    #[test]
    fn palette_lookups_stay_inside_the_picture(
        t in -5.0f32..5.0, r in -5.0f32..5.0, id in 0u8..=255, y in -1000i32..1000, x in -1000i32..1000,
    ) {
        let picture = rgba(8, 8, |x, y| [x as u8, y as u8, 0]);
        for text in ["", "format=grid\nyVariance=4"] {
            let palette = Palette::from_rgba(&spec(text, "x.properties"), 8, 8, &picture).expect("palette");
            let [px, py, _] = palette.sample(climate(t, r), id, y, [x, x]);
            proptest::prop_assert!(px < 8 && py < 8);
        }
    }
}

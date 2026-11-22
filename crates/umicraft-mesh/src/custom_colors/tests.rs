// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the custom block colours. The palettes are built in code.

use umicraft_packext::colors::{Format, PaletteSpec};
use umicraft_packext::{Properties, RangeList};

use super::*;

/// A column grid: the biome of a column is `biome(x)`.
struct Columns(fn(i32) -> u8);

impl BlockSource for Columns {
    fn block(&self, _x: i32, _y: i32, _z: i32) -> BlockState {
        BlockState::new(DIRT, 0)
    }

    fn biome(&self, x: i32, _z: i32) -> Option<u8> {
        Some((self.0)(x))
    }
}

fn plains(_: i32) -> u8 {
    PLAINS
}

fn swamp(_: i32) -> u8 {
    SWAMP
}

fn picture(width: u32, height: u32, pixel: impl Fn(u32, u32) -> Rgb) -> Vec<u8> {
    (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .flat_map(|(x, y)| {
            let [r, g, b] = pixel(x, y);
            [r, g, b, 255]
        })
        .collect()
}

fn spec(text: &str) -> PaletteSpec {
    let props = Properties::parse(text.as_bytes()).expect("properties");
    PaletteSpec::parse(
        &props,
        "colormap/custom/x.properties",
        Format::Vanilla,
        &|_| None,
    )
    .expect("spec")
}

/// A `width` x `height` palette whose pixel is `(x, y, 9)`.
fn gradient(text: &str, width: u32, height: u32) -> Palette {
    let pixels = picture(width, height, |x, y| [x as u8, y as u8, 9]);
    Palette::from_rgba(&spec(text), width, height, &pixels).expect("palette")
}

fn solid(color: Rgb) -> Palette {
    Palette::from_rgba(&spec("format=fixed"), 0, 0, &[])
        .map_or_else(|_| Palette::fixed(color), |_| Palette::fixed(color))
}

fn tints() -> BiomeTints {
    BiomeTints::from_samplers(|_, _| [10, 20, 30], |_, _| [40, 50, 60])
}

fn tint(
    custom: &CustomColors,
    source: &Columns,
    state: BlockState,
    tinted: bool,
    mode: TintMode,
) -> Option<u32> {
    let table = tints();
    custom.tint(source, state, [0, 64, 0], tinted, (Some(&table), mode))
}

fn block(id: u16) -> Vec<BlockMatch> {
    vec![BlockMatch { id, metadata: None }]
}

#[test]
fn nothing_customised_means_the_vanilla_path() {
    let custom = CustomColors::default();
    assert!(custom.is_empty());
    for id in [2, 8, 18, 55, 111] {
        assert_eq!(
            tint(
                &custom,
                &Columns(plains),
                BlockState::new(id, 0),
                true,
                TintMode::VANILLA
            ),
            None
        );
    }
}

#[test]
fn a_block_palette_tints_every_face_of_its_block() {
    let mut custom = CustomColors::default();
    custom.add_block_palette(block(1), &solid([1, 2, 3]));
    assert!(custom.has_block(1) && !custom.has_block(2));
    let stone = BlockState::new(1, 0);
    // Even a face the model does not tint.
    assert_eq!(
        tint(&custom, &Columns(plains), stone, false, TintMode::VANILLA),
        Some(0x0001_0203)
    );
    assert_eq!(
        tint(
            &custom,
            &Columns(plains),
            BlockState::new(4, 0),
            false,
            TintMode::VANILLA
        ),
        None
    );
}

#[test]
fn a_block_palette_respects_the_metadata_list() {
    let mut custom = CustomColors::default();
    let only_two = vec![BlockMatch {
        id: 35,
        metadata: Some(RangeList::parse("2 4-5").expect("range list")),
    }];
    custom.add_block_palette(only_two, &solid([9, 9, 9]));
    let at = |meta| {
        tint(
            &custom,
            &Columns(plains),
            BlockState::new(35, meta),
            false,
            TintMode::VANILLA,
        )
    };
    assert!(at(2).is_some() && at(4).is_some() && at(5).is_some());
    assert!(at(0).is_none() && at(3).is_none());
}

#[test]
fn the_first_palette_added_for_a_block_wins() {
    let mut custom = CustomColors::default();
    custom.add_block_palette(block(1), &solid([1, 1, 1]));
    custom.add_block_palette(block(1), &solid([2, 2, 2]));
    assert_eq!(
        tint(
            &custom,
            &Columns(plains),
            BlockState::new(1, 0),
            false,
            TintMode::VANILLA
        ),
        Some(0x0001_0101)
    );
}

#[test]
fn the_side_of_a_grass_block_takes_the_dirt_palette() {
    let mut custom = CustomColors::default();
    custom.add_block_palette(block(DIRT), &solid([7, 7, 7]));
    let grass = BlockState::new(GRASS_BLOCK, 0);
    assert_eq!(
        tint(&custom, &Columns(plains), grass, false, TintMode::VANILLA),
        Some(0x0007_0707)
    );
    // The tinted top has no grass palette and no swamp one: vanilla.
    assert_eq!(
        tint(&custom, &Columns(plains), grass, true, TintMode::VANILLA),
        None
    );
}

#[test]
fn water_takes_the_water_palette_by_biome_climate() {
    let custom = CustomColors {
        water: Some(gradient("", 256, 256)),
        ..CustomColors::default()
    };
    let water = BlockState::new(9, 0);
    // Plains (0.8, 0.4): x = 50, y = 173 (see the palette tests).
    assert_eq!(
        tint(&custom, &Columns(plains), water, true, TintMode::VANILLA),
        Some(0x0032_AD09)
    );
}

#[test]
fn leaves_of_spruce_and_birch_take_their_own_palettes() {
    let custom = CustomColors {
        pine: Some(solid([1, 1, 1])),
        birch: Some(solid([2, 2, 2])),
        ..CustomColors::default()
    };
    let leaves = |meta| {
        tint(
            &custom,
            &Columns(plains),
            BlockState::new(LEAVES, meta),
            true,
            TintMode::VANILLA,
        )
    };
    assert_eq!(leaves(1), Some(0x0001_0101));
    assert_eq!(leaves(2 | 4), Some(0x0002_0202));
    // Oak leaves use no custom palette here.
    assert_eq!(leaves(0), None);
}

#[test]
fn swamp_palettes_apply_only_in_swamp_columns_and_blend_at_the_border() {
    let custom = CustomColors {
        swamp_grass: Some(solid([100, 100, 100])),
        ..CustomColors::default()
    };
    let grass = BlockState::new(GRASS_BLOCK, 0);
    // Not a swamp: the vanilla table colour.
    assert_eq!(
        tint(&custom, &Columns(plains), grass, true, TintMode::VANILLA),
        Some(0x000A_141E)
    );
    // A swamp that is not smoothed: the whole colour of the palette.
    let sharp = TintMode {
        smooth_biomes: false,
        ..TintMode::VANILLA
    };
    assert_eq!(
        tint(&custom, &Columns(swamp), grass, true, sharp),
        Some(0x0064_6464)
    );
    // Swamp colours off: the swamp counts as plains, and plains use the table.
    let no_swamp = TintMode {
        swamp_colors: false,
        ..TintMode::VANILLA
    };
    assert_eq!(
        tint(&custom, &Columns(swamp), grass, true, no_swamp),
        Some(0x000A_141E)
    );
    // Swamp on the left of the block and plains on the right: three columns of each.
    let border = Columns(|x| if x < 0 { SWAMP } else { PLAINS });
    let mixed = tint(&custom, &border, grass, true, TintMode::VANILLA).expect("blended");
    let red = (mixed >> 16) & 0xFF;
    assert_eq!(red, (100 * 3 + 10 * 6) / 9);
}

#[test]
fn a_grid_palette_reads_the_height_of_the_block() {
    let mut custom = CustomColors::default();
    custom.add_block_palette(block(1), &gradient("format=grid", 40, 128));
    let table = tints();
    let at = |y| {
        custom.tint(
            &Columns(plains),
            BlockState::new(1, 0),
            [0, y, 0],
            false,
            (Some(&table), TintMode::VANILLA),
        )
    };
    // Biome 1 across, y down: pixel (1, y, 9).
    assert_eq!(at(40), Some(0x0001_2809));
    assert_eq!(at(100), Some(0x0001_6409));
}

#[test]
fn the_lily_pad_redstone_and_stems_take_their_colours() {
    let ramp = |n: u32| {
        let pixels = picture(n, 1, |x, _| [x as u8 * 10, 0, 0]);
        Palette::from_rgba(
            &PaletteSpec::bare("colormap/r", Format::Grid),
            n,
            1,
            &pixels,
        )
        .expect("palette")
    };
    let custom = CustomColors {
        lilypad: Some([5, 6, 7]),
        redstone: Some(ramp(16)),
        stem: Some(ramp(8)),
        melon_stem: Some(solid([1, 2, 3])),
        ..CustomColors::default()
    };
    let at = |id, meta| {
        tint(
            &custom,
            &Columns(plains),
            BlockState::new(id, meta),
            true,
            TintMode::VANILLA,
        )
    };
    assert_eq!(at(LILY_PAD, 0), Some(0x0005_0607));
    assert_eq!(at(REDSTONE_WIRE, 15), Some(150 << 16));
    assert_eq!(
        at(PUMPKIN_STEM, 3),
        Some(30 << 16),
        "the shared stem palette"
    );
    assert_eq!(at(MELON_STEM, 3), Some(0x0001_0203), "its own palette wins");
    // The model must tint the face for these.
    let untinted = tint(
        &custom,
        &Columns(plains),
        BlockState::new(LILY_PAD, 0),
        false,
        TintMode::VANILLA,
    );
    assert_eq!(untinted, None);
}

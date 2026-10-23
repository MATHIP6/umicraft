// SPDX-License-Identifier: MIT OR Apache-2.0

//! 1.8 biome tints for the grayscale grass, foliage and water textures.
//!
//! In 1.8 the grass block, the leaves, the vines and the tall grass use
//! grayscale textures; the client multiplies them by a colour sampled from the
//! biome (`ColorizerGrass`, `ColorizerFoliage`, `BiomeColorHelper`). The model
//! format marks the affected faces with `tintindex`, and the world stores one
//! biome byte per column. This module turns those three inputs into the
//! `0x00RRGGBB` value carried by [`Vertex::tint`](crate::Vertex::tint).
//!
//! The colormaps themselves are Mojang assets. They are decoded by
//! `umicraft-assets` from the verified user jar at runtime and handed to
//! [`BiomeTints::from_samplers`] as two sampling closures; this crate never
//! embeds a colormap. Tests build the table from constant closures.
//!
//! Sources (ADR-0001): `ColorizerGrass.func_77480_a`,
//! `ColorizerFoliage.func_77470_a`, `BiomeColorHelper.func_180285_a`,
//! `BiomeGenBase.func_180627_b`/`func_180625_c` and the `BiomeGen*`
//! overrides. `docs/sources.md` records the provenance and the two deliberate
//! simplifications.

use umicraft_world::BlockState;

use crate::BlockSource;

/// Untinted colour packed as `0x00RRGGBB`.
pub const WHITE: u32 = 0x00FF_FFFF;

/// Number of base 1.8 biomes (`0..=39`).
const BASE_BIOMES: usize = 40;

/// Biome ids `128..=167` are mutations of the base biome `id & 127`.
const MUTATED_BIOMES: u8 = 128;

/// The block class a tinted face samples from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TintClass {
    /// The texture keeps its own colours (stone, dirt, flowers, ...).
    Untinted,
    /// `ColorizerGrass`: grass blocks, tall grass, double grass/fern.
    Grass,
    /// `ColorizerFoliage`: leaves and vines.
    Foliage,
    /// Biome water colour: white except for the swamp.
    Water,
}

/// Biome temperature and rainfall of the 1.8 base biomes, indexed by id.
///
/// Values come from `BiomeGenBase`'s registration and the `BiomeGen*`
/// constructors (`func_76732_a`). The default is `0.5, 0.5`. Order: ocean,
/// plains, desert, extreme hills, forest, taiga, swampland, river, hell, end,
/// frozen ocean, frozen river, ice plains, ice mountains, mushroom island,
/// mushroom shore, beach, desert hills, forest hills, taiga hills, extreme
/// hills edge, jungle, jungle hills, jungle edge, deep ocean, stone beach,
/// cold beach, birch forest, birch forest hills, roofed forest, cold taiga,
/// cold taiga hills, mega taiga, mega taiga hills, extreme hills+, savanna,
/// savanna plateau, mesa, mesa plateau F, mesa plateau.
const CLIMATE: [(f32, f32); BASE_BIOMES] = [
    (0.5, 0.5),
    (0.8, 0.4),
    (2.0, 0.0),
    (0.2, 0.3),
    (0.7, 0.8),
    (0.25, 0.8),
    (0.8, 0.9),
    (0.5, 0.5),
    (2.0, 0.0),
    (0.5, 0.5),
    (0.0, 0.5),
    (0.0, 0.5),
    (0.0, 0.5),
    (0.0, 0.5),
    (0.9, 1.0),
    (0.9, 1.0),
    (0.8, 0.4),
    (2.0, 0.0),
    (0.7, 0.8),
    (0.25, 0.8),
    (0.2, 0.3),
    (0.95, 0.9),
    (0.95, 0.9),
    (0.95, 0.8),
    (0.5, 0.5),
    (0.2, 0.3),
    (0.05, 0.3),
    (0.6, 0.6),
    (0.6, 0.6),
    (0.7, 0.8),
    (-0.5, 0.4),
    (-0.5, 0.4),
    (0.3, 0.8),
    (0.3, 0.8),
    (0.2, 0.3),
    (1.2, 0.0),
    (1.0, 0.0),
    (2.0, 0.0),
    (2.0, 0.0),
    (2.0, 0.0),
];

/// Swamp grass/foliage colour (`BiomeGenSwamp.func_180627_b`/`_c`).
///
/// 1.8 picks between this and `0x4C763C` with a per-position noise; the
/// world model has no such noise, so the flat branch is used (ADR-0009).
const SWAMP_GRASS: [u8; 3] = [0x6A, 0x70, 0x39];

/// Swamp water multiplier (`BiomeGenSwamp.field_76759_H`).
const SWAMP_WATER: [u8; 3] = [0xE0, 0xFF, 0xAE];

/// Mesa grass and foliage (`BiomeGenMesa`).
const MESA_GRASS: [u8; 3] = [0x90, 0x81, 0x4D];
const MESA_FOLIAGE: [u8; 3] = [0x9E, 0x81, 0x4D];

/// Base biome ids of the mesa variants.
const MESA_BIOMES: std::ops::RangeInclusive<usize> = 37..=39;

/// Roofed forest grass darkening: `((c & 0x00FEFEFE) + 0x0028340A) >> 1`.
const ROOFED_MASK: u32 = 0x00FE_FEFE;
const ROOFED_OFFSET: u32 = 0x0028_340A;

/// Per-biome tint colours for the three colormaps.
///
/// The table is immutable and small enough to live in an `Arc` shared by every
/// meshing worker. It is built once from the two runtime colormaps, so the
/// mesher never touches a PNG or the disk.
#[derive(Debug, Clone)]
pub struct BiomeTints {
    grass: [[u8; 3]; BASE_BIOMES],
    foliage: [[u8; 3]; BASE_BIOMES],
    water: [[u8; 3]; BASE_BIOMES],
}

impl BiomeTints {
    /// Builds the table from the grass and foliage colormap samplers.
    ///
    /// Each closure receives `(temperature, rainfall)` in `0..=1` and returns
    /// the sampled `[r, g, b]` (see `umicraft_assets::Colormap::sample`).
    /// The samplers are called 40 times each; the resulting table is the only
    /// retained data, so the closures can borrow the decoded colormaps.
    ///
    /// ```
    /// use umicraft_mesh::tint::BiomeTints;
    /// let tints = BiomeTints::from_samplers(|_, _| [0x79, 0xC0, 0x5A], |_, _| [0x59, 0xAE, 0x30]);
    /// assert_eq!(tints.color(umicraft_mesh::tint::TintClass::Grass, 1), [0x79, 0xC0, 0x5A]);
    /// ```
    #[must_use]
    pub fn from_samplers<G, F>(grass: G, foliage: F) -> Self
    where
        G: Fn(f32, f32) -> [u8; 3],
        F: Fn(f32, f32) -> [u8; 3],
    {
        let mut tints = Self {
            grass: [[0xFF; 3]; BASE_BIOMES],
            foliage: [[0xFF; 3]; BASE_BIOMES],
            water: [[0xFF; 3]; BASE_BIOMES],
        };
        for (id, &(temperature, rainfall)) in CLIMATE.iter().enumerate() {
            tints.grass[id] = grass(temperature, rainfall);
            tints.foliage[id] = foliage(temperature, rainfall);
        }

        // Vanilla overrides, applied after the colormap sampling.
        tints.grass[6] = SWAMP_GRASS;
        tints.foliage[6] = SWAMP_GRASS;
        tints.water[6] = SWAMP_WATER;
        tints.grass[29] = unpack(roofed(pack(tints.grass[29])));
        for id in MESA_BIOMES {
            tints.grass[id] = MESA_GRASS;
            tints.foliage[id] = MESA_FOLIAGE;
        }
        tints
    }

    /// Colour of one class at one biome id.
    ///
    /// Mutated biome ids (`128..=167`) inherit their base colour, unknown ids
    /// fall back to plains (1), and [`TintClass::Untinted`] is always white.
    #[must_use]
    pub fn color(&self, class: TintClass, biome: u8) -> [u8; 3] {
        let base = if biome < MUTATED_BIOMES {
            usize::from(biome)
        } else {
            usize::from(biome & 127)
        };
        let base = if base < BASE_BIOMES { base } else { 1 };
        match class {
            TintClass::Untinted => [0xFF; 3],
            TintClass::Grass => self.grass[base],
            TintClass::Foliage => self.foliage[base],
            TintClass::Water => self.water[base],
        }
    }
}

/// Tint class of a block state, following the 1.8 block colour methods.
///
/// The model decides *which faces* are tinted (`tintindex`); this function
/// decides which colormap a tinted face samples. Blocks outside the list keep
/// their texture colours. `BlockTallGrass` metadata 0 is the dead bush and
/// `BlockDoublePlant` tints only the grass/fern variants.
#[must_use]
pub(crate) fn tint_class(state: BlockState) -> TintClass {
    match state.id() {
        2 => TintClass::Grass,
        31 if state.metadata() != 0 => TintClass::Grass,
        175 if matches!(state.metadata() & 3, 2 | 3) => TintClass::Grass,
        18 | 106 => TintClass::Foliage,
        8 | 9 => TintClass::Water,
        _ => TintClass::Untinted,
    }
}

/// Samples the 3×3 biome-colour average at a block column.
///
/// Vanilla `BiomeColorHelper` averages the colours of the nine columns around
/// the rendered block, which is what makes a biome border blend instead of
/// showing a hard step. Columns without loaded biome data are skipped; when
/// none is available the face stays white. The result is packed as
/// `0x00RRGGBB` for [`Vertex::tint`](crate::Vertex::tint).
pub(crate) fn biome_tint<S: BlockSource + ?Sized>(
    source: &S,
    class: TintClass,
    x: i32,
    z: i32,
    tints: &BiomeTints,
) -> u32 {
    if class == TintClass::Untinted {
        return WHITE;
    }
    let mut channels = [0u32; 3];
    let mut samples = 0u32;
    for dz in -1..=1 {
        for dx in -1..=1 {
            let Some(biome) = source.biome(x + dx, z + dz) else {
                continue;
            };
            let [red, green, blue] = tints.color(class, biome);
            channels[0] += u32::from(red);
            channels[1] += u32::from(green);
            channels[2] += u32::from(blue);
            samples += 1;
        }
    }
    if samples == 0 {
        return WHITE;
    }
    pack([
        (channels[0] / samples) as u8,
        (channels[1] / samples) as u8,
        (channels[2] / samples) as u8,
    ])
}

/// Packs an RGB triple into the vertex tint layout.
const fn pack(rgb: [u8; 3]) -> u32 {
    (rgb[0] as u32) << 16 | (rgb[1] as u32) << 8 | rgb[2] as u32
}

/// Unpacks a vertex tint into its RGB triple.
const fn unpack(tint: u32) -> [u8; 3] {
    [(tint >> 16) as u8, (tint >> 8) as u8, tint as u8]
}

/// The 1.8 roofed-forest grass blend on packed RGB.
const fn roofed(grass: u32) -> u32 {
    ((grass & ROOFED_MASK) + ROOFED_OFFSET) >> 1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a table whose grass/foliage colours encode the climate, so the
    /// tests can assert which temperature and rainfall were sampled.
    fn climate_tints() -> BiomeTints {
        BiomeTints::from_samplers(
            |temperature, rainfall| [(temperature * 100.0) as u8, (rainfall * 100.0) as u8, 0],
            |temperature, rainfall| [(temperature * 100.0) as u8, (rainfall * 100.0) as u8, 1],
        )
    }

    #[test]
    fn the_climate_table_matches_the_vanilla_biomes() {
        let tints = climate_tints();
        assert_eq!(tints.color(TintClass::Grass, 0), [50, 50, 0], "ocean");
        assert_eq!(tints.color(TintClass::Grass, 1), [80, 40, 0], "plains");
        assert_eq!(tints.color(TintClass::Grass, 2), [200, 0, 0], "desert");
        assert_eq!(tints.color(TintClass::Grass, 5), [25, 80, 0], "taiga");
        assert_eq!(
            tints.color(TintClass::Grass, 18),
            [70, 80, 0],
            "forest hills"
        );
        assert_eq!(tints.color(TintClass::Grass, 27), [60, 60, 0], "birch");
        assert_eq!(tints.color(TintClass::Grass, 30), [0, 40, 0], "-0.5 clamps");
    }

    #[test]
    fn mutated_biomes_inherit_the_base_colour() {
        let tints = climate_tints();
        assert_eq!(
            tints.color(TintClass::Grass, 128 + 1),
            tints.color(TintClass::Grass, 1)
        );
        assert_eq!(
            tints.color(TintClass::Foliage, 128 + 21),
            tints.color(TintClass::Foliage, 21)
        );
    }

    #[test]
    fn unknown_biomes_fall_back_to_plains() {
        let tints = climate_tints();
        assert_eq!(
            tints.color(TintClass::Grass, 200),
            tints.color(TintClass::Grass, 1)
        );
        assert_eq!(
            tints.color(TintClass::Grass, 255),
            tints.color(TintClass::Grass, 1)
        );
    }

    #[test]
    fn swamp_and_mesa_use_the_vanilla_overrides() {
        let tints = climate_tints();
        assert_eq!(tints.color(TintClass::Grass, 6), SWAMP_GRASS);
        assert_eq!(tints.color(TintClass::Foliage, 6), SWAMP_GRASS);
        assert_eq!(tints.color(TintClass::Water, 6), SWAMP_WATER);
        assert_eq!(tints.color(TintClass::Water, 1), [0xFF; 3]);
        for id in 37..=39 {
            assert_eq!(tints.color(TintClass::Grass, id), MESA_GRASS);
            assert_eq!(tints.color(TintClass::Foliage, id), MESA_FOLIAGE);
        }
    }

    #[test]
    fn roofed_forest_grass_is_darkened_like_vanilla() {
        let tints = climate_tints();
        // Base sample of roofed forest (0.7, 0.8) is [70, 80, 0].
        assert_eq!(
            tints.color(TintClass::Grass, 29),
            unpack(roofed(pack([70, 80, 0])))
        );
        assert_eq!(tints.color(TintClass::Grass, 29), [55, 66, 5]);
        // The foliage of the roofed forest is not blended.
        assert_eq!(tints.color(TintClass::Foliage, 29), [70, 80, 1]);
    }

    #[test]
    fn untinted_faces_are_white() {
        let tints = climate_tints();
        assert_eq!(tints.color(TintClass::Untinted, 1), [0xFF; 3]);
        assert_eq!(tints.color(TintClass::Untinted, 6), [0xFF; 3]);
    }

    #[test]
    fn tint_classes_follow_the_vanilla_blocks() {
        assert_eq!(tint_class(BlockState::new(2, 0)), TintClass::Grass);
        assert_eq!(tint_class(BlockState::new(31, 0)), TintClass::Untinted);
        assert_eq!(tint_class(BlockState::new(31, 1)), TintClass::Grass);
        assert_eq!(tint_class(BlockState::new(31, 2)), TintClass::Grass);
        assert_eq!(tint_class(BlockState::new(175, 0)), TintClass::Untinted);
        assert_eq!(tint_class(BlockState::new(175, 1)), TintClass::Untinted);
        assert_eq!(tint_class(BlockState::new(175, 2)), TintClass::Grass);
        assert_eq!(tint_class(BlockState::new(175, 3)), TintClass::Grass);
        assert_eq!(tint_class(BlockState::new(18, 0)), TintClass::Foliage);
        assert_eq!(tint_class(BlockState::new(18, 2)), TintClass::Foliage);
        assert_eq!(tint_class(BlockState::new(106, 0)), TintClass::Foliage);
        assert_eq!(tint_class(BlockState::new(8, 0)), TintClass::Water);
        assert_eq!(tint_class(BlockState::new(9, 0)), TintClass::Water);
        assert_eq!(tint_class(BlockState::new(1, 0)), TintClass::Untinted);
    }

    struct BiomeGrid {
        biome: u8,
    }

    impl BlockSource for BiomeGrid {
        fn block(&self, _x: i32, _y: i32, _z: i32) -> BlockState {
            BlockState::AIR
        }

        fn biome(&self, _x: i32, _z: i32) -> Option<u8> {
            Some(self.biome)
        }
    }

    #[test]
    fn the_three_by_three_average_blends_neighbouring_biomes() {
        let tints = BiomeTints::from_samplers(|_, _| [0x10, 0x20, 0x30], |_, _| [0; 3]);
        let source = BiomeGrid { biome: 1 };
        assert_eq!(
            biome_tint(&source, TintClass::Grass, 0, 0, &tints),
            pack([0x10, 0x20, 0x30])
        );

        // A source whose centre is plains and whose neighbours are desert:
        // the average is taken over the nine colours, not just the centre.
        struct Border {
            tints: BiomeTints,
        }
        impl BlockSource for Border {
            fn block(&self, _x: i32, _y: i32, _z: i32) -> BlockState {
                BlockState::AIR
            }

            fn biome(&self, x: i32, _z: i32) -> Option<u8> {
                Some(if x < 0 { 2 } else { 1 })
            }
        }
        let border = Border {
            tints: BiomeTints::from_samplers(
                |temperature, _| {
                    if temperature > 1.0 {
                        [0, 0, 0]
                    } else {
                        [90, 90, 90]
                    }
                },
                |_, _| [0; 3],
            ),
        };
        // At x=0, three of the nine columns (x = -1) are desert.
        assert_eq!(
            biome_tint(&border, TintClass::Grass, 0, 0, &border.tints),
            pack([60, 60, 60])
        );
    }

    #[test]
    fn a_source_without_biome_data_stays_white() {
        struct NoBiomes;
        impl BlockSource for NoBiomes {
            fn block(&self, _x: i32, _y: i32, _z: i32) -> BlockState {
                BlockState::AIR
            }
        }
        let tints = climate_tints();
        assert_eq!(biome_tint(&NoBiomes, TintClass::Grass, 0, 0, &tints), WHITE);
    }
}

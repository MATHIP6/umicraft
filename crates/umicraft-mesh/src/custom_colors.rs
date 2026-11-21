// SPDX-License-Identifier: MIT OR Apache-2.0

//! Custom block colours (OptiFine `CustomColors`, ADR-0088/0089).
//!
//! The packs' colormap pictures replace the vanilla tint of some blocks:
//!
//! - `palette.block.*` and `colormap/custom|blocks/*.properties` link a
//!   palette to any block (and metadata), tinted or not;
//! - `water.png`, `pine.png`, `birch.png`, `swampgrass.png` and
//!   `swampfoliage.png` replace the water, spruce leaf, birch leaf and swamp
//!   colours;
//! - `redstone.png`, `stem.png`, `pumpkinstem.png`, `melonstem.png` and the
//!   `lilypad` key replace the fixed colours of those blocks.
//!
//! [`CustomColors::tint`] answers for one block and returns `None` when the
//! pack customises nothing for it, so the vanilla tint path runs unchanged.
//! Palettes of a non-constant format are averaged over the 3×3 columns around
//! the block when `Smooth Biomes` is on (`getSmoothColorMultiplier`).

use umicraft_packext::colors::{Climate, Palette, Rgb};
use umicraft_packext::ctm::BlockMatch;
use umicraft_world::BlockState;

use crate::tint::{biome_climate, pack, tint_class, BiomeTints, TintClass, TintMode};
use crate::BlockSource;

/// Base biome id of the swamp.
const SWAMP: u8 = 6;
/// Plains, which stands in for the swamp when `Swamp Colors` is off.
const PLAINS: u8 = 1;
const LEAVES: u16 = 18;
const LILY_PAD: u16 = 111;
const REDSTONE_WIRE: u16 = 55;
const PUMPKIN_STEM: u16 = 104;
const MELON_STEM: u16 = 105;
const DOUBLE_PLANT: u16 = 175;
const GRASS_BLOCK: u16 = 2;
const DIRT: u16 = 3;

/// A palette with the blocks it applies to.
#[derive(Debug, Clone)]
struct BlockPalette {
    matches: Vec<BlockMatch>,
    palette: Palette,
}

impl BlockPalette {
    fn matches(&self, state: BlockState) -> bool {
        self.matches.iter().any(|entry| {
            entry.id == state.id()
                && entry
                    .metadata
                    .as_ref()
                    .is_none_or(|list| list.contains(i64::from(state.metadata())))
        })
    }
}

/// The custom colours of one resource load. Immutable and shared by every
/// meshing worker.
#[derive(Debug, Clone, Default)]
pub struct CustomColors {
    /// Palettes by block id; the first that matches wins, in file order.
    blocks: Vec<Vec<BlockPalette>>,
    /// `water.png`.
    pub water: Option<Palette>,
    /// `pine.png`: spruce leaves.
    pub pine: Option<Palette>,
    /// `birch.png`: birch leaves.
    pub birch: Option<Palette>,
    /// `swampgrass.png`.
    pub swamp_grass: Option<Palette>,
    /// `swampfoliage.png`.
    pub swamp_foliage: Option<Palette>,
    /// `redstone.png`: 16 colours by power level.
    pub redstone: Option<Palette>,
    /// `stem.png`: 8 colours by growth age.
    pub stem: Option<Palette>,
    /// `pumpkinstem.png`.
    pub pumpkin_stem: Option<Palette>,
    /// `melonstem.png`.
    pub melon_stem: Option<Palette>,
    /// `lilypad` of `color.properties`.
    pub lilypad: Option<Rgb>,
}

impl CustomColors {
    /// Links `palette` to every block of `matches`. Palettes added earlier
    /// win over later ones for the same block.
    pub fn add_block_palette(&mut self, matches: Vec<BlockMatch>, palette: &Palette) {
        let ids: Vec<u16> = matches.iter().map(|entry| entry.id).collect();
        for id in ids {
            let slot = usize::from(id);
            if self.blocks.len() <= slot {
                self.blocks.resize(slot + 1, Vec::new());
            }
            let own: Vec<BlockMatch> = matches
                .iter()
                .filter(|entry| entry.id == id)
                .cloned()
                .collect();
            self.blocks[slot].push(BlockPalette {
                matches: own,
                palette: palette.clone(),
            });
        }
    }

    /// Whether nothing is customised.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.blocks.iter().all(Vec::is_empty)
            && [
                &self.water,
                &self.pine,
                &self.birch,
                &self.swamp_grass,
                &self.swamp_foliage,
                &self.redstone,
                &self.stem,
                &self.pumpkin_stem,
                &self.melon_stem,
            ]
            .iter()
            .all(|palette| palette.is_none())
            && self.lilypad.is_none()
    }

    /// Whether a palette is linked to block `id`, so that every face of the
    /// block takes a tint, tinted by the model or not.
    #[must_use]
    pub fn has_block(&self, id: u16) -> bool {
        self.blocks
            .get(usize::from(id))
            .is_some_and(|list| !list.is_empty())
    }

    /// Whether a face of block `id` needs a tint of its own because of a
    /// custom palette: the block has one, or it is a grass block whose
    /// untinted side takes the dirt palette.
    #[must_use]
    pub(crate) fn tints_face(&self, id: u16, tinted: bool) -> bool {
        self.has_block(id) || (id == GRASS_BLOCK && !tinted && self.has_block(DIRT))
    }

    fn block_palette(&self, state: BlockState) -> Option<&Palette> {
        self.blocks
            .get(usize::from(state.id()))?
            .iter()
            .find(|entry| entry.matches(state))
            .map(|entry| &entry.palette)
    }

    /// The tint of a face of `state` at `position`, or `None` when the packs
    /// customise nothing for it. `tinted` says whether the model marks the
    /// face for tinting: an untinted face only takes a block palette, and the
    /// side of a grass block takes the dirt one (`getColorMultiplier`).
    pub(crate) fn tint<S: BlockSource + ?Sized>(
        &self,
        source: &S,
        state: BlockState,
        position: [i32; 3],
        tinted: bool,
        (tints, mode): (Option<&BiomeTints>, TintMode),
    ) -> Option<u32> {
        let (state, position) = lower_half(source, state, position);
        let palette_state = if !tinted && state.id() == GRASS_BLOCK {
            BlockState::new(DIRT, 0)
        } else {
            state
        };
        if let Some(palette) = self.block_palette(palette_state) {
            return sample(source, position, mode, Some(palette), None, &|_| None);
        }
        if !tinted {
            return None;
        }
        if let Some(color) = self.fixed_palette_color(state) {
            return Some(pack(color));
        }
        let (primary, swamp, class) = self.biome_palettes(state)?;
        let fallback =
            |biome: u8| tints.map(|tints| tints.color_with(class, biome, mode.swamp_colors));
        sample(source, position, mode, primary, swamp, &fallback)
    }

    /// The colour of the blocks that depend on their metadata, not on the
    /// biome: lily pad, redstone wire and stems.
    fn fixed_palette_color(&self, state: BlockState) -> Option<Rgb> {
        let meta = usize::from(state.metadata());
        match state.id() {
            LILY_PAD => self.lilypad,
            REDSTONE_WIRE => self.redstone.as_ref().map(|p| p.at_index(meta.min(15))),
            PUMPKIN_STEM => self
                .stem_palette(&self.pumpkin_stem)
                .map(|p| p.at_index(meta & 7)),
            MELON_STEM => self
                .stem_palette(&self.melon_stem)
                .map(|p| p.at_index(meta & 7)),
            _ => None,
        }
    }

    fn stem_palette<'a>(&'a self, own: &'a Option<Palette>) -> Option<&'a Palette> {
        own.as_ref().or(self.stem.as_ref())
    }

    /// The palettes that colour the biome-tinted blocks: the palette of the
    /// block itself, the swamp one, and the vanilla class for the fallback.
    fn biome_palettes(
        &self,
        state: BlockState,
    ) -> Option<(Option<&Palette>, Option<&Palette>, TintClass)> {
        let class = tint_class(state);
        let leaf = state.id() == LEAVES;
        let (primary, swamp) = match (class, state.metadata() & 3) {
            (TintClass::Water, _) => (self.water.as_ref(), None),
            (TintClass::Grass, _) => (None, self.swamp_grass.as_ref()),
            _ if leaf && state.metadata() & 3 == 1 => (self.pine.as_ref(), None),
            _ if leaf && state.metadata() & 3 == 2 => (self.birch.as_ref(), None),
            (TintClass::Foliage, _) => (None, self.swamp_foliage.as_ref()),
            (TintClass::Untinted, _) => return None,
        };
        if primary.is_none() && swamp.is_none() {
            return None;
        }
        Some((primary, swamp, class))
    }
}

/// The upper half of a double plant takes the colour of the lower half
/// (`getColorMultiplier`).
fn lower_half<S: BlockSource + ?Sized>(
    source: &S,
    state: BlockState,
    [x, y, z]: [i32; 3],
) -> (BlockState, [i32; 3]) {
    if state.id() == DOUBLE_PLANT && state.metadata() >= 8 {
        (source.block(x, y - 1, z), [x, y - 1, z])
    } else {
        (state, [x, y, z])
    }
}

/// The average colour over the 3×3 columns (or the single one) around
/// `position`. Each column takes `primary`, or `swamp` in a swamp biome, or
/// the vanilla `fallback`. `None` when no column has a biome or no source of
/// colour.
fn sample<S: BlockSource + ?Sized>(
    source: &S,
    [x, y, z]: [i32; 3],
    mode: TintMode,
    primary: Option<&Palette>,
    swamp: Option<&Palette>,
    fallback: &dyn Fn(u8) -> Option<Rgb>,
) -> Option<u32> {
    if let Some(constant) = primary.filter(|palette| palette.is_constant()) {
        let climate = Climate {
            temperature: 0.5,
            rainfall: 0.5,
        };
        return Some(pack(constant.sample(climate, PLAINS, y, [x, z])));
    }
    let radius = i32::from(mode.smooth_biomes);
    let mut channels = [0u32; 3];
    let mut samples = 0u32;
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            let Some(mut biome) = source.biome(x + dx, z + dz) else {
                continue;
            };
            if biome & 127 == SWAMP && !mode.swamp_colors {
                biome = PLAINS;
            }
            let palette = primary.or_else(|| swamp.filter(|_| biome & 127 == SWAMP));
            let color = match palette {
                Some(palette) => {
                    let (temperature, rainfall) = biome_climate(biome);
                    let climate = Climate {
                        temperature,
                        rainfall,
                    };
                    Some(palette.sample(climate, biome, y, [x + dx, z + dz]))
                }
                None => fallback(biome),
            };
            let Some([red, green, blue]) = color else {
                continue;
            };
            channels[0] += u32::from(red);
            channels[1] += u32::from(green);
            channels[2] += u32::from(blue);
            samples += 1;
        }
    }
    (samples > 0).then(|| {
        pack([
            (channels[0] / samples) as u8,
            (channels[1] / samples) as u8,
            (channels[2] / samples) as u8,
        ])
    })
}

#[cfg(test)]
mod tests;

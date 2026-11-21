// SPDX-License-Identifier: MIT OR Apache-2.0

//! The video options the mesh workers read (ADR-0048).
//!
//! The workers run on their own threads and hold only an immutable mesher, so
//! an option that changes what they emit is one process-wide value, like the
//! GUI scale: the window stores it, then asks the game thread to re-mesh every
//! loaded column (`GameEvent::ReMeshAll`).

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use umicraft_mesh::{BetterGrass, ConnectedTextures, Leaves, Lighting, MeshSettings};

use crate::options::video_sections::{LeavesMode, OffFastFancy};
use crate::options::{VideoOptions, MAX_SMOOTH_LIGHTING};

/// The `Smooth Lighting` option value the workers mesh with.
static SMOOTH_LIGHTING: AtomicU8 = AtomicU8::new(MAX_SMOOTH_LIGHTING);
/// The `Other Block Variants` option the workers mesh with.
static BLOCK_ALTERNATIVES: AtomicBool = AtomicBool::new(true);

/// The leaf rendering the workers mesh with, as [`leaves_to_u8`] encodes it.
static LEAVES: AtomicU8 = AtomicU8::new(LEAVES_SMART);
/// The `Swamp Colors` option the workers mesh with.
static SWAMP_COLORS: AtomicBool = AtomicBool::new(true);
/// The `Smooth Biomes` option the workers mesh with.
static SMOOTH_BIOMES: AtomicBool = AtomicBool::new(true);
/// The `Better Grass` option, as [`choice_to_u8`] encodes it.
static BETTER_GRASS: AtomicU8 = AtomicU8::new(0);
/// The `Better Snow` option the workers mesh with.
static BETTER_SNOW: AtomicBool = AtomicBool::new(false);

/// The `Connected Textures` option, as [`choice_to_u8`] encodes it.
static CONNECTED_TEXTURES: AtomicU8 = AtomicU8::new(1);
/// The `Natural Textures` option the workers mesh with.
static NATURAL_TEXTURES: AtomicBool = AtomicBool::new(false);
/// The `Custom Colors` option. Resources read it when they load, so a change
/// takes effect with the resource reload (`Effect::ReloadResources`).
static CUSTOM_COLORS: AtomicBool = AtomicBool::new(true);

const LEAVES_FAST: u8 = 0;
const LEAVES_SMART: u8 = 1;
const LEAVES_FANCY: u8 = 2;

const fn leaves_to_u8(mode: LeavesMode) -> u8 {
    match mode {
        LeavesMode::Fast => LEAVES_FAST,
        LeavesMode::Smart => LEAVES_SMART,
        LeavesMode::Fancy => LEAVES_FANCY,
    }
}

const fn leaves_from_u8(value: u8) -> Leaves {
    match value {
        LEAVES_FAST => Leaves::Fast,
        LEAVES_FANCY => Leaves::Fancy,
        _ => Leaves::Smart,
    }
}

const fn choice_to_u8(choice: OffFastFancy) -> u8 {
    match choice {
        OffFastFancy::Off => 0,
        OffFastFancy::Fast => 1,
        OffFastFancy::Fancy => 2,
    }
}

const fn grass_from_u8(value: u8) -> BetterGrass {
    match value {
        1 => BetterGrass::Fast,
        2 => BetterGrass::Fancy,
        _ => BetterGrass::Off,
    }
}

const fn connected_from_u8(value: u8) -> ConnectedTextures {
    match value {
        1 => ConnectedTextures::Fast,
        2 => ConnectedTextures::Fancy,
        _ => ConnectedTextures::Off,
    }
}

/// Stores the mesh-affecting options of `video`. Whether to re-mesh is
/// decided by the option table (`video_spec`, `Effect::ReMesh`).
pub fn store(video: &VideoOptions) {
    SMOOTH_LIGHTING.store(video.smooth_lighting, Ordering::Relaxed);
    BLOCK_ALTERNATIVES.store(video.block_alternatives, Ordering::Relaxed);
    let leaves = video.details.trees.resolve(video.fancy_graphics);
    LEAVES.store(leaves_to_u8(leaves), Ordering::Relaxed);
    SWAMP_COLORS.store(video.details.swamp_colors, Ordering::Relaxed);
    SMOOTH_BIOMES.store(video.details.smooth_biomes, Ordering::Relaxed);
    BETTER_GRASS.store(choice_to_u8(video.quality.better_grass), Ordering::Relaxed);
    BETTER_SNOW.store(video.quality.better_snow, Ordering::Relaxed);
    CONNECTED_TEXTURES.store(
        choice_to_u8(video.quality.connected_textures),
        Ordering::Relaxed,
    );
    NATURAL_TEXTURES.store(video.quality.natural_textures, Ordering::Relaxed);
    CUSTOM_COLORS.store(video.quality.custom_colors, Ordering::Relaxed);
}

/// Whether the packs' custom colours are on.
#[must_use]
pub fn custom_colors_enabled() -> bool {
    CUSTOM_COLORS.load(Ordering::Relaxed)
}

/// The settings the mesh workers use right now.
#[must_use]
pub fn current() -> MeshSettings {
    MeshSettings {
        lighting: Lighting::from_option(SMOOTH_LIGHTING.load(Ordering::Relaxed)),
        block_alternatives: BLOCK_ALTERNATIVES.load(Ordering::Relaxed),
        leaves: leaves_from_u8(LEAVES.load(Ordering::Relaxed)),
        swamp_colors: SWAMP_COLORS.load(Ordering::Relaxed),
        smooth_biomes: SMOOTH_BIOMES.load(Ordering::Relaxed),
        better_grass: grass_from_u8(BETTER_GRASS.load(Ordering::Relaxed)),
        better_snow: BETTER_SNOW.load(Ordering::Relaxed),
        connected_textures: connected_from_u8(CONNECTED_TEXTURES.load(Ordering::Relaxed)),
        natural_textures: NATURAL_TEXTURES.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_reference_defaults_except_trees() {
        // Nothing in the test run stores another value; the Umicraft trees
        // default is Smart, the reference is Fancy.
        assert_eq!(
            current(),
            MeshSettings {
                leaves: Leaves::Smart,
                connected_textures: ConnectedTextures::Fast,
                ..MeshSettings::default()
            }
        );
        assert_eq!(
            Lighting::from_option(VideoOptions::default().smooth_lighting),
            MeshSettings::default().lighting
        );
        assert_eq!(
            VideoOptions::default().block_alternatives,
            MeshSettings::default().block_alternatives
        );
    }

    #[test]
    fn the_encodings_round_trip() {
        for mode in [LeavesMode::Fast, LeavesMode::Smart, LeavesMode::Fancy] {
            let leaves = leaves_from_u8(leaves_to_u8(mode));
            let expected = match mode {
                LeavesMode::Fast => Leaves::Fast,
                LeavesMode::Smart => Leaves::Smart,
                LeavesMode::Fancy => Leaves::Fancy,
            };
            assert_eq!(leaves, expected);
        }
        for (choice, expected) in [
            (OffFastFancy::Off, BetterGrass::Off),
            (OffFastFancy::Fast, BetterGrass::Fast),
            (OffFastFancy::Fancy, BetterGrass::Fancy),
        ] {
            assert_eq!(grass_from_u8(choice_to_u8(choice)), expected);
        }
        for (choice, expected) in [
            (OffFastFancy::Off, ConnectedTextures::Off),
            (OffFastFancy::Fast, ConnectedTextures::Fast),
            (OffFastFancy::Fancy, ConnectedTextures::Fancy),
        ] {
            assert_eq!(connected_from_u8(choice_to_u8(choice)), expected);
        }
    }
}

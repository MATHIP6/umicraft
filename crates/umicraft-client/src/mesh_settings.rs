// SPDX-License-Identifier: MIT OR Apache-2.0

//! The video options the mesh workers read (ADR-0048).
//!
//! The workers run on their own threads and hold only an immutable mesher, so
//! an option that changes what they emit is one process-wide value, like the
//! GUI scale: the window stores it, then asks the game thread to re-mesh every
//! loaded column (`GameEvent::ReMeshAll`).

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use umicraft_mesh::{Lighting, MeshSettings};

use crate::options::{VideoOptions, MAX_SMOOTH_LIGHTING};

/// The `Smooth Lighting` option value the workers mesh with.
static SMOOTH_LIGHTING: AtomicU8 = AtomicU8::new(MAX_SMOOTH_LIGHTING);
/// The `Other Block Variants` option the workers mesh with.
static BLOCK_ALTERNATIVES: AtomicBool = AtomicBool::new(true);

/// Stores the mesh-affecting options of `video`. Returns whether any value
/// changed, so the caller re-meshes only then.
pub fn store(video: &VideoOptions) -> bool {
    let lighting =
        SMOOTH_LIGHTING.swap(video.smooth_lighting, Ordering::Relaxed) != video.smooth_lighting;
    let alternatives = BLOCK_ALTERNATIVES.swap(video.block_alternatives, Ordering::Relaxed)
        != video.block_alternatives;
    lighting || alternatives
}

/// The settings the mesh workers use right now.
#[must_use]
pub fn current() -> MeshSettings {
    MeshSettings {
        lighting: Lighting::from_option(SMOOTH_LIGHTING.load(Ordering::Relaxed)),
        block_alternatives: BLOCK_ALTERNATIVES.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_reference_defaults() {
        // Nothing in the test run stores another value.
        assert_eq!(current(), MeshSettings::default());
        assert_eq!(
            Lighting::from_option(VideoOptions::default().smooth_lighting),
            MeshSettings::default().lighting
        );
        assert_eq!(
            VideoOptions::default().block_alternatives,
            MeshSettings::default().block_alternatives
        );
    }
}

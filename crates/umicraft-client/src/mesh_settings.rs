// SPDX-License-Identifier: MIT OR Apache-2.0

//! The video options the mesh workers read (ADR-0048).
//!
//! The workers run on their own threads and hold only an immutable mesher, so
//! an option that changes what they emit is one process-wide value, like the
//! GUI scale: the window stores it, then asks the game thread to re-mesh every
//! loaded column (`GameEvent::ReMeshAll`).

use std::sync::atomic::{AtomicU8, Ordering};

use umicraft_mesh::Lighting;

use crate::options::MAX_SMOOTH_LIGHTING;

/// The `Smooth Lighting` option value the workers mesh with.
static SMOOTH_LIGHTING: AtomicU8 = AtomicU8::new(MAX_SMOOTH_LIGHTING);

/// Stores the `Smooth Lighting` option (`0` off, `1` minimum, `2` maximum).
/// Returns whether the value changed, so the caller re-meshes only then.
pub fn set_smooth_lighting(level: u8) -> bool {
    store_changed(&SMOOTH_LIGHTING, level)
}

/// Stores `level` in `cell` and reports whether it differed.
fn store_changed(cell: &AtomicU8, level: u8) -> bool {
    cell.swap(level, Ordering::Relaxed) != level
}

/// The lighting the mesh workers use right now.
#[must_use]
pub fn lighting() -> Lighting {
    Lighting::from_option(SMOOTH_LIGHTING.load(Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_value_reports_a_change_once() {
        // A local cell: the real one is read by every mesh test.
        let cell = AtomicU8::new(2);
        assert!(store_changed(&cell, 0));
        assert!(!store_changed(&cell, 0), "no change, no re-mesh");
        assert!(store_changed(&cell, 1));
    }

    #[test]
    fn the_default_is_the_maximum_smooth_lighting() {
        assert_eq!(Lighting::from_option(MAX_SMOOTH_LIGHTING), Lighting::Smooth);
        assert_eq!(SMOOTH_LIGHTING.load(Ordering::Relaxed), MAX_SMOOTH_LIGHTING);
    }
}

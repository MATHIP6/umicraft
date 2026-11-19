// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example mod: panicking.
//!
//! A mod whose `init` panics. `umicraft_mod!` catches the panic at the C boundary and
//! returns `UMI_STATUS_PANICKED`, so the host reports a load failure instead of the
//! process aborting. It exists to test that guarantee; do not copy it.

use umicraft_modapi::{umicraft_mod, InitCx, Mod, ModError};

/// A mod that never finishes initialising.
struct Panicking;

impl Mod for Panicking {
    /// Always panics, on purpose.
    fn init(_cx: &mut InitCx<'_>) -> Result<Self, ModError> {
        panic!("panicking example: init failed on purpose");
    }
}

umicraft_mod!(Panicking);

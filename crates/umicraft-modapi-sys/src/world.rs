// SPDX-License-Identifier: MIT OR Apache-2.0

//! World layers (API 1.4, ADR-0097): geometry a mod draws in world coordinates
//! each frame, and the entities it marks for the mask post passes read.

use core::ffi::c_void;

use crate::UmiStatus;

/// The host's buffer a world layer fills while it is drawn; opaque to mods.
#[repr(C)]
pub struct UmiWorldCx {
    _private: [u8; 0],
}

/// Fills a world layer for the current frame through the `world_*` host
/// functions. Called on the render thread once per frame in a world, after the
/// camera layers, within a time budget.
pub type UmiWorldDrawFn = extern "C" fn(world: *mut UmiWorldCx, user: *mut c_void) -> UmiStatus;

/// `world_*` flag: drawn over the blocks in front of it instead of hidden by
/// them (no depth test).
pub const UMI_WORLD_THROUGH_WALLS: u32 = 1;
/// `world_box` flag: fills the six faces instead of drawing the twelve edges.
pub const UMI_WORLD_FILL: u32 = 2;

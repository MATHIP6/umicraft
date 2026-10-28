// SPDX-License-Identifier: MIT OR Apache-2.0

//! Item rendering (P3): the GUI icon atlas and the provider contract.
//!
//! Item icons are rendered once through the terrain pipeline (the item
//! sprites share the block texture array) into small cells of an icon atlas.
//! The UI then draws them as ordinary textured rectangles, so slot
//! backgrounds, icons, stack counts and tooltips keep their command order
//! and a steady-state frame renders no item geometry at all.

mod icon_atlas;

pub(crate) use icon_atlas::IconAtlas;
pub use icon_atlas::{ICON_ATLAS_SIZE, ICON_CELL_SIZE};

use umicraft_mesh::item::ItemMesh;

/// Supplies the geometry of an icon the atlas has not rendered yet.
///
/// The key is the opaque `u64` a UI slot carries
/// (`umicraft_ui::DrawCommand::Item`); the client decides what it encodes
/// (item id, damage, tint, ...). The mesh must already be posed into the
/// 16x16 GUI box with [`umicraft_model::item::pose::gui_pose`].
pub trait IconProvider {
    /// Fills `mesh` with the posed geometry of `key`.
    ///
    /// Returns `false` when the key names no drawable item; nothing is drawn
    /// for it and it is asked again on a later frame.
    fn build_icon(&mut self, key: u64, mesh: &mut ItemMesh) -> bool;
}

/// A provider for callers without items: every icon is unavailable.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoIcons;

impl IconProvider for NoIcons {
    fn build_icon(&mut self, _key: u64, _mesh: &mut ItemMesh) -> bool {
        false
    }
}

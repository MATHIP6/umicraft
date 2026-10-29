// SPDX-License-Identifier: MIT OR Apache-2.0

//! Data-driven inventory and container screens (vanilla 1.8 layouts).
//!
//! Every screen is a [`ScreenSpec`]: a GUI texture, its visible size and the
//! position of each slot, all in unscaled texture pixels. The numbers come
//! from the vanilla `Container*` classes (slot positions) and `Gui*` classes
//! (texture, size, progress sprites). [`ScreenKind::spec`] dispatches to one
//! file per screen and [`ScreenLayout`] places a spec in a viewport: auto GUI
//! scale ([`gui_scale`]), centring, slot rectangles and hit testing.
//!
//! Slot indices are *window* indices as the protocol numbers them: the player
//! window is 0 result, 1-4 grid, 5-8 armour, 9-35 backpack, 36-44 hotbar; a
//! container window has its own slots first, then 27 backpack and 9 hotbar
//! slots.
//!
//! ```
//! use umicraft_ui::screens::{ScreenKind, ScreenLayout};
//! use umicraft_ui::Rect;
//!
//! let layout = ScreenLayout::new(Rect::new(0.0, 0.0, 1280.0, 720.0), ScreenKind::Furnace);
//! assert_eq!(layout.slots.len(), 39);
//! ```

mod chest;
mod crafting;
mod decoration;
mod dispenser;
mod furnace;
mod hopper;
mod layout;
mod player;
mod scale;
mod screen_layout;

pub use decoration::{Decoration, Fill, SpriteRect};
pub use scale::gui_scale;
pub use screen_layout::ScreenLayout;

/// Which screen to show. More kinds are added as their windows are supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScreenKind {
    /// The player inventory (window 0).
    Player,
    /// A chest-like container with `rows` rows of nine slots (1..=6).
    Chest {
        /// Number of slot rows.
        rows: u8,
    },
    /// The crafting table.
    CraftingTable,
    /// The furnace.
    Furnace,
    /// The 3x3 dispenser, also used for droppers.
    Dispenser,
    /// The five-slot hopper.
    Hopper,
}

/// One slot of a screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotSpec {
    /// The window slot index.
    pub index: usize,
    /// Left edge of the 16x16 item area, in unscaled texture pixels.
    pub x: u32,
    /// Top edge of the 16x16 item area, in unscaled texture pixels.
    pub y: u32,
}

/// The static description of a screen.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenSpec {
    /// Name of the GUI atlas region, e.g. `"container/furnace"`.
    pub texture: &'static str,
    /// Visible width and height in unscaled pixels.
    pub size: (u32, u32),
    /// The pieces of the texture that make up the background. Most screens
    /// have one; chests have two (see `chest.rs`).
    pub background: Vec<Decoration>,
    /// The slots, in window order.
    pub slots: Vec<SlotSpec>,
}

impl ScreenSpec {
    /// A spec whose background is the texture's top-left `size` rectangle.
    fn single(texture: &'static str, size: (u32, u32), slots: Vec<SlotSpec>) -> Self {
        Self {
            texture,
            size,
            background: vec![Decoration::full((0, 0), size, (0, 0))],
            slots,
        }
    }
}

impl ScreenKind {
    /// The static layout of this screen.
    #[must_use]
    pub fn spec(self) -> ScreenSpec {
        match self {
            Self::Player => player::spec(),
            Self::Chest { rows } => chest::spec(rows),
            Self::CraftingTable => crafting::spec(),
            Self::Furnace => furnace::spec(),
            Self::Dispenser => dispenser::spec(),
            Self::Hopper => hopper::spec(),
        }
    }

    /// The dynamic sprites (flames, progress arrows) for the window
    /// `properties` the server sent. Screens without any return an empty list.
    #[must_use]
    pub fn decorations(self, properties: &[i16]) -> Vec<Decoration> {
        match self {
            Self::Furnace => furnace::decorations(properties),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [ScreenKind; 8] = [
        ScreenKind::Player,
        ScreenKind::Chest { rows: 3 },
        ScreenKind::Chest { rows: 6 },
        ScreenKind::CraftingTable,
        ScreenKind::Furnace,
        ScreenKind::Dispenser,
        ScreenKind::Hopper,
        ScreenKind::Chest { rows: 1 },
    ];

    #[test]
    fn every_screen_has_unique_contiguous_slot_indices_inside_its_texture() {
        for kind in KINDS {
            let spec = kind.spec();
            for (i, slot) in spec.slots.iter().enumerate() {
                assert_eq!(slot.index, i, "{kind:?}");
                assert!(slot.x + 16 <= spec.size.0, "{kind:?} slot {i}");
                assert!(slot.y + 16 <= spec.size.1, "{kind:?} slot {i}");
            }
        }
    }

    #[test]
    fn container_screens_end_with_27_backpack_and_9_hotbar_slots() {
        for kind in KINDS.into_iter().filter(|k| *k != ScreenKind::Player) {
            let spec = kind.spec();
            let own = spec.slots.len() - 36;
            assert_eq!(spec.slots[own].x, 8, "{kind:?}");
            assert_eq!(spec.slots[own + 27].y, spec.slots[own].y + 58, "{kind:?}");
        }
    }

    #[test]
    fn only_the_furnace_has_decorations() {
        assert!(ScreenKind::Hopper.decorations(&[1, 2, 3, 4]).is_empty());
        assert!(!ScreenKind::Furnace.decorations(&[]).is_empty());
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! What a minecart carries (MCP 1.8.9 `EntityMinecart`,
//! `EntityMinecartFurnace`, `EntityMinecartTNT`).
//!
//! Every cart has a display tile, the block drawn inside it: the default one
//! of its type, or the one a command set (entries 20 to 22). A furnace cart
//! that burns fuel shows a lit furnace (entry 16), and a TNT cart that was set
//! off counts its fuse down from 80 once the server sends status 10.

use umicraft_protocol::packets::play::Metadata;

/// `EntityMinecartFurnace`: entry 16, a byte, non-zero while it burns fuel.
const POWERED: u8 = 16;
/// `EntityMinecart.entityInit`: the block state id of the custom tile.
const DISPLAY_TILE: u8 = 20;
/// The height of the custom tile over the cart floor, in pixels.
const DISPLAY_OFFSET: u8 = 21;
/// Whether the custom tile replaces the default one.
const HAS_DISPLAY_TILE: u8 = 22;
/// `getDisplayTileOffset` before entry 21 is sent.
const DEFAULT_DISPLAY_OFFSET: i32 = 6;
/// The fuse a TNT cart starts with when it is set off (`ignite`).
pub const TNT_CART_FUSE: i32 = 80;

/// The block a command put in a cart: a 1.8 block state id (`id | meta << 12`)
/// and its height over the floor in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayTile {
    /// `Block.getStateId`: the block id in the low 12 bits, the meta above.
    pub state: i32,
    /// `getDisplayTileOffset`.
    pub offset: i32,
}

impl DisplayTile {
    /// The block id of the tile.
    #[must_use]
    pub const fn block_id(self) -> i32 {
        self.state & 0xFFF
    }

    /// The block metadata of the tile.
    #[must_use]
    pub const fn metadata(self) -> i32 {
        (self.state >> 12) & 0xF
    }
}

/// The state of a minecart beyond its shake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CartMeta {
    /// A furnace cart burning fuel (`isMinecartPowered`).
    pub powered: bool,
    /// Whether entry 22 is set: the custom tile replaces the default one.
    pub custom_tile: bool,
    /// The custom tile; used only while [`Self::custom_tile`] is set.
    pub tile: DisplayTile,
    /// Ticks left before a lit TNT cart blows up, `-1` until it is lit
    /// (`minecartTNTFuse`).
    pub fuse: i32,
}

impl Default for CartMeta {
    fn default() -> Self {
        Self {
            powered: false,
            custom_tile: false,
            tile: DisplayTile {
                state: 0,
                offset: DEFAULT_DISPLAY_OFFSET,
            },
            fuse: -1,
        }
    }
}

impl CartMeta {
    /// The custom display tile, when entry 22 asks for it.
    #[must_use]
    pub fn display_tile(&self) -> Option<DisplayTile> {
        self.custom_tile.then_some(self.tile)
    }

    /// Merges the cart entries of `metadata`.
    pub(super) fn apply(&mut self, metadata: &Metadata) {
        if let Some(powered) = metadata.get_byte(POWERED) {
            self.powered = powered != 0;
        }
        if let Some(state) = metadata.get_int(DISPLAY_TILE) {
            self.tile.state = state;
        }
        if let Some(offset) = metadata.get_int(DISPLAY_OFFSET) {
            self.tile.offset = offset;
        }
        if let Some(custom) = metadata.get_byte(HAS_DISPLAY_TILE) {
            self.custom_tile = custom != 0;
        }
    }

    /// Sets a TNT cart off (`EntityMinecartTNT.handleStatusUpdate(10)`).
    pub fn ignite(&mut self) {
        self.fuse = TNT_CART_FUSE;
    }

    /// One tick of the fuse; it stops at 0, where the server blows the cart
    /// up (`EntityMinecartTNT.onUpdate`).
    pub(super) fn tick(&mut self) {
        if self.fuse > 0 {
            self.fuse -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use umicraft_protocol::packets::play::metadata::WatchedObject;

    use super::*;

    fn entry(type_id: u8, index: u8, value: Vec<u8>) -> WatchedObject {
        WatchedObject {
            type_id,
            index,
            value,
        }
    }

    #[test]
    fn the_cart_entries_set_the_furnace_and_the_custom_tile() {
        let mut cart = CartMeta::default();
        assert_eq!(cart.display_tile(), None);
        cart.apply(&Metadata {
            objects: vec![
                entry(0, POWERED, vec![1]),
                // Stone with meta 1 (granite): `1 | 1 << 12`.
                entry(2, DISPLAY_TILE, (1_i32 | 1 << 12).to_be_bytes().to_vec()),
                entry(2, DISPLAY_OFFSET, 3_i32.to_be_bytes().to_vec()),
                entry(0, HAS_DISPLAY_TILE, vec![1]),
            ],
        });
        assert!(cart.powered);
        let tile = cart.display_tile().expect("custom tile");
        assert_eq!((tile.block_id(), tile.metadata(), tile.offset), (1, 1, 3));
    }

    #[test]
    fn a_lit_fuse_counts_down_to_zero_and_stays_there() {
        let mut cart = CartMeta::default();
        cart.tick();
        assert_eq!(cart.fuse, -1, "not lit");
        cart.ignite();
        for _ in 0..100 {
            cart.tick();
        }
        assert_eq!(cart.fuse, 0);
    }
}

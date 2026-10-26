// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Click Window` (0x0E, serverbound, play): the client clicks a slot.
//!
//! Layout (MCP `C0EPacketClickWindow`, `readPacketData`):
//!
//! | field | width | notes |
//! |---|---|---|
//! | window id | i8 | |
//! | slot | i16 | `-999` means outside the window |
//! | button | i8 | left=0, right=1 (mode-dependent) |
//! | action number | i16 | monotonically increasing per window |
//! | mode | i8 | 0=normal, 1=shift, 2=number key, 3=middle, 4=drop, 5=drag, 6=double |
//! | clicked item | ItemStack | the item that was in the slot at click time |

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::wire::{read_i16, read_i8, write_i16, write_i8};
use crate::packets::play::ItemStack;
use crate::packets::EncodablePacket;

/// `Click Window` (0x0E, serverbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClickWindow {
    /// The window id.
    pub window_id: i8,
    /// The slot that was clicked; `-999` for outside-window clicks.
    pub slot: i16,
    /// The mouse button (meaning depends on `mode`).
    pub button: i8,
    /// The transaction action number.
    pub action_number: i16,
    /// The click mode (0–6).
    pub mode: i8,
    /// The item that was in the slot at the time of the click.
    pub clicked_item: ItemStack,
}

impl ClickWindow {
    /// Wire packet id.
    pub const ID: u8 = 0x0E;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on stream truncation,
    /// [`ProtocolError::Invariant`] on invalid NBT.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let window_id = read_i8(reader)?;
        let slot = read_i16(reader)?;
        let button = read_i8(reader)?;
        let action_number = read_i16(reader)?;
        let mode = read_i8(reader)?;
        let clicked_item = ItemStack::decode(reader)?;
        Ok(Self {
            window_id,
            slot,
            button,
            action_number,
            mode,
            clicked_item,
        })
    }
}

impl EncodablePacket for ClickWindow {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_i8(writer, self.window_id)?;
        write_i16(writer, self.slot)?;
        write_i8(writer, self.button)?;
        write_i16(writer, self.action_number)?;
        write_i8(writer, self.mode)?;
        self.clicked_item.encode(writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(packet: &ClickWindow) -> ClickWindow {
        let mut buf = Vec::new();
        packet.encode_body(&mut buf).unwrap();
        ClickWindow::decode(&mut &buf[..]).unwrap()
    }

    #[test]
    fn a_normal_left_click_round_trips() {
        let packet = ClickWindow {
            window_id: 0,
            slot: 36,
            button: 0,
            action_number: 1,
            mode: 0,
            clicked_item: ItemStack {
                item_id: 1,
                stack_size: 64,
                damage: 0,
                nbt: None,
            },
        };
        assert_eq!(round_trip(&packet), packet);
    }

    #[test]
    fn an_outside_click_round_trips() {
        let packet = ClickWindow {
            window_id: 0,
            slot: -999,
            button: 0,
            action_number: 2,
            mode: 4,
            clicked_item: ItemStack::EMPTY,
        };
        assert_eq!(round_trip(&packet), packet);
    }
}

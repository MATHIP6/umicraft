// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Window Items` (0x30, clientbound, play): the server sends every slot of
//! a window in one bulk update.
//!
//! Layout (MCP `S30PacketWindowItems`, `readPacketData`):
//!
//! | field | width | notes |
//! |---|---|---|
//! | window id | u8 | |
//! | count | i16 | number of item stacks that follow |
//! | items | ItemStack × count | one per slot |

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_u8;
use crate::packets::play::wire::{read_i16, write_i16};
use crate::packets::play::ItemStack;
use crate::packets::EncodablePacket;

/// The largest number of slots the protocol can describe in one packet.
///
/// A vanilla 1.8 large chest is 90 slots (54 + 36 player); 256 leaves
/// headroom for horse and mod containers while rejecting absurd values
/// before allocating.
const MAX_SLOTS: usize = 256;

/// `Window Items` (0x30, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowItems {
    /// The window id; `0` is the player inventory.
    pub window_id: u8,
    /// One item stack per slot, in window-index order.
    pub items: Vec<ItemStack>,
}

impl WindowItems {
    /// Wire packet id.
    pub const ID: u8 = 0x30;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on stream truncation,
    /// [`ProtocolError::Invariant`] on invalid fields.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let window_id = read_u8(reader)?;
        let count = read_i16(reader)?;
        if count < 0 {
            return Err(ProtocolError::Invariant(format!(
                "WindowItems count is negative: {count}"
            )));
        }
        let count = count as usize;
        if count > MAX_SLOTS {
            return Err(ProtocolError::Invariant(format!(
                "WindowItems count {count} exceeds the limit of {MAX_SLOTS}"
            )));
        }
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            items.push(ItemStack::decode(reader)?);
        }
        Ok(Self { window_id, items })
    }
}

impl EncodablePacket for WindowItems {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&[self.window_id])?;
        write_i16(writer, self.items.len() as i16)?;
        for item in &self.items {
            item.encode(writer)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(packet: &WindowItems) -> WindowItems {
        let mut buf = Vec::new();
        packet.encode_body(&mut buf).unwrap();
        WindowItems::decode(&mut &buf[..]).unwrap()
    }

    #[test]
    fn an_empty_window_round_trips() {
        let packet = WindowItems {
            window_id: 0,
            items: vec![],
        };
        assert_eq!(round_trip(&packet), packet);
    }

    #[test]
    fn a_filled_window_round_trips() {
        let packet = WindowItems {
            window_id: 1,
            items: vec![
                ItemStack {
                    item_id: 1,
                    stack_size: 64,
                    damage: 0,
                    nbt: None,
                },
                ItemStack::EMPTY,
                ItemStack {
                    item_id: 276,
                    stack_size: 1,
                    damage: 42,
                    nbt: None,
                },
            ],
        };
        assert_eq!(round_trip(&packet), packet);
    }

    #[test]
    fn a_negative_count_is_rejected() {
        let mut buf = Vec::new();
        buf.push(0); // window_id
        buf.extend_from_slice(&(-1i16).to_be_bytes()); // count
        assert!(WindowItems::decode(&mut &buf[..]).is_err());
    }
}

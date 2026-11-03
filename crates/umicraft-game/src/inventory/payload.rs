// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `MC|...` plugin-message payloads of the container screens.
//!
//! The 1.8 client talks to anvils, beacons, villagers and books through
//! custom payload channels (MCP `GuiRepair.renameItem`, `GuiBeacon`,
//! `GuiMerchant`, `GuiScreenBook`, `NetHandlerPlayClient.handleCustomPayload`).
//! This module builds the serverbound payloads and parses `MC|TrList`.
//! Integers are big-endian; strings are VarInt-prefixed UTF-8 and item stacks
//! use the `Slot` encoding of the protocol crate.

use std::io::{Cursor, Read};

use thiserror::Error;
use umicraft_protocol::packets::play::ItemStack;
use umicraft_protocol::types::write_string;
use umicraft_protocol::ProtocolError;

/// Channel of the anvil rename (serverbound).
pub const ITEM_NAME: &str = "MC|ItemName";
/// Channel of the beacon effect choice (serverbound).
pub const BEACON: &str = "MC|Beacon";
/// Channel of the villager offer choice (serverbound).
pub const TRADE_SELECT: &str = "MC|TrSel";
/// Channel of the villager offers (clientbound).
pub const TRADE_LIST: &str = "MC|TrList";
/// Channel of a saved book draft (serverbound).
pub const BOOK_EDIT: &str = "MC|BEdit";
/// Channel of a signed book (serverbound).
pub const BOOK_SIGN: &str = "MC|BSign";

/// Longest name the anvil text field takes (`GuiTextField.setMaxStringLength(40)`).
pub const ANVIL_NAME_LIMIT: usize = 40;

/// Why a payload could not be built or read.
#[derive(Debug, Error)]
pub enum PayloadError {
    /// A field is malformed or the payload is truncated.
    #[error("malformed payload: {0}")]
    Protocol(#[from] ProtocolError),
    /// The payload ended early.
    #[error("truncated payload")]
    Truncated(#[from] std::io::Error),
}

/// One offer of a villager (`MerchantRecipe`).
#[derive(Debug, Clone, PartialEq)]
pub struct Trade {
    /// The first item the villager wants.
    pub buy: ItemStack,
    /// The optional second item.
    pub buy_second: Option<ItemStack>,
    /// What the player gets.
    pub sell: ItemStack,
    /// Whether the offer ran out (`uses >= max_uses`).
    pub disabled: bool,
    /// How many times it was used.
    pub uses: i32,
    /// How many uses it allows.
    pub max_uses: i32,
}

/// The offers of a villager window.
#[derive(Debug, Clone, PartialEq)]
pub struct TradeList {
    /// The window the offers belong to.
    pub window_id: i32,
    /// The offers, in the order the server listed them.
    pub trades: Vec<Trade>,
}

/// The payload of `MC|ItemName`: the new name, or an empty string to reset it.
///
/// # Errors
/// [`PayloadError::Protocol`] when `name` is longer than the field allows.
pub fn item_name(name: &str) -> Result<Vec<u8>, PayloadError> {
    let mut data = Vec::new();
    write_string(&mut data, name, ANVIL_NAME_LIMIT)?;
    Ok(data)
}

/// The payload of `MC|Beacon`: the primary and secondary effect ids (0 for
/// none, `GuiBeacon.actionPerformed`).
#[must_use]
pub fn beacon(primary: i32, secondary: i32) -> Vec<u8> {
    let mut data = Vec::with_capacity(8);
    data.extend(primary.to_be_bytes());
    data.extend(secondary.to_be_bytes());
    data
}

/// The payload of `MC|TrSel`: the chosen offer index.
#[must_use]
pub fn trade_selection(index: i32) -> Vec<u8> {
    index.to_be_bytes().to_vec()
}

/// The payload of `MC|BEdit` and `MC|BSign`: the book stack.
///
/// # Errors
/// [`PayloadError::Protocol`] when the stack cannot be encoded.
pub fn book(stack: &ItemStack) -> Result<Vec<u8>, PayloadError> {
    let mut data = Vec::new();
    stack.encode(&mut data)?;
    Ok(data)
}

/// Parses an `MC|TrList` payload.
///
/// # Errors
/// [`PayloadError`] when the payload is truncated or holds an invalid stack.
pub fn parse_trades(data: &[u8]) -> Result<TradeList, PayloadError> {
    let mut reader = Cursor::new(data);
    let window_id = read_i32(&mut reader)?;
    let count = read_u8(&mut reader)?;
    let mut trades = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        let buy = ItemStack::decode(&mut reader)?;
        let sell = ItemStack::decode(&mut reader)?;
        let buy_second = if read_u8(&mut reader)? != 0 {
            Some(ItemStack::decode(&mut reader)?)
        } else {
            None
        };
        let disabled = read_u8(&mut reader)? != 0;
        let uses = read_i32(&mut reader)?;
        let max_uses = read_i32(&mut reader)?;
        trades.push(Trade {
            buy,
            buy_second,
            sell,
            disabled,
            uses,
            max_uses,
        });
    }
    Ok(TradeList { window_id, trades })
}

fn read_u8(reader: &mut impl Read) -> Result<u8, std::io::Error> {
    let mut byte = [0];
    reader.read_exact(&mut byte)?;
    Ok(byte[0])
}

fn read_i32(reader: &mut impl Read) -> Result<i32, std::io::Error> {
    let mut bytes = [0; 4];
    reader.read_exact(&mut bytes)?;
    Ok(i32::from_be_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(item_id: i16, count: i8) -> ItemStack {
        ItemStack {
            item_id,
            stack_size: count,
            damage: 0,
            nbt: None,
        }
    }

    fn encode_trade(out: &mut Vec<u8>, trade: &Trade) {
        trade.buy.encode(out).unwrap();
        trade.sell.encode(out).unwrap();
        out.push(u8::from(trade.buy_second.is_some()));
        if let Some(second) = &trade.buy_second {
            second.encode(out).unwrap();
        }
        out.push(u8::from(trade.disabled));
        out.extend(trade.uses.to_be_bytes());
        out.extend(trade.max_uses.to_be_bytes());
    }

    #[test]
    fn rename_is_a_varint_prefixed_string() {
        assert_eq!(item_name("Excalibur").unwrap()[..2], [9, b'E']);
        assert_eq!(item_name("").unwrap(), [0]);
    }

    #[test]
    fn rename_rejects_names_over_the_field_limit() {
        assert!(item_name(&"a".repeat(ANVIL_NAME_LIMIT + 1)).is_err());
        assert!(item_name(&"a".repeat(ANVIL_NAME_LIMIT)).is_ok());
    }

    #[test]
    fn beacon_and_trade_selection_are_big_endian_ints() {
        assert_eq!(beacon(1, 10), [0, 0, 0, 1, 0, 0, 0, 10]);
        assert_eq!(trade_selection(258), [0, 0, 1, 2]);
    }

    #[test]
    fn trade_list_round_trips() {
        let trades = vec![
            Trade {
                buy: stack(297, 20),
                buy_second: None,
                sell: stack(388, 1),
                disabled: false,
                uses: 0,
                max_uses: 7,
            },
            Trade {
                buy: stack(265, 3),
                buy_second: Some(stack(1, 1)),
                sell: stack(276, 1),
                disabled: true,
                uses: 7,
                max_uses: 7,
            },
        ];
        let mut data = 5_i32.to_be_bytes().to_vec();
        data.push(2);
        trades.iter().for_each(|t| encode_trade(&mut data, t));
        let list = parse_trades(&data).unwrap();
        assert_eq!(list.window_id, 5);
        assert_eq!(list.trades, trades);
    }

    #[test]
    fn truncated_trade_lists_are_errors_not_panics() {
        let mut data = 5_i32.to_be_bytes().to_vec();
        data.push(1);
        assert!(parse_trades(&data).is_err());
        assert!(parse_trades(&[]).is_err());
    }
}

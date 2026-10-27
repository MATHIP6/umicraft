// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Entity Properties` (0x20, clientbound, play): attribute values of a
//! living entity (max health, movement speed, ...) with their modifiers.
//!
//! Layout (MCP `S20PacketEntityProperties`, `readPacketData`): entity id
//! (VarInt), property count (i32), then per property a name (string, at most
//! 64 chars), a base value (f64), a modifier count (VarInt) and per modifier
//! a UUID (two big-endian i64: most then least significant half), an amount
//! (f64) and an operation (i8: 0 add, 1 multiply base, 2 multiply total).
//!
//! Both counts come from the wire, so they are checked against fixed caps
//! before anything is allocated; vanilla sends a few dozen properties at most.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_f64, read_i32};
use crate::packets::play::wire::{read_i8, write_i8};
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};
use crate::varint::{read_var_i32, write_var_u32};

/// Longest attribute name the reference client reads.
const MAX_NAME_CHARS: usize = 64;
/// Most properties accepted in one packet.
const MAX_PROPERTIES: usize = 1024;
/// Most modifiers accepted on one property.
const MAX_MODIFIERS: usize = 1024;
/// Initial `Vec` capacity; the vectors grow with the data actually received.
const INITIAL_CAPACITY: usize = 16;

/// One modifier applied to an attribute.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttributeModifier {
    /// Modifier UUID (most significant half first).
    pub uuid: u128,
    /// Amount added or multiplied, depending on `operation`.
    pub amount: f64,
    /// Operation: 0 add, 1 multiply base, 2 multiply total.
    pub operation: i8,
}

/// One attribute with its base value and modifiers.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityProperty {
    /// Attribute name, for example `generic.movementSpeed`.
    pub key: String,
    /// Base value before modifiers.
    pub value: f64,
    /// Modifiers in wire order.
    pub modifiers: Vec<AttributeModifier>,
}

/// `Entity Properties` (0x20, clientbound, play).
#[derive(Debug, Clone, PartialEq)]
pub struct EntityProperties {
    /// Entity the attributes belong to.
    pub entity_id: i32,
    /// Attributes in wire order.
    pub properties: Vec<EntityProperty>,
}

/// Validates a wire count against `max` and converts it to `usize`.
fn checked_count(count: i32, max: usize, what: &str) -> Result<usize, ProtocolError> {
    usize::try_from(count)
        .ok()
        .filter(|&n| n <= max)
        .ok_or_else(|| ProtocolError::Invariant(format!("{what} count {count} is out of range")))
}

fn read_modifier(reader: &mut impl Read) -> Result<AttributeModifier, ProtocolError> {
    let mut uuid = [0u8; 16];
    reader.read_exact(&mut uuid)?;
    Ok(AttributeModifier {
        uuid: u128::from_be_bytes(uuid),
        amount: read_f64(reader)?,
        operation: read_i8(reader)?,
    })
}

fn read_property(reader: &mut impl Read) -> Result<EntityProperty, ProtocolError> {
    let key = read_string(reader, MAX_NAME_CHARS)?;
    let value = read_f64(reader)?;
    let count = checked_count(read_var_i32(reader)?, MAX_MODIFIERS, "modifier")?;
    let mut modifiers = Vec::with_capacity(count.min(INITIAL_CAPACITY));
    for _ in 0..count {
        modifiers.push(read_modifier(reader)?);
    }
    Ok(EntityProperty {
        key,
        value,
        modifiers,
    })
}

impl EntityProperties {
    /// Wire packet id.
    pub const ID: u8 = 0x20;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, a malformed VarInt, an over-long
    /// name, or a negative or oversized property or modifier count.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let entity_id = read_var_i32(reader)?;
        let count = checked_count(read_i32(reader)?, MAX_PROPERTIES, "property")?;
        let mut properties = Vec::with_capacity(count.min(INITIAL_CAPACITY));
        for _ in 0..count {
            properties.push(read_property(reader)?);
        }
        Ok(Self {
            entity_id,
            properties,
        })
    }
}

impl EncodablePacket for EntityProperties {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        // Lengths are bounded by the in-memory vectors; i32 cannot overflow
        // for any packet that fits the 2 MiB frame limit.
        writer.write_all(&(self.properties.len() as i32).to_be_bytes())?;
        for property in &self.properties {
            write_string(writer, &property.key, MAX_NAME_CHARS)?;
            writer.write_all(&property.value.to_be_bytes())?;
            write_var_u32(writer, property.modifiers.len() as u32)?;
            for modifier in &property.modifiers {
                writer.write_all(&modifier.uuid.to_be_bytes())?;
                writer.write_all(&modifier.amount.to_be_bytes())?;
                write_i8(writer, modifier.operation)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> EntityProperties {
        EntityProperties {
            entity_id: 2,
            properties: vec![EntityProperty {
                key: "generic.maxHealth".into(),
                value: 20.0,
                modifiers: vec![AttributeModifier {
                    uuid: 0x0102_0304_0506_0708_090A_0B0C_0D0E_0F10,
                    amount: 0.5,
                    operation: 2,
                }],
            }],
        }
    }

    #[test]
    fn entity_properties_wire_shape() {
        let mut expected = vec![0x02];
        expected.extend_from_slice(&1i32.to_be_bytes());
        expected.push(17);
        expected.extend_from_slice(b"generic.maxHealth");
        expected.extend_from_slice(&20.0f64.to_be_bytes());
        expected.push(1); // one modifier
        expected.extend(1..=16u8); // uuid
        expected.extend_from_slice(&0.5f64.to_be_bytes());
        expected.push(2);

        let packet = sample();
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(
            EntityProperties::decode(&mut &expected[..]).unwrap(),
            packet
        );
    }

    #[test]
    fn hostile_counts_and_truncation_are_errors() {
        let mut buffer = Vec::new();
        sample().encode_body(&mut buffer).unwrap();
        assert!(EntityProperties::decode(&mut &buffer[..buffer.len() - 1][..]).is_err());

        let mut negative = vec![0x02];
        negative.extend_from_slice(&(-1i32).to_be_bytes());
        assert!(EntityProperties::decode(&mut &negative[..]).is_err());

        let mut huge = vec![0x02];
        huge.extend_from_slice(&i32::MAX.to_be_bytes());
        assert!(EntityProperties::decode(&mut &huge[..]).is_err());
    }
}

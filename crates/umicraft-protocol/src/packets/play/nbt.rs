// SPDX-License-Identifier: MIT OR Apache-2.0

//! Optional NBT compounds as 1.8 puts them on the wire.
//!
//! Item stacks (`PacketBuffer.readItemStackFromBuffer`), `Update Block
//! Entity` (0x35) and `Update Entity NBT` (0x49) carry an uncompressed NBT
//! compound read by MCP `PacketBuffer.readNBTTagCompoundFromBuffer`. The
//! client does not interpret the tags yet, so the compound is validated and
//! captured verbatim; consumers (signs, skulls, banners, item display) parse
//! the bytes they need later.

use std::io::Read;

use crate::error::ProtocolError;
use crate::packets::codec::{read_i32, read_u8};
use crate::packets::play::wire::{read_i16, skip_bytes, RecordingReader};

/// Maximum nesting depth accepted while skipping an NBT compound.
///
/// Vanilla allows 512; item stacks and block entities never nest deeper than a
/// handful of levels, and the limit keeps a hostile server from exhausting the stack.
const MAX_NBT_DEPTH: u32 = 32;

/// Maximum number of bytes a single NBT length prefix may claim.
///
/// The protocol caps a whole packet at 2 MiB, so anything larger is bogus.
const MAX_NBT_BYTES: usize = 2 * 1024 * 1024;

/// Reads an optional NBT compound and returns its raw bytes.
///
/// # Errors
/// [`ProtocolError`] on truncation, an unknown tag type, a negative or
/// oversized length, or nesting deeper than [`MAX_NBT_DEPTH`].
///
/// 1.8 (`PacketBuffer.readNBTTagCompoundFromBuffer`) writes a single `0x00`
/// byte when there is no compound; otherwise a
/// plain (uncompressed) NBT compound follows — tag type `10`, the compound
/// name, the entries and the terminating `0x00`.
pub(crate) fn read_optional_nbt(reader: &mut impl Read) -> Result<Option<Vec<u8>>, ProtocolError> {
    let mut recorder = RecordingReader::new(reader);
    let mut tag = [0u8; 1];
    recorder.read_exact(&mut tag)?;
    if tag[0] == 0 {
        return Ok(None);
    }
    if tag[0] != 10 {
        return Err(ProtocolError::Invariant(format!(
            "NBT starts with tag type {} instead of a compound",
            tag[0]
        )));
    }
    skip_nbt_name(&mut recorder)?;
    skip_nbt_payload(&mut recorder, 10, 0)?;
    Ok(Some(recorder.into_recorded()))
}

/// Skips the name of an NBT entry (a 16-bit length plus that many bytes).
fn skip_nbt_name(reader: &mut impl Read) -> Result<(), ProtocolError> {
    let len = read_i16(reader)?;
    if len < 0 {
        return Err(ProtocolError::Invariant(format!(
            "negative NBT name length {len}"
        )));
    }
    skip_bytes(reader, len as usize)
}

/// Skips the payload of an NBT tag of the given type.
///
/// The tag type byte and the entry name have already been consumed; the
/// caller is positioned at the payload.
fn skip_nbt_payload(reader: &mut impl Read, tag_type: u8, depth: u32) -> Result<(), ProtocolError> {
    if depth > MAX_NBT_DEPTH {
        return Err(ProtocolError::Invariant(format!(
            "NBT nested deeper than {MAX_NBT_DEPTH} levels"
        )));
    }
    match tag_type {
        0 => {}                      // TAG_End carries no payload
        1 => skip_bytes(reader, 1)?, // TAG_Byte
        2 => skip_bytes(reader, 2)?, // TAG_Short
        3 => skip_bytes(reader, 4)?, // TAG_Int
        4 => skip_bytes(reader, 8)?, // TAG_Long
        5 => skip_bytes(reader, 4)?, // TAG_Float
        6 => skip_bytes(reader, 8)?, // TAG_Double
        7 => {
            // TAG_Byte_Array: int length, then that many bytes.
            let len = read_i32(reader)?;
            skip_bytes(reader, checked_len(len)?)?;
        }
        8 => {
            // TAG_String: short length, then that many bytes.
            let len = read_i16(reader)?;
            if len < 0 {
                return Err(ProtocolError::Invariant(format!(
                    "negative NBT string length {len}"
                )));
            }
            skip_bytes(reader, len as usize)?;
        }
        9 => {
            // TAG_List: element type, int count, then the elements.
            let element_type = read_u8(reader)?;
            let count = read_i32(reader)?;
            if count < 0 {
                return Err(ProtocolError::Invariant(format!(
                    "negative NBT list length {count}"
                )));
            }
            for _ in 0..count {
                skip_nbt_payload(reader, element_type, depth + 1)?;
            }
        }
        10 => {
            // TAG_Compound: entries until the terminating TAG_End.
            loop {
                let mut tag = [0u8; 1];
                reader.read_exact(&mut tag)?;
                if tag[0] == 0 {
                    break;
                }
                skip_nbt_name(reader)?;
                skip_nbt_payload(reader, tag[0], depth + 1)?;
            }
        }
        11 => {
            // TAG_Int_Array: int length, then that many 32-bit values.
            let len = read_i32(reader)?;
            let len = checked_len(len)?;
            skip_bytes(
                reader,
                len.checked_mul(4).ok_or_else(|| {
                    ProtocolError::Invariant("NBT int array length overflows".to_string())
                })?,
            )?;
        }
        other => {
            return Err(ProtocolError::Invariant(format!(
                "unknown NBT tag type {other}"
            )))
        }
    }
    Ok(())
}

/// Converts a length prefix into a `usize`, rejecting negative and
/// implausible values.
fn checked_len(len: i32) -> Result<usize, ProtocolError> {
    if len < 0 {
        return Err(ProtocolError::Invariant(format!(
            "negative NBT length {len}"
        )));
    }
    let len = len as usize;
    if len > MAX_NBT_BYTES {
        return Err(ProtocolError::Invariant(format!(
            "NBT length {len} exceeds the packet budget of {MAX_NBT_BYTES} bytes"
        )));
    }
    Ok(len)
}

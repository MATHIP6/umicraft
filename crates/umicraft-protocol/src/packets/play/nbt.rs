// SPDX-License-Identifier: MIT OR Apache-2.0

//! Optional NBT compounds as 1.8 puts them on the wire.
//!
//! Item stacks (`PacketBuffer.readItemStackFromBuffer`), `Update Block
//! Entity` (0x35) and `Update Entity NBT` (0x49) carry an uncompressed NBT
//! compound read by MCP `PacketBuffer.readNBTTagCompoundFromBuffer`. The
//! client does not interpret the tags yet, so the compound is validated and
//! captured verbatim; consumers (signs, skulls, banners, item display) parse
//! the bytes they need later.
//!
//! The validation applies the reference client's limits, because a few
//! hundred hostile bytes can otherwise describe billions of elements (a list
//! of empty lists): the nesting depth (512), the rejection of a list without
//! an element type, and the 2 MiB budget of `NBTSizeTracker`, charged with
//! the per-tag costs of each `NBTTag*.read` (found by the fuzz targets: a
//! 210-byte item stack took minutes to skip).

use std::io::Read;

use crate::error::ProtocolError;
use crate::packets::codec::{read_i32, read_u8};
use crate::packets::play::wire::{read_i16, skip_bytes, RecordingReader};

/// Maximum nesting depth (MCP `NBTTagCompound.read` / `NBTTagList.read`,
/// `depth > 512`).
const MAX_NBT_DEPTH: u32 = 512;

/// The budget of one compound read from a packet, in the reference client's
/// accounting units (MCP `PacketBuffer.readNBTTagCompoundFromBuffer`,
/// `new NBTSizeTracker(2097152L)`).
const MAX_NBT_BUDGET: u64 = 2_097_152;

/// The `TAG_End` type id.
const TAG_END: u8 = 0;

/// The `TAG_Compound` type id.
const TAG_COMPOUND: u8 = 10;

/// Reads an optional NBT compound and returns its raw bytes.
///
/// 1.8 (`PacketBuffer.readNBTTagCompoundFromBuffer`) writes a single `0x00`
/// byte when there is no compound; otherwise a plain (uncompressed) NBT
/// compound follows — tag type `10`, the compound name, the entries and the
/// terminating `0x00`.
///
/// # Errors
/// [`ProtocolError`] on truncation, an unknown tag type, a negative length,
/// a list of `TAG_End` elements, nesting deeper than 512 levels or a
/// compound over the 2 MiB budget of the reference client.
pub(crate) fn read_optional_nbt(reader: &mut impl Read) -> Result<Option<Vec<u8>>, ProtocolError> {
    let mut recorder = RecordingReader::new(reader);
    let tag = read_u8(&mut recorder)?;
    if tag == TAG_END {
        return Ok(None);
    }
    if tag != TAG_COMPOUND {
        return Err(ProtocolError::Invariant(format!(
            "NBT starts with tag type {tag} instead of a compound"
        )));
    }
    let mut skipper = Skipper {
        reader: &mut recorder,
        budget: SizeBudget::default(),
    };
    skipper.skip_string()?;
    skipper.skip_payload(TAG_COMPOUND, 0)?;
    Ok(Some(recorder.into_recorded()))
}

/// The reference client's NBT size accounting (MCP `NBTSizeTracker.read`).
///
/// Each call charges `bits / 8` units; the read fails once the total passes
/// [`MAX_NBT_BUDGET`].
#[derive(Debug, Default)]
struct SizeBudget {
    used: u64,
}

impl SizeBudget {
    /// Charges a cost given in bits, like `NBTSizeTracker.read(long bits)`.
    fn charge(&mut self, bits: u64) -> Result<(), ProtocolError> {
        self.used = self.used.saturating_add(bits / 8);
        if self.used > MAX_NBT_BUDGET {
            return Err(ProtocolError::Invariant(format!(
                "NBT compound over the {MAX_NBT_BUDGET}-unit size budget"
            )));
        }
        Ok(())
    }
}

/// Walks an NBT payload without keeping it, charging the size budget.
struct Skipper<'r, R> {
    reader: &'r mut R,
    budget: SizeBudget,
}

impl<R: Read> Skipper<'_, R> {
    /// Skips a modified-UTF-8 string (a 16-bit length plus that many bytes)
    /// and returns its byte length.
    ///
    /// The reference client charges per UTF-16 unit of the decoded string;
    /// the byte length is never smaller, so the budget is at least as strict.
    fn skip_string(&mut self) -> Result<u64, ProtocolError> {
        let len = read_i16(self.reader)?;
        let len = u16::try_from(len).map_err(|_| {
            ProtocolError::Invariant(format!("negative NBT string length {len}"))
        })?;
        skip_bytes(self.reader, usize::from(len))?;
        Ok(u64::from(len))
    }

    /// Reads a signed 32-bit length prefix, rejecting negative values.
    fn read_len(&mut self) -> Result<u64, ProtocolError> {
        let len = read_i32(self.reader)?;
        u64::try_from(len)
            .map_err(|_| ProtocolError::Invariant(format!("negative NBT length {len}")))
    }

    /// Skips `len` elements of `width` bytes each; the budget was charged
    /// before, so the product is small.
    fn skip_elements(&mut self, len: u64, width: u64) -> Result<(), ProtocolError> {
        let bytes = usize::try_from(len * width)
            .map_err(|_| ProtocolError::Invariant("NBT array too long".to_string()))?;
        skip_bytes(self.reader, bytes)
    }

    /// Skips the payload of a tag of the given type; the type byte and the
    /// entry name were already consumed. The costs are those of each MCP
    /// `NBTTag*.read`.
    fn skip_payload(&mut self, tag_type: u8, depth: u32) -> Result<(), ProtocolError> {
        match tag_type {
            0 => self.budget.charge(64),
            1 => self.fixed(72, 1),
            2 => self.fixed(80, 2),
            3 => self.fixed(96, 4),
            4 => self.fixed(128, 8),
            5 => self.fixed(96, 4),
            6 => self.fixed(128, 8),
            7 => self.array(8),
            8 => {
                self.budget.charge(288)?;
                let len = self.skip_string()?;
                self.budget.charge(16 * len)
            }
            9 => self.list(depth),
            10 => self.compound(depth),
            11 => self.array(32),
            other => Err(ProtocolError::Invariant(format!(
                "unknown NBT tag type {other}"
            ))),
        }
    }

    /// A fixed-width scalar tag.
    fn fixed(&mut self, bits: u64, width: usize) -> Result<(), ProtocolError> {
        self.budget.charge(bits)?;
        skip_bytes(self.reader, width)
    }

    /// `TAG_Byte_Array` (8 bits per element) or `TAG_Int_Array` (32).
    fn array(&mut self, element_bits: u64) -> Result<(), ProtocolError> {
        self.budget.charge(192)?;
        let len = self.read_len()?;
        self.budget.charge(element_bits.saturating_mul(len))?;
        self.skip_elements(len, element_bits / 8)
    }

    /// `TAG_List`: element type, count, then the elements.
    fn list(&mut self, depth: u32) -> Result<(), ProtocolError> {
        self.budget.charge(296)?;
        check_depth(depth)?;
        let element_type = read_u8(self.reader)?;
        let count = self.read_len()?;
        if element_type == TAG_END && count > 0 {
            // MCP `NBTTagList.read`: "Missing type on ListTag".
            return Err(ProtocolError::Invariant(
                "NBT list of TAG_End elements".to_string(),
            ));
        }
        self.budget.charge(32u64.saturating_mul(count))?;
        for _ in 0..count {
            self.skip_payload(element_type, depth + 1)?;
        }
        Ok(())
    }

    /// `TAG_Compound`: entries until the terminating `TAG_End`.
    fn compound(&mut self, depth: u32) -> Result<(), ProtocolError> {
        self.budget.charge(384)?;
        check_depth(depth)?;
        loop {
            let tag = read_u8(self.reader)?;
            if tag == TAG_END {
                return Ok(());
            }
            let name_len = self.skip_string()?;
            self.budget.charge(224 + 16 * name_len)?;
            self.skip_payload(tag, depth + 1)?;
        }
    }
}

/// Rejects nesting deeper than the reference client allows.
fn check_depth(depth: u32) -> Result<(), ProtocolError> {
    if depth > MAX_NBT_DEPTH {
        return Err(ProtocolError::Invariant(format!(
            "NBT nested deeper than {MAX_NBT_DEPTH} levels"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A root compound with an empty name around `entries`.
    fn compound(entries: &[u8]) -> Vec<u8> {
        let mut bytes = vec![TAG_COMPOUND, 0, 0];
        bytes.extend_from_slice(entries);
        bytes.push(TAG_END);
        bytes
    }

    #[test]
    fn a_lone_end_tag_is_no_compound() {
        assert_eq!(read_optional_nbt(&mut &[0u8][..]).unwrap(), None);
    }

    #[test]
    fn a_compound_is_captured_verbatim() {
        // {"a": 7b, "s": "hi"}
        let bytes = compound(&[1, 0, 1, b'a', 7, 8, 0, 1, b's', 0, 2, b'h', b'i']);
        let mut reader = &bytes[..];
        assert_eq!(read_optional_nbt(&mut reader).unwrap(), Some(bytes.clone()));
        assert!(reader.is_empty());
    }

    #[test]
    fn a_list_of_end_tags_is_rejected_at_once() {
        // Regression (fuzzing): a list of 2^31 - 1 `TAG_End` elements
        // consumes no input per element and used to loop for minutes.
        let bytes = compound(&[9, 0, 1, b'l', TAG_END, 0x7F, 0xFF, 0xFF, 0xFF]);
        assert!(read_optional_nbt(&mut &bytes[..]).is_err());
    }

    #[test]
    fn a_huge_list_count_exhausts_the_budget_before_any_element() {
        // A list announcing 2^31 - 1 empty compounds costs 32 bits each:
        // over budget without reading one of them.
        let bytes = compound(&[9, 0, 1, b'l', TAG_COMPOUND, 0x7F, 0xFF, 0xFF, 0xFF]);
        let err = read_optional_nbt(&mut &bytes[..]).unwrap_err();
        assert!(err.to_string().contains("budget"), "{err}");
    }

    #[test]
    fn nested_lists_beyond_the_reference_depth_are_rejected() {
        let mut entries = vec![9, 0, 1, b'l'];
        for _ in 0..=MAX_NBT_DEPTH {
            entries.extend_from_slice(&[9, 0, 0, 0, 1]); // a list of one list
        }
        entries.extend_from_slice(&[TAG_END, 0, 0, 0, 0]);
        let bytes = compound(&entries);
        let err = read_optional_nbt(&mut &bytes[..]).unwrap_err();
        assert!(err.to_string().contains("deeper"), "{err}");
    }

    #[test]
    fn negative_lengths_and_unknown_tags_are_errors() {
        let negative = compound(&[7, 0, 1, b'b', 0xFF, 0xFF, 0xFF, 0xFF]);
        assert!(read_optional_nbt(&mut &negative[..]).is_err());
        let unknown = compound(&[12, 0, 1, b'x']);
        assert!(read_optional_nbt(&mut &unknown[..]).is_err());
        assert!(read_optional_nbt(&mut &[TAG_COMPOUND, 0][..]).is_err());
    }
}

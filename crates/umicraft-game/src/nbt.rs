// SPDX-License-Identifier: MIT OR Apache-2.0

//! A bounds-checked NBT tree parser for the data the server sends.
//!
//! Item stacks, skulls and block entities carry their NBT as raw bytes (see
//! `umicraft_protocol::packets::play::ItemStack`). The protocol crate has
//! already validated the framing, but the bytes still come from the network,
//! so every read here is bounds-checked, the nesting is limited and nothing
//! panics: a malformed compound is an [`NbtError`], never a crash.
//!
//! Layout (MCP `CompressedStreamTools`, `NBTTagCompound.read`): a root tag
//! type, a name, then the payload; strings are a big-endian `u16` length and
//! UTF-8 bytes (Java modified UTF-8 differs only for NUL and supplementary
//! characters, which are decoded lossily).

use thiserror::Error;

/// The deepest nesting that is parsed, well under the 512 of the reference
/// so a hostile compound cannot exhaust the stack.
const MAX_DEPTH: u32 = 32;

/// Why a compound could not be parsed.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum NbtError {
    /// The data ended in the middle of a tag.
    #[error("NBT data is truncated")]
    Truncated,
    /// A tag type outside `1..=11`.
    #[error("unknown NBT tag type {0}")]
    UnknownTag(u8),
    /// A negative array or list length.
    #[error("negative NBT length")]
    NegativeLength,
    /// The nesting exceeded the supported depth.
    #[error("NBT nesting is too deep")]
    TooDeep,
    /// The root tag is not a compound.
    #[error("the NBT root is not a compound")]
    RootNotCompound,
}

/// One NBT value (MCP `NBTBase` and its subclasses).
#[derive(Debug, Clone, PartialEq)]
pub enum Nbt {
    /// `TAG_Byte`.
    Byte(i8),
    /// `TAG_Short`.
    Short(i16),
    /// `TAG_Int`.
    Int(i32),
    /// `TAG_Long`.
    Long(i64),
    /// `TAG_Float`.
    Float(f32),
    /// `TAG_Double`.
    Double(f64),
    /// `TAG_Byte_Array`.
    ByteArray(Vec<u8>),
    /// `TAG_String`.
    String(String),
    /// `TAG_List`.
    List(Vec<Nbt>),
    /// `TAG_Compound`, entries in wire order.
    Compound(Vec<(String, Nbt)>),
    /// `TAG_Int_Array`.
    IntArray(Vec<i32>),
}

impl Nbt {
    /// Parses a root compound (tag type, name, payload) and returns its value.
    ///
    /// # Errors
    /// [`NbtError`] when the bytes are truncated, malformed, too deeply
    /// nested or do not start with a compound.
    pub fn parse(data: &[u8]) -> Result<Self, NbtError> {
        let mut cursor = Cursor { data, pos: 0 };
        if cursor.byte()? != TAG_COMPOUND {
            return Err(NbtError::RootNotCompound);
        }
        cursor.string()?;
        cursor.payload(TAG_COMPOUND, 0)
    }

    /// The entry `key` of a compound, `None` for another tag or a missing key.
    pub fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Compound(entries) => entries
                .iter()
                .find_map(|(name, value)| (name == key).then_some(value)),
            _ => None,
        }
    }

    /// The text of a `TAG_String`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }

    /// The elements of a `TAG_List`.
    pub fn as_list(&self) -> Option<&[Self]> {
        match self {
            Self::List(items) => Some(items),
            _ => None,
        }
    }

    /// The value of any integer tag (byte, short or int), widened.
    pub fn as_int(&self) -> Option<i32> {
        match *self {
            Self::Byte(value) => Some(i32::from(value)),
            Self::Short(value) => Some(i32::from(value)),
            Self::Int(value) => Some(value),
            _ => None,
        }
    }
}

const TAG_END: u8 = 0;
const TAG_COMPOUND: u8 = 10;

/// A bounds-checked cursor over NBT bytes.
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn take(&mut self, count: usize) -> Result<&[u8], NbtError> {
        let end = self.pos.checked_add(count).ok_or(NbtError::Truncated)?;
        let bytes = self.data.get(self.pos..end).ok_or(NbtError::Truncated)?;
        self.pos = end;
        Ok(bytes)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], NbtError> {
        let mut out = [0; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    fn byte(&mut self) -> Result<u8, NbtError> {
        Ok(self.array::<1>()?[0])
    }

    fn length(&mut self) -> Result<usize, NbtError> {
        let value = i32::from_be_bytes(self.array()?);
        usize::try_from(value).map_err(|_| NbtError::NegativeLength)
    }

    fn string(&mut self) -> Result<String, NbtError> {
        let length = usize::from(u16::from_be_bytes(self.array()?));
        Ok(String::from_utf8_lossy(self.take(length)?).into_owned())
    }

    /// Reads the payload of a tag of type `tag`.
    fn payload(&mut self, tag: u8, depth: u32) -> Result<Nbt, NbtError> {
        if depth > MAX_DEPTH {
            return Err(NbtError::TooDeep);
        }
        Ok(match tag {
            1 => Nbt::Byte(i8::from_be_bytes(self.array()?)),
            2 => Nbt::Short(i16::from_be_bytes(self.array()?)),
            3 => Nbt::Int(i32::from_be_bytes(self.array()?)),
            4 => Nbt::Long(i64::from_be_bytes(self.array()?)),
            5 => Nbt::Float(f32::from_be_bytes(self.array()?)),
            6 => Nbt::Double(f64::from_be_bytes(self.array()?)),
            7 => {
                let length = self.length()?;
                Nbt::ByteArray(self.take(length)?.to_vec())
            }
            8 => Nbt::String(self.string()?),
            9 => self.list(depth)?,
            10 => self.compound(depth)?,
            11 => self.int_array()?,
            other => return Err(NbtError::UnknownTag(other)),
        })
    }

    fn list(&mut self, depth: u32) -> Result<Nbt, NbtError> {
        let item_tag = self.byte()?;
        let length = self.length()?;
        // Reserve nothing up front: the length is untrusted.
        let mut items = Vec::new();
        for _ in 0..length {
            items.push(self.payload(item_tag, depth + 1)?);
        }
        Ok(Nbt::List(items))
    }

    fn compound(&mut self, depth: u32) -> Result<Nbt, NbtError> {
        let mut entries = Vec::new();
        loop {
            let tag = self.byte()?;
            if tag == TAG_END {
                return Ok(Nbt::Compound(entries));
            }
            let name = self.string()?;
            entries.push((name, self.payload(tag, depth + 1)?));
        }
    }

    fn int_array(&mut self) -> Result<Nbt, NbtError> {
        let length = self.length()?;
        let bytes = self.take(length.checked_mul(4).ok_or(NbtError::Truncated)?)?;
        let values = bytes
            .chunks_exact(4)
            .map(|chunk| i32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        Ok(Nbt::IntArray(values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `{display: {Name: "Hi", color: 255}}` as a server would send it.
    fn display_compound() -> Vec<u8> {
        let mut nbt = vec![10, 0, 0, 10, 0, 7];
        nbt.extend(b"display");
        nbt.extend([8, 0, 4]);
        nbt.extend(b"Name");
        nbt.extend([0, 2]);
        nbt.extend(b"Hi");
        nbt.extend([3, 0, 5]);
        nbt.extend(b"color");
        nbt.extend(255_i32.to_be_bytes());
        nbt.extend([0, 0]);
        nbt
    }

    #[test]
    fn parses_nested_compounds_and_reads_values() {
        let nbt = Nbt::parse(&display_compound()).expect("valid");
        let display = nbt.get("display").expect("display");
        assert_eq!(display.get("Name").and_then(Nbt::as_str), Some("Hi"));
        assert_eq!(display.get("color").and_then(Nbt::as_int), Some(255));
        assert!(display.get("missing").is_none());
    }

    #[test]
    fn every_truncation_is_an_error_not_a_panic() {
        let bytes = display_compound();
        for end in 0..bytes.len() {
            assert!(Nbt::parse(&bytes[..end]).is_err(), "prefix {end}");
        }
    }

    #[test]
    fn rejects_a_root_that_is_not_a_compound() {
        assert_eq!(Nbt::parse(&[8, 0, 0]), Err(NbtError::RootNotCompound));
    }

    #[test]
    fn rejects_unknown_tags_and_negative_lengths() {
        assert_eq!(
            Nbt::parse(&[10, 0, 0, 99, 0, 0]),
            Err(NbtError::UnknownTag(99))
        );
        let negative = [10, 0, 0, 9, 0, 0, 3, 0xFF, 0xFF, 0xFF, 0xFF];
        assert_eq!(Nbt::parse(&negative), Err(NbtError::NegativeLength));
    }

    #[test]
    fn rejects_deep_nesting() {
        let mut bytes = vec![10, 0, 0];
        for _ in 0..64 {
            bytes.extend([10, 0, 0]);
        }
        assert_eq!(Nbt::parse(&bytes), Err(NbtError::TooDeep));
    }

    #[test]
    fn a_huge_declared_list_fails_without_allocating_it() {
        let huge = [10, 0, 0, 9, 0, 0, 1, 0x7F, 0xFF, 0xFF, 0xFF];
        assert_eq!(Nbt::parse(&huge), Err(NbtError::Truncated));
    }
}

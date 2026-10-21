// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fixed-width packet field primitives shared by every state's packet codecs.
//!
//! These readers complement [`crate::varint`] (VarInt coding) and
//! [`crate::types`] (VarInt-length-prefixed strings and byte arrays): anything
//! that is plain big-endian binary or a single flag byte lands here. All reads
//! are total over hostile input — truncation surfaces as
//! [`ProtocolError::Io`], never a panic.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::varint::{read_var_i32, write_var_u32};

/// Reads a boolean byte (any non-zero value is true).
///
/// # Errors
/// [`ProtocolError::Io`] on stream truncation.
pub fn read_bool<R: Read>(reader: &mut R) -> Result<bool, ProtocolError> {
    let mut byte = [0u8; 1];
    reader.read_exact(&mut byte)?;
    Ok(byte[0] != 0)
}

/// Reads one byte.
///
/// # Errors
/// [`ProtocolError::Io`] on stream truncation.
pub fn read_u8<R: Read>(reader: &mut R) -> Result<u8, ProtocolError> {
    let mut byte = [0u8; 1];
    reader.read_exact(&mut byte)?;
    Ok(byte[0])
}

/// Reads a big-endian i64.
///
/// # Errors
/// [`ProtocolError::Io`] on stream truncation.
pub fn read_i64<R: Read>(reader: &mut R) -> Result<i64, ProtocolError> {
    let mut bytes = [0u8; 8];
    reader.read_exact(&mut bytes)?;
    Ok(i64::from_be_bytes(bytes))
}

/// Reads a big-endian i32.
///
/// # Errors
/// [`ProtocolError::Io`] on stream truncation.
pub fn read_i32<R: Read>(reader: &mut R) -> Result<i32, ProtocolError> {
    let mut bytes = [0u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(i32::from_be_bytes(bytes))
}

/// Reads every remaining byte of the body (used by 1.8 `Plugin Message`,
/// whose payload is "everything after the channel string").
///
/// # Errors
/// [`ProtocolError::Io`] on stream failure.
pub fn read_rest<R: Read>(reader: &mut R) -> Result<Vec<u8>, ProtocolError> {
    let mut rest = Vec::new();
    reader.read_to_end(&mut rest)?;
    Ok(rest)
}

/// Reads a big-endian f64.
///
/// # Errors
/// [`ProtocolError::Io`] on stream truncation.
pub fn read_f64<R: Read>(reader: &mut R) -> Result<f64, ProtocolError> {
    let mut bytes = [0u8; 8];
    reader.read_exact(&mut bytes)?;
    Ok(f64::from_be_bytes(bytes))
}

/// Reads a big-endian f32.
///
/// # Errors
/// [`ProtocolError::Io`] on stream truncation.
pub fn read_f32<R: Read>(reader: &mut R) -> Result<f32, ProtocolError> {
    let mut bytes = [0u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(f32::from_be_bytes(bytes))
}

/// Reads a byte array that is itself VarInt-length-prefixed.
///
/// # Errors
/// [`ProtocolError::Invariant`] when the declared length is implausible,
/// [`ProtocolError::Io`] on truncation.
pub fn read_byte_array<R: Read>(reader: &mut R, max: usize) -> Result<Vec<u8>, ProtocolError> {
    let len = read_var_i32(reader)?;
    if len < 0 {
        return Err(ProtocolError::Invariant(format!(
            "negative array length {len}"
        )));
    }
    let len = len as usize;
    if len > max {
        return Err(ProtocolError::Invariant(format!(
            "array length {len} exceeds budget {max}"
        )));
    }
    let mut data = vec![0u8; len];
    reader.read_exact(&mut data)?;
    Ok(data)
}

/// Writes a byte array with a VarInt length prefix (the 1.8 `writeByteArray`).
///
/// # Errors
/// [`ProtocolError`] on stream failure.
pub fn write_byte_array<W: Write>(writer: &mut W, data: &[u8]) -> Result<(), ProtocolError> {
    write_var_u32(writer, data.len() as u32)?;
    writer.write_all(data)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_array_round_trip() {
        let mut buffer = Vec::new();
        write_byte_array(&mut buffer, &[1, 2, 3, 4]).unwrap();
        assert_eq!(buffer, [4, 1, 2, 3, 4]);
        assert_eq!(read_byte_array(&mut &buffer[..], 16).unwrap(), [1, 2, 3, 4]);
    }

    #[test]
    fn byte_array_budget_is_enforced() {
        let mut buffer = Vec::new();
        write_byte_array(&mut buffer, &[0; 8]).unwrap();
        let err = read_byte_array(&mut &buffer[..], 4).unwrap_err();
        assert!(matches!(err, ProtocolError::Invariant(_)));
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! VarInt / VarLong: Minecraft's variable-length integer encoding.
//!
//! Integers are split into 7-bit groups, least-significant first; the highest
//! bit of each byte records whether more groups follow. A 32-bit VarInt needs
//! at most 5 bytes.
//!
//! Source: community protocol documentation for 1.8 ("varint and_varlong"
//! in the pre-1.13 wiki.vg protocol pages), also validated against captured
//! 1.8 traffic once `umicraft-testkit` capture lands.

use std::io::{Read, Write};

use crate::error::ProtocolError;

/// Maximum number of bytes a 32-bit VarInt occupies on the wire.
pub const VAR_INT_32_MAX: usize = 5;

/// Reads a 32-bit VarInt.
///
/// # Errors
/// [`ProtocolError::VarIntTooLong`] when the encoding does not terminate within
/// 5 bytes (or its 5th byte carries bits beyond 32 bits), [`ProtocolError::Io`]
/// when the stream ends mid-VarInt.
pub fn read_var_u32<R: Read>(reader: &mut R) -> Result<u32, ProtocolError> {
    let mut result = 0u32;
    for shift in 0..VAR_INT_32_MAX {
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte)?;
        let payload = u32::from(byte[0] & 0x7F);
        if shift == VAR_INT_32_MAX - 1 && payload > 0x0F {
            // The 5th byte may only contribute bits 31..28; a larger payload
            // means the sender encoded a value wider than 32 bits.
            return Err(ProtocolError::VarIntTooLong);
        }
        result |= payload << (shift * 7);
        if byte[0] & 0x80 == 0 {
            return Ok(result);
        }
    }
    Err(ProtocolError::VarIntTooLong)
}

/// Reads a 32-bit VarInt and reinterprets its bits as a signed value
/// (two's complement, matching the protocol's integer types).
///
/// # Errors
/// Same as [`read_var_u32`].
pub fn read_var_i32<R: Read>(reader: &mut R) -> Result<i32, ProtocolError> {
    Ok(read_var_u32(reader)? as i32)
}

/// Writes a 32-bit VarInt.
///
/// # Errors
/// [`ProtocolError::Io`] when the underlying writer rejects the bytes.
pub fn write_var_u32<W: Write>(writer: &mut W, mut value: u32) -> Result<(), ProtocolError> {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
            writer.write_all(&[byte])?;
        } else {
            writer.write_all(&[byte])?;
            return Ok(());
        }
    }
}

/// Encodes a 32-bit VarInt into a byte vector (non-streaming helper).
#[must_use]
pub fn var_u32_to_vec(value: u32) -> Vec<u8> {
    let mut buffer = Vec::with_capacity(VAR_INT_32_MAX);
    // Infallible on `Vec`; the writer path can only fail on real I/O.
    write_var_u32(&mut buffer, value).expect("Vec write cannot fail");
    buffer
}

/// Returns the exact on-wire length of `value` as a VarInt.
#[must_use]
pub fn var_u32_len(value: u32) -> usize {
    let mut len = 1;
    let mut value = value >> 7;
    while value != 0 {
        len += 1;
        value >>= 7;
    }
    len
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Known round-trips: 0, small values, 7-bit boundary, 14-bit boundary,
    /// sign-extension sentinels, and a maximum-width encoding.
    #[test]
    fn known_encodings() {
        assert_eq!(var_u32_to_vec(0), [0x00]);
        assert_eq!(var_u32_to_vec(1), [0x01]);
        assert_eq!(var_u32_to_vec(127), [0x7F]);
        assert_eq!(var_u32_to_vec(128), [0x80, 0x01]);
        assert_eq!(var_u32_to_vec(16_383), [0xFF, 0x7F]);
        assert_eq!(var_u32_to_vec(16_384), [0x80, 0x80, 0x01]);
        assert_eq!(var_u32_to_vec(u32::MAX), [0xFF, 0xFF, 0xFF, 0xFF, 0x0F]);
        assert_eq!(var_u32_len(16_383), 2);
        assert_eq!(var_u32_len(u32::MAX), 5);
    }

    #[test]
    fn round_trips_every_bit_pattern() {
        for value in [0u32, 1, 0x7F, 0x80, 2_097_151, u32::MAX / 3, u32::MAX] {
            let encoded = var_u32_to_vec(value);
            let mut slice = &encoded[..];
            assert_eq!(read_var_u32(&mut slice).unwrap(), value);
            assert!(slice.is_empty());
        }
    }

    #[test]
    fn negative_reuses_two_complement_bits() {
        // -1 is stored as the u32 bit pattern 0xFFFF_FFFF (5 bytes).
        let mut slice = &var_u32_to_vec(u32::MAX)[..];
        assert_eq!(read_var_i32(&mut slice).unwrap(), -1);
    }

    #[test]
    fn over_wide_varints_fail_without_panicking() {
        // Six 0x80 bytes is the classic overflow attempt.
        let overflow = [0x80, 0x80, 0x80, 0x80, 0x80, 0x00];
        let mut slice = &overflow[..];
        let err = read_var_u32(&mut slice).unwrap_err();
        assert!(matches!(err, ProtocolError::VarIntTooLong));

        // A 5th byte carrying bits above 32 is also invalid.
        let wide = [0x80, 0x80, 0x80, 0x80, 0x10];
        assert!(read_var_u32(&mut &wide[..]).is_err());

        // Truncated streams are io errors, not panics.
        assert!(read_var_u32(&mut &[0x80, 0x80][..]).is_err());
    }
}

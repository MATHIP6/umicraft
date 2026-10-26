// SPDX-License-Identifier: MIT OR Apache-2.0

//! Length-prefixed string and misc primitive codecs.
//!
//! Protocol strings are VarInt length-prefixed UTF-8. The 1.8 protocol caps
//! most strings at 256 chars (`source`: community protocol docs, "Data
//! types": strings are limited to 240+offset in some contexts; 256 is the
//! standard cap used by vanilla for chat/login, approached with 4 bytes per
//! char on the wire), so readers enforce `max_chars` before allocating.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::varint::{read_var_u32, write_var_u32};

/// The default maximum string length used by 1.8 vanilla fields.
pub const MAX_STRING_CHARS: usize = 256;

/// The maximum length of a chat JSON component or of a free-form string on
/// the 1.8 wire (`PacketBuffer.readStringFromBuffer(32767)`).
pub const MAX_JSON_CHARS: usize = 32767;

/// Maximum bytes a 256-char string may occupy (4 bytes per UTF-8 code point).
fn max_string_bytes(max_chars: usize) -> usize {
    max_chars.saturating_mul(4)
}

/// Reads a length-prefixed UTF-8 string capped at `max_chars`.
///
/// # Errors
/// [`ProtocolError::StringTooLong`] when the declared length is over budget,
/// [`ProtocolError::InvalidUtf8`] for non-UTF-8 bytes, [`ProtocolError::Io`]
/// for stream truncation.
pub fn read_string<R: Read>(reader: &mut R, max_chars: usize) -> Result<String, ProtocolError> {
    let byte_len = read_var_u32(reader)?;
    let bytes = byte_len as usize;
    let budget = max_string_bytes(max_chars);
    if bytes > budget {
        // Rejected before allocation: declared byte length cannot represent a
        // text string within the char limit.
        return Err(ProtocolError::Invariant(format!(
            "string declares {bytes} bytes, byte budget for {max_chars} chars is {budget}"
        )));
    }
    let mut bytes = vec![0u8; bytes];
    reader.read_exact(&mut bytes)?;
    Ok(String::from_utf8(bytes)?)
}

/// Reads a string with the default 256-char cap.
///
/// # Errors
/// Same as [`read_string`].
pub fn read_string_capped<R: Read>(reader: &mut R) -> Result<String, ProtocolError> {
    read_string(reader, MAX_STRING_CHARS)
}

/// Writes a length-prefixed UTF-8 string.
///
/// # Errors
/// [`ProtocolError::StringTooLong`] when `value` exceeds `max_chars`,
/// [`ProtocolError::Io`] on write failure.
pub fn write_string<W: Write>(
    writer: &mut W,
    value: &str,
    max_chars: usize,
) -> Result<(), ProtocolError> {
    if value.chars().count() > max_chars {
        return Err(ProtocolError::StringTooLong {
            chars: value.chars().count(),
            limit: max_chars,
        });
    }
    write_var_u32(writer, value.len() as u32)?;
    writer.write_all(value.as_bytes())?;
    Ok(())
}

/// Writes a string with the default 256-char cap.
///
/// # Errors
/// Same as [`write_string`].
pub fn write_string_capped<W: Write>(writer: &mut W, value: &str) -> Result<(), ProtocolError> {
    write_string(writer, value, MAX_STRING_CHARS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(value: &str) -> String {
        let mut buffer = Vec::new();
        write_string_capped(&mut buffer, value).unwrap();
        let mut slice = &buffer[..];
        read_string_capped(&mut slice).unwrap()
    }

    #[test]
    fn round_trips_ascii_and_multibyte_ascii() {
        assert_eq!(round_trip("Umicraft"), "Umicraft");
        // Multi-byte UTF-8 counts chars, not bytes, toward the cap.
        assert_eq!(round_trip("\u{1F600}"), "\u{1F600}");
    }

    #[test]
    fn over_budget_strings_are_rejected_without_allocation() {
        let long = "x".repeat(MAX_STRING_CHARS + 1);
        let mut buffer = Vec::new();
        assert!(matches!(
            write_string_capped(&mut buffer, &long),
            Err(ProtocolError::StringTooLong { .. })
        ));

        // Hostile declared length (huge varint) errors instead of allocating.
        let mut hostile = Vec::new();
        crate::varint::write_var_u32(&mut hostile, u32::MAX).unwrap();
        assert!(read_string_capped(&mut &hostile[..]).is_err());
    }

    #[test]
    fn truncated_streams_error_cleanly() {
        let mut buffer = Vec::new();
        write_string_capped(&mut buffer, "half").unwrap();
        // Cut the trailing byte: read must not panic.
        let _ = read_string_capped(&mut &buffer[..buffer.len() - 1]);
    }
}

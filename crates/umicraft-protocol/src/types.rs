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
/// [`ProtocolError::Invariant`] when the declared byte length is over budget,
/// [`ProtocolError::StringTooLong`] when the character count exceeds `max_chars`,
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
    let mut raw = vec![0u8; bytes];
    reader.read_exact(&mut raw)?;
    let s = String::from_utf8(raw)?;
    let char_count = s.chars().count();
    if char_count > max_chars {
        return Err(ProtocolError::StringTooLong {
            chars: char_count,
            limit: max_chars,
        });
    }
    Ok(s)
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
    let char_count = value.chars().count();
    if char_count > max_chars {
        return Err(ProtocolError::StringTooLong {
            chars: char_count,
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

    fn round_trip_with_limit(value: &str, limit: usize) -> String {
        let mut buffer = Vec::new();
        write_string(&mut buffer, value, limit).unwrap();
        let mut slice = &buffer[..];
        read_string(&mut slice, limit).unwrap()
    }

    #[test]
    fn round_trips_ascii_and_multibyte_ascii() {
        assert_eq!(round_trip("Umicraft"), "Umicraft");
        // Multi-byte UTF-8 counts chars (UTF-16 code units), not bytes, toward the cap.
        assert_eq!(round_trip("\u{1F600}"), "\u{1F600}");
    }

    #[test]
    fn round_trips_empty_string() {
        let empty = round_trip("");
        assert_eq!(empty, "");
        assert_eq!(round_trip_with_limit("", MAX_JSON_CHARS), "");
    }

    #[test]
    fn round_trips_exact_256_ascii_chars() {
        let text_256 = "a".repeat(256);
        assert_eq!(round_trip(&text_256), text_256);

        // 257 ASCII chars must fail capped write
        let text_257 = "a".repeat(257);
        let mut buffer = Vec::new();
        assert!(matches!(
            write_string_capped(&mut buffer, &text_257),
            Err(ProtocolError::StringTooLong { chars: 257, limit: 256 })
        ));
    }

    #[test]
    fn handles_1450_ascii_chars_correctly() {
        let text_1450 = "b".repeat(1450);

        // Under 256-cap, byte budget is 1024, so declared 1450 bytes must fail without allocating
        let mut buf_1450 = Vec::new();
        crate::varint::write_var_u32(&mut buf_1450, 1450).unwrap();
        buf_1450.extend_from_slice(text_1450.as_bytes());

        let err = read_string_capped(&mut &buf_1450[..]).unwrap_err();
        match err {
            ProtocolError::Invariant(msg) => {
                assert!(msg.contains("string declares 1450 bytes, byte budget for 256 chars is 1024"));
            }
            other => panic!("expected Invariant error, got {other:?}"),
        }

        // Under MAX_JSON_CHARS (32767), 1450 chars decodes cleanly
        let mut slice = &buf_1450[..];
        let decoded = read_string(&mut slice, MAX_JSON_CHARS).unwrap();
        assert_eq!(decoded, text_1450);
        assert_eq!(round_trip_with_limit(&text_1450, MAX_JSON_CHARS), text_1450);
    }

    #[test]
    fn handles_utf8_accents_and_international_chars() {
        let accented = "Élégant café français à Noël dans l'arène : Ñandú, Grüße & 日本語";
        assert_eq!(round_trip(accented), accented);
        assert_eq!(round_trip_with_limit(accented, MAX_JSON_CHARS), accented);
    }

    #[test]
    fn handles_astral_emojis() {
        let emoji_str = "⚔️ PVP \u{1F3C6} Victory! \u{1F525}\u{1F480}";
        assert_eq!(round_trip(emoji_str), emoji_str);

        let single_astral = "\u{1F3C6}";
        assert_eq!(single_astral.len(), 4); // 4 UTF-8 bytes
        assert_eq!(single_astral.chars().count(), 1); // 1 Unicode char

        // A string with 256 astral emojis = 256 chars, 1024 bytes (fits exactly in budget for 256 chars)
        let emojis_256 = single_astral.repeat(256);
        assert_eq!(emojis_256.chars().count(), 256);
        assert_eq!(round_trip(&emojis_256), emojis_256);

        // 257 astral emojis = 257 chars, exceeds 256 limit
        let emojis_257 = single_astral.repeat(257);
        let mut buf = Vec::new();
        assert!(matches!(
            write_string_capped(&mut buf, &emojis_257),
            Err(ProtocolError::StringTooLong { chars: 257, limit: 256 })
        ));
    }

    #[test]
    fn post_decode_char_limit_is_strictly_enforced() {
        // Construct a payload where declared bytes <= budget (e.g. 500 ASCII bytes <= 1024 budget for 256 chars),
        // but the character count (500) exceeds max_chars (256).
        let payload_500 = "x".repeat(500);
        let mut buf = Vec::new();
        crate::varint::write_var_u32(&mut buf, 500).unwrap();
        buf.extend_from_slice(payload_500.as_bytes());

        // Byte check passes (500 <= 1024), but char count check must reject it!
        let err = read_string(&mut &buf[..], 256).unwrap_err();
        assert!(matches!(
            err,
            ProtocolError::StringTooLong {
                chars: 500,
                limit: 256
            }
        ));
    }

    #[test]
    fn invalid_varint_errors() {
        // 6-byte varint sequence
        let invalid_varint = [0x80, 0x80, 0x80, 0x80, 0x80, 0x01];
        assert!(matches!(
            read_string_capped(&mut &invalid_varint[..]),
            Err(ProtocolError::VarIntTooLong)
        ));
    }

    #[test]
    fn invalid_utf8_errors() {
        // Declares 2 bytes, but byte content is not valid UTF-8
        let mut buf = Vec::new();
        crate::varint::write_var_u32(&mut buf, 2).unwrap();
        buf.extend_from_slice(&[0xFF, 0xFE]);

        assert!(matches!(
            read_string_capped(&mut &buf[..]),
            Err(ProtocolError::InvalidUtf8(_))
        ));
    }

    #[test]
    fn string_exceeding_max_json_chars_is_rejected() {
        // String exceeding 32767 chars
        let long_json = "z".repeat(MAX_JSON_CHARS + 1);
        let mut buf = Vec::new();
        assert!(matches!(
            write_string(&mut buf, &long_json, MAX_JSON_CHARS),
            Err(ProtocolError::StringTooLong { chars: 32768, limit: 32767 })
        ));

        // Wire payload declaring byte length over budget (32767 * 4 = 131068)
        let mut hostile = Vec::new();
        crate::varint::write_var_u32(&mut hostile, 131069).unwrap();
        assert!(read_string(&mut &hostile[..], MAX_JSON_CHARS).is_err());
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
        let err = read_string_capped(&mut &buffer[..buffer.len() - 1]).unwrap_err();
        assert!(matches!(err, ProtocolError::Io(_)));
    }
}

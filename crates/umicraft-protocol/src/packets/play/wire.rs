// SPDX-License-Identifier: MIT OR Apache-2.0

//! Wire helpers shared by the 1.8 play-state packets.
//!
//! [`crate::packets::codec`] already carries the fixed-width primitives the
//! login and world packets need (`u8`, `i32`, `i64`, `f32`, `f64`, `bool`).
//! The play packets added for milestone M3 additionally need signed 8- and
//! 16-bit fields (hotbar slots, entity deltas, the angle byte) and a way to
//! capture a variable-length blob whose size is only known after parsing it
//! (an item stack, a data-watcher list). Those helpers live here.
//!
//! All reads are total over hostile input: a truncated or malformed field
//! surfaces as [`ProtocolError`], never a panic.

use std::io::{Read, Write};

use crate::error::ProtocolError;

/// Reads a signed 8-bit field (a Java `byte`).
///
/// # Errors
/// [`ProtocolError::Io`] on stream truncation.
pub fn read_i8<R: Read>(reader: &mut R) -> Result<i8, ProtocolError> {
    let mut byte = [0u8; 1];
    reader.read_exact(&mut byte)?;
    Ok(i8::from_be_bytes(byte))
}

/// Reads a signed big-endian 16-bit field (a Java `short`).
///
/// # Errors
/// [`ProtocolError::Io`] on stream truncation.
pub fn read_i16<R: Read>(reader: &mut R) -> Result<i16, ProtocolError> {
    let mut bytes = [0u8; 2];
    reader.read_exact(&mut bytes)?;
    Ok(i16::from_be_bytes(bytes))
}

/// Writes a signed 8-bit field.
///
/// # Errors
/// [`ProtocolError::Io`] on write failure.
pub fn write_i8<W: Write>(writer: &mut W, value: i8) -> Result<(), ProtocolError> {
    writer.write_all(&value.to_be_bytes())?;
    Ok(())
}

/// Writes a signed big-endian 16-bit field.
///
/// # Errors
/// [`ProtocolError::Io`] on write failure.
pub fn write_i16<W: Write>(writer: &mut W, value: i16) -> Result<(), ProtocolError> {
    writer.write_all(&value.to_be_bytes())?;
    Ok(())
}

/// Skips exactly `len` bytes without allocating them.
///
/// # Errors
/// [`ProtocolError::Io`] when the stream ends before `len` bytes were read.
pub fn skip_bytes<R: Read>(reader: &mut R, len: usize) -> Result<(), ProtocolError> {
    // Fixed-size scratch buffer: the skipped region may be large, but the
    // memory used stays constant.
    let mut scratch = [0u8; 512];
    let mut left = len;
    while left > 0 {
        let take = left.min(scratch.len());
        reader.read_exact(&mut scratch[..take])?;
        left -= take;
    }
    Ok(())
}

/// A reader that records every byte it hands to the caller.
///
/// Used where the wire format only reveals the length of a field *after*
/// parsing it: the bytes are parsed for their meaning and captured verbatim
/// so the field can be written back unchanged.
#[derive(Debug)]
pub struct RecordingReader<R> {
    /// The wrapped reader.
    inner: R,
    /// Every byte read so far, in order.
    recorded: Vec<u8>,
}

impl<R: Read> RecordingReader<R> {
    /// Wraps `inner` so that its bytes are recorded.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            recorded: Vec::new(),
        }
    }

    /// Returns the recorded bytes and consumes the wrapper.
    pub fn into_recorded(self) -> Vec<u8> {
        self.recorded
    }
}

impl<R: Read> Read for RecordingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buf)?;
        self.recorded.extend_from_slice(&buf[..read]);
        Ok(read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_fields_round_trip() {
        let mut buffer = Vec::new();
        write_i8(&mut buffer, -1).unwrap();
        write_i16(&mut buffer, -1234).unwrap();
        let mut slice = &buffer[..];
        assert_eq!(read_i8(&mut slice).unwrap(), -1);
        assert_eq!(read_i16(&mut slice).unwrap(), -1234);
        assert!(slice.is_empty());
    }

    #[test]
    fn skipping_consumes_exactly_the_requested_bytes() {
        let bytes = [1u8, 2, 3, 4, 5];
        let mut slice = &bytes[..];
        skip_bytes(&mut slice, 3).unwrap();
        assert_eq!(slice, &[4, 5]);
        // Truncation is an I/O error, not a panic.
        assert!(skip_bytes(&mut slice, 10).is_err());
    }

    #[test]
    fn recording_reader_captures_the_bytes_it_read() {
        let bytes = [9u8, 8, 7];
        let mut recorder = RecordingReader::new(&bytes[..]);
        let mut one = [0u8; 1];
        recorder.read_exact(&mut one).unwrap();
        assert_eq!(one[0], 9);
        assert_eq!(recorder.into_recorded(), vec![9u8]);
    }
}

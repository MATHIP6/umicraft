// SPDX-License-Identifier: MIT OR Apache-2.0

//! Packet framing: length-prefixed packets with optional zlib compression.
//!
//! Once the server sends Set Compression, every packet carries:
//! `[packet length][uncompressed length][data]`, where `data` is zlib-compressed
//! iff `uncompressed length > 0`. Before that, packets are simply
//! `[packet length][data]`. The packet length counts the compression-varint
//! bytes plus the data bytes themselves.
//!
//! Source: community protocol docs ("Packet structure" + "Compression" pages,
//! pre-1.13), matching vanilla's enabled-by-default `network-compression-threshold`
//! of 256 in 1.8 server.properties.

use std::io::{Read, Write};

use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use tracing::debug;

use crate::error::ProtocolError;
use crate::varint::{read_var_u32, var_u32_len, write_var_u32};

/// Vanilla-sized cap on a single packet body (2^21 - 1). Enforced on both
/// declared lengths so a hostile server cannot make us allocate gigabytes.
pub const MAX_PACKET_LEN: usize = 2_097_151;

/// How encryption/compression currently apply to the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Framing {
    /// No compression negotiated yet.
    Raw,
    /// Compression threshold negotiated; `None` disables compression entirely?
    /// `u32::MAX` is what vanilla sends to disable, we keep the raw threshold.
    Compressed {
        /// Packets of at least this many bytes are zlib-compressed.
        threshold: u32,
    },
}

impl Framing {
    /// Reads one framed packet body (decompressed when applicable).
    ///
    /// Returns the raw (uncompressed) packet bytes, *without* packet id —
    /// higher layers decode the body per state.
    ///
    /// # Errors
    /// [`ProtocolError::PacketTooLarge`] on oversized lengths,
    /// [`ProtocolError::Zlib`] on corrupt compressed streams, [`ProtocolError::Io`]
    /// on short reads.
    pub fn read_frame<R: Read>(&self, reader: &mut R) -> Result<Vec<u8>, ProtocolError> {
        let packet_len = read_var_u32(reader)? as usize;
        if packet_len > MAX_PACKET_LEN {
            return Err(ProtocolError::PacketTooLarge {
                size: packet_len,
                limit: MAX_PACKET_LEN,
            });
        }
        if packet_len == 0 {
            // A zero-length packet is always invalid: it cannot carry even a
            // packet id. Refuse rather than loop; the caller decides to close.
            return Err(ProtocolError::Invariant("zero-length packet".to_string()));
        }
        let mut body = vec![0u8; packet_len];
        reader.read_exact(&mut body)?;

        match self {
            Framing::Raw => Ok(body),
            Framing::Compressed { .. } => {
                // The uncompressed-length varint lives at the front of `body`.
                let mut inner = &body[..];
                let uncompressed_len = read_var_u32(&mut inner)? as usize;
                if uncompressed_len > MAX_PACKET_LEN {
                    return Err(ProtocolError::PacketTooLarge {
                        size: uncompressed_len,
                        limit: MAX_PACKET_LEN,
                    });
                }
                if uncompressed_len == 0 {
                    // Uncompressed packet: strip the zero-varint and pass through.
                    Ok(body[var_u32_len(0)..].to_vec())
                } else {
                    let mut inflated = Vec::with_capacity(uncompressed_len);
                    {
                        let mut decoder = ZlibDecoder::new(&mut inner);
                        decoder.read_to_end(&mut inflated)?;
                    }
                    if inflated.len() == uncompressed_len {
                        Ok(inflated)
                    } else {
                        Err(ProtocolError::Invariant(format!(
                            "compressed packet claims {uncompressed_len} bytes but inflated to {}",
                            inflated.len()
                        )))
                    }
                }
            }
        }
    }

    /// Writes one framed packet body with the connection's compression rules.
    ///
    /// `body` is the raw (uncompressed) packet bytes including packet id.
    ///
    /// # Errors
    /// [`ProtocolError::PacketTooLarge`] for oversized payloads,
    /// [`ProtocolError::Io`] on stream errors.
    pub fn write_frame<W: Write>(&self, writer: &mut W, body: &[u8]) -> Result<(), ProtocolError> {
        if body.is_empty() {
            return Err(ProtocolError::Invariant("empty packet body".to_string()));
        }
        match self {
            Framing::Raw => Self::write_raw(writer, body),
            Framing::Compressed { threshold } => {
                if *threshold == 0 || body.len() < *threshold as usize {
                    // Under threshold: an empty "uncompressed length" varint
                    // marks the packet as sent uncompressed.
                    let mut packet = crate::varint::var_u32_to_vec(0);
                    packet.reserve_exact(body.len());
                    packet.extend_from_slice(body);
                    Self::write_raw(writer, &packet)
                } else {
                    let mut packet = Vec::with_capacity(body.len());
                    write_var_u32(&mut packet, body.len() as u32)?;
                    {
                        let mut encoder = ZlibEncoder::new(&mut packet, Compression::default());
                        encoder.write_all(body)?;
                        encoder.finish()?;
                    } // encoder flushes into `packet` when dropped/finished.
                    debug!(size = packet.len(), raw = body.len(), "compressed frame");
                    Self::write_raw(writer, &packet)
                }
            }
        }
    }

    fn write_raw<W: Write>(writer: &mut W, payload: &[u8]) -> Result<(), ProtocolError> {
        if payload.len() > MAX_PACKET_LEN {
            return Err(ProtocolError::PacketTooLarge {
                size: payload.len(),
                limit: MAX_PACKET_LEN,
            });
        }
        write_var_u32(writer, payload.len() as u32)?;
        writer.write_all(payload)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(framing: Framing, body: &[u8]) -> Vec<u8> {
        let mut buffer = Vec::new();
        framing.write_frame(&mut buffer, body).unwrap();
        framing.read_frame(&mut &buffer[..]).unwrap()
    }

    #[test]
    fn raw_framing_round_trip() {
        let result = round_trip(Framing::Raw, b"\x00hello");
        assert_eq!(result, b"\x00hello");
    }

    #[test]
    fn compressed_framing_round_trip_small_and_large() {
        let framing = Framing::Compressed { threshold: 256 };
        // Under threshold: stays uncompressed (empty uncompressed-length varint).
        assert_eq!(round_trip(framing, b"small payload"), b"small payload");
        // Over threshold: compresses but the reader sees identical bytes.
        let big = vec![7u8; 4096];
        assert_eq!(round_trip(framing, &big), big);
    }

    #[test]
    fn compression_off_via_zero_threshold() {
        let framing = Framing::Compressed { threshold: 0 };
        assert_eq!(round_trip(framing, &[0xAA; 8192]), [0xAA; 8192]);
    }

    #[test]
    fn oversized_declared_lengths_do_not_allocate() {
        // Craft a frame whose declared length exceeds the cap.
        let mut hostile = Vec::new();
        crate::varint::write_var_u32(&mut hostile, (MAX_PACKET_LEN + 1) as u32).unwrap();
        let err = Framing::Raw.read_frame(&mut &hostile[..]).unwrap_err();
        assert!(matches!(err, ProtocolError::PacketTooLarge { .. }));

        // Same for the decoded uncompressed length claim.
        let mut compressed_claim = Vec::new();
        crate::varint::write_var_u32(&mut compressed_claim, (MAX_PACKET_LEN + 1) as u32).unwrap();
        let mut frame = Vec::new();
        crate::varint::write_var_u32(&mut frame, compressed_claim.len() as u32).unwrap();
        frame.extend_from_slice(&compressed_claim);
        let err = Framing::Compressed { threshold: 100 }
            .read_frame(&mut &frame[..])
            .unwrap_err();
        assert!(matches!(err, ProtocolError::PacketTooLarge { .. }));
    }

    #[test]
    fn corrupted_compression_is_an_error() {
        // packet: [len 6][uncompressed_len varint 16384 (3 bytes)][3 junk bytes]
        let mut buffer = Vec::new();
        crate::varint::write_var_u32(&mut buffer, 6).unwrap();
        write_var_u32(&mut buffer, 16_384).unwrap(); // claim a heavy uncompressed size
        buffer.extend_from_slice(&[0x99u8; 3]); // ...backed by zlib-garbage
        let err = Framing::Compressed { threshold: 1 }
            .read_frame(&mut &buffer[..])
            .unwrap_err();
        // flate2's std Read adapters surface corrupt zlib streams as io
        // errors; DecompressError/Invariant appear on other corrupt paths.
        assert!(matches!(
            err,
            ProtocolError::Io(_) | ProtocolError::Zlib(_) | ProtocolError::Invariant(_)
        ));
    }
}

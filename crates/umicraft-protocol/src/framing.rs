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

use std::cell::RefCell;
use std::io::{Read, Write};

use flate2::write::ZlibEncoder;
use flate2::{Compression, Decompress, FlushDecompress, Status};
use tracing::debug;

use crate::error::ProtocolError;
use crate::varint::{read_var_u32, write_var_u32};

thread_local! {
    static THREAD_DECOMPRESSOR: RefCell<Decompress> = RefCell::new(Decompress::new(true));
}

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
        let mut packet = vec![0u8; packet_len];
        reader.read_exact(&mut packet)?;
        self.unwrap_body(&packet)
    }

    /// Splits one fully-buffered framed packet into its (uncompressed) body.
    ///
    /// Connection-oriented readers use this on an accumulated pending buffer,
    /// so a truncated TCP read is simply "wait for more bytes" instead of an
    /// error; only genuinely malformed frames error here.
    ///
    /// # Errors
    /// [`ProtocolError::PacketTooLarge`] on oversized declarations,
    /// [`ProtocolError::Zlib`] on corrupt compressed streams,
    /// [`ProtocolError::Invariant`] on length mismatches.
    pub fn unwrap_body(&self, packet: &[u8]) -> Result<Vec<u8>, ProtocolError> {
        match self {
            Framing::Raw => Ok(packet.to_vec()),
            Framing::Compressed { .. } => {
                // The uncompressed-length varint lives at the front.
                let mut inner = packet;
                let uncompressed_len = read_var_u32(&mut inner)? as usize;
                if uncompressed_len > MAX_PACKET_LEN {
                    return Err(ProtocolError::PacketTooLarge {
                        size: uncompressed_len,
                        limit: MAX_PACKET_LEN,
                    });
                }
                if uncompressed_len == 0 {
                    // Uncompressed packet: strip the zero-varint and pass through.
                    Ok(inner.to_vec())
                } else {
                    let wire_len = inner.len();
                    let mut inflated = Vec::with_capacity(uncompressed_len);
                    let (res, consumed) = THREAD_DECOMPRESSOR.with(|cell| {
                        let mut dec = cell.borrow_mut();
                        dec.reset(true);
                        let before_in = dec.total_in();
                        let res = dec.decompress_vec(inner, &mut inflated, FlushDecompress::Finish);
                        let consumed = (dec.total_in() - before_in) as usize;
                        (res, consumed)
                    });
                    match res {
                        Ok(Status::StreamEnd) | Ok(Status::Ok) if inflated.len() == uncompressed_len => Ok(inflated),
                        Ok(_) => Err(ProtocolError::Invariant(format!(
                            "compressed packet claims {uncompressed_len} bytes but inflated to {}",
                            inflated.len()
                        ))),
                        Err(err) => Err(ProtocolError::Invariant(format!(
                            "zlib inflate failed ({err:?}): claimed {uncompressed_len} bytes, \
                             wire had {wire_len}, consumed {consumed}, inflated {}",
                            inflated.len()
                        ))),
                    }
                }
            }
        }
    }

    /// Splits one fully-buffered framed packet into `(packet_id, body)`.
    ///
    /// Avoids intermediate frame vector allocations by directly extracting the
    /// packet ID and returning the body vector in a single allocation.
    ///
    /// # Errors
    /// [`ProtocolError::PacketTooLarge`] on oversized declarations,
    /// [`ProtocolError::Zlib`] on corrupt compressed streams,
    /// [`ProtocolError::Invariant`] on length mismatches or empty frames.
    pub fn unwrap_packet(&self, frame: &[u8]) -> Result<(u8, Vec<u8>), ProtocolError> {
        match self {
            Framing::Raw => {
                if frame.is_empty() {
                    return Err(ProtocolError::Invariant("empty packet frame".to_string()));
                }
                let id = frame[0];
                let body = frame[1..].to_vec();
                Ok((id, body))
            }
            Framing::Compressed { .. } => {
                let mut inner = frame;
                let uncompressed_len = read_var_u32(&mut inner)? as usize;
                if uncompressed_len > MAX_PACKET_LEN {
                    return Err(ProtocolError::PacketTooLarge {
                        size: uncompressed_len,
                        limit: MAX_PACKET_LEN,
                    });
                }
                if uncompressed_len == 0 {
                    if inner.is_empty() {
                        return Err(ProtocolError::Invariant("empty packet frame".to_string()));
                    }
                    let id = inner[0];
                    let body = inner[1..].to_vec();
                    Ok((id, body))
                } else {
                    if uncompressed_len == 0 {
                        return Err(ProtocolError::Invariant("zero uncompressed length".to_string()));
                    }
                    let wire_len = inner.len();
                    let mut inflated = Vec::with_capacity(uncompressed_len);
                    let (res, consumed) = THREAD_DECOMPRESSOR.with(|cell| {
                        let mut dec = cell.borrow_mut();
                        dec.reset(true);
                        let before_in = dec.total_in();
                        let res = dec.decompress_vec(inner, &mut inflated, FlushDecompress::Finish);
                        let consumed = (dec.total_in() - before_in) as usize;
                        (res, consumed)
                    });
                    match res {
                        Ok(Status::StreamEnd) | Ok(Status::Ok) if inflated.len() == uncompressed_len => {
                            let id = inflated[0];
                            inflated.drain(..1);
                            Ok((id, inflated))
                        }
                        Ok(_) => Err(ProtocolError::Invariant(format!(
                            "compressed packet claims {uncompressed_len} bytes but inflated to {}",
                            inflated.len()
                        ))),
                        Err(err) => Err(ProtocolError::Invariant(format!(
                            "zlib inflate failed ({err:?}): claimed {uncompressed_len} bytes, \
                             wire had {wire_len}, consumed {consumed}, inflated {}",
                            inflated.len()
                        ))),
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
                    let mut packet = Vec::with_capacity(body.len() + 32);
                    write_var_u32(&mut packet, body.len() as u32)?;
                    {
                        let mut encoder = ZlibEncoder::new(&mut packet, Compression::default());
                        encoder.write_all(body)?;
                        encoder.finish()?;
                    }
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
    fn unwrap_packet_raw_and_compressed() {
        let payload = b"\x07hello world payload";
        let raw_frame = payload;
        let (id, body) = Framing::Raw.unwrap_packet(raw_frame).unwrap();
        assert_eq!(id, 0x07);
        assert_eq!(body, b"hello world payload");

        // Compressed under threshold
        let framing_uncompressed = Framing::Compressed { threshold: 256 };
        let mut buffer = Vec::new();
        framing_uncompressed.write_frame(&mut buffer, payload).unwrap();
        let mut reader = buffer.as_slice();
        let _ = read_var_u32(&mut reader).unwrap();
        let (id, body) = framing_uncompressed.unwrap_packet(reader).unwrap();
        assert_eq!(id, 0x07);
        assert_eq!(body, b"hello world payload");

        // Compressed over threshold
        let framing_compressed = Framing::Compressed { threshold: 4 };
        let mut buffer = Vec::new();
        framing_compressed.write_frame(&mut buffer, payload).unwrap();
        let mut reader = buffer.as_slice();
        let _ = read_var_u32(&mut reader).unwrap();
        let (id, body) = framing_compressed.unwrap_packet(reader).unwrap();
        assert_eq!(id, 0x07);
        assert_eq!(body, b"hello world payload");
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

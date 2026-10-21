// SPDX-License-Identifier: MIT OR Apache-2.0

//! In-memory packet capture: entries of (direction, timestamp, framed bytes).
//!
//! Byte format (`UMCAPT01`): magic, then a sequence of
//! `[direction u8][timestamp_ms u64 big-endian][len u32 big-endian][bytes]`.
//! Captured bytes are the *complete on-wire frame* including its length
//! varint, so a capture can be replayed as a raw byte stream for either
//! side without re-framing.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
/// Which side of a connection a captured frame came from.
pub enum Direction {
    /// Bytes flowing toward the server (clientbound server-side view).
    ClientToServer = 0,
    /// Bytes flowing toward the client (serverbound server-side view).
    ServerToClient = 1,
}

impl TryFrom<u8> for Direction {
    type Error = u8;

    /// # Errors
    /// The unknown direction byte, when not 0 or 1.
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::ClientToServer),
            1 => Ok(Self::ServerToClient),
            other => Err(other),
        }
    }
}

/// One recorded frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureEntry {
    /// Flow direction.
    pub direction: Direction,
    /// Captured wall time in milliseconds since a fixed reference.
    pub timestamp_ms: u64,
    /// Full on-wire frame bytes (length prefix included).
    pub data: Vec<u8>,
}

/// A capture bundle that can accumulate in memory and round-trip the file
/// format above.
#[derive(Debug, Clone, Default)]
pub struct Capture {
    /// Recorded frames, in capture order.
    pub entries: Vec<CaptureEntry>,
}

impl Capture {
    /// Records one frame with a caller-supplied timestamp.
    pub fn add(&mut self, direction: Direction, timestamp_ms: u64, data: Vec<u8>) {
        self.entries.push(CaptureEntry {
            direction,
            timestamp_ms,
            data,
        });
    }

    /// Records one pre-built frame (used by the replay tool, which hands
    /// captured entries back as frames).
    pub fn add_entry(&mut self, entry: CaptureEntry) {
        self.entries.push(entry);
    }

    /// Serializes to the capture byte format.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"UMCAPT01");
        for entry in &self.entries {
            out.push(entry.direction as u8);
            out.extend_from_slice(&entry.timestamp_ms.to_be_bytes());
            out.extend_from_slice(&(entry.data.len() as u32).to_be_bytes());
            out.extend_from_slice(&entry.data);
        }
        out
    }

    /// Parses the capture byte format.
    ///
    /// # Errors
    /// `std::io::Error` of kind `InvalidData` for a bad magic, truncated
    /// entry, oversized declaration, or unknown direction byte.
    pub fn from_bytes(bytes: &[u8]) -> std::io::Result<Self> {
        if bytes.len() < 8 || &bytes[..8] != b"UMCAPT01" {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "capture file has no UMCAPT01 magic",
            ));
        }
        let mut cursor = 8;
        let mut entries = Vec::new();
        while cursor < bytes.len() {
            let take = |cursor: &mut usize, need: usize| -> std::io::Result<&[u8]> {
                let end = cursor
                    .checked_add(need)
                    .filter(|end| *end <= bytes.len())
                    .ok_or_else(|| {
                        std::io::Error::new(std::io::ErrorKind::InvalidData, "truncated capture")
                    })?;
                let slice = &bytes[*cursor..end];
                *cursor = end;
                Ok(slice)
            };
            let direction =
                Direction::try_from(take(&mut cursor, 1)?[0]).map_err(|value: u8| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("capture file with unknown direction byte {value}"),
                    )
                })?;
            let timestamp_bytes = take(&mut cursor, 8)?;
            let timestamp_ms = u64::from_be_bytes(timestamp_bytes.try_into().map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "unreachable: slice was 8 bytes",
                )
            })?);
            let len_bytes = take(&mut cursor, 4)?;
            let len = u32::from_be_bytes(len_bytes.try_into().map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "unreachable: slice was 4 bytes",
                )
            })?) as usize;
            if len > bytes.len() - cursor {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("capture entry declares {len} bytes but {len}+ bytes remain ... buffer too small"),
                ));
            }
            let data = take(&mut cursor, len)?.to_vec();
            entries.push(CaptureEntry {
                direction,
                timestamp_ms,
                data,
            });
        }
        Ok(Self { entries })
    }

    /// Writes the capture to a file.
    ///
    /// # Errors
    /// Whatever `std::fs::write` reports.
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        std::fs::write(path, self.to_bytes())
    }

    /// Reads a capture from a file.
    ///
    /// # Errors
    /// Whatever `std::fs::read`/parsing reports.
    pub fn load(path: &std::path::Path) -> std::io::Result<Self> {
        Self::from_bytes(&std::fs::read(path)?)
    }
}

/// One-line description of a captured frame, advancing
/// `compression_active` when a login-state `Set Compression` body passes
/// by (compression applies from the packet *after* that frame). Frames
/// that cannot be interpreted are still described by size.
///
/// The threshold of the temporary `Framing` value does not matter here:
/// `unwrap_body` only consults the frame's own uncompressed-length varint.
#[must_use]
pub fn describe_frame(entry: &CaptureEntry, compression_active: &mut bool) -> String {
    use umicraft_protocol::framing::Framing;
    use umicraft_protocol::varint::take_var_u32;

    let mut payload = entry.data.as_slice();
    let mut consumed = 0usize;
    let Some(declared_len) = take_var_u32(payload, &mut consumed).ok().flatten() else {
        return format!("{} bytes (framing unreadable)", entry.data.len());
    };
    payload = &payload[consumed..];
    if declared_len as usize != payload.len() {
        return format!(
            "{} bytes, declared {declared_len} (len mismatch)",
            entry.data.len()
        );
    }

    // Try the compression-framed layout first once the session switched;
    // fall back to the raw layout when inflation fails on a boundary frame.
    let body = if *compression_active {
        Framing::Compressed { threshold: 1 }
            .unwrap_body(payload)
            .ok()
            .or_else(|| Framing::Raw.unwrap_body(payload).ok())
    } else {
        Framing::Raw.unwrap_body(payload).ok()
    };

    let Some(body) = body else {
        return format!(
            "{payload_len} framed bytes (payload unreadable)",
            payload_len = payload.len()
        );
    };

    // A login-state Set Compression body starts with packet id 0x03 and
    // carries exactly one threshold varint.
    if !*compression_active && body.first() == Some(&0x03) {
        if let Some(threshold) = read_trailing_var(&body) {
            *compression_active = true;
            return format!(
                "Set Compression: threshold={threshold} body_len={} \
                 [compression on from the next frame]",
                body.len()
            );
        }
    }

    // Small varint-only keepalive bodies are the acceptance metric; longer
    // 0x00 bodies are handshake/status shapes that must not read as ids.
    if body.first() == Some(&0x00) {
        if let Some(keepalive_id) = read_trailing_var(&body) {
            return format!("Keep Alive: id={keepalive_id} body_len={}", body.len());
        }
    }

    match body.first().copied() {
        Some(id) => format!("packet id=0x{id:02x} body_len={}", body.len()),
        None => "empty body".to_string(),
    }
}

/// Reads the VarInt that trails a one-byte body prefix (keepalive id after
/// `0x00`, Set Compression threshold after `0x03`); `None` when the rest is
/// not exactly one VarInt.
fn read_trailing_var(body: &[u8]) -> Option<i64> {
    use umicraft_protocol::varint::take_var_u32;

    let rest = body.get(1..)?;
    let mut consumed = 0usize;
    let value = take_var_u32(rest, &mut consumed).ok().flatten()?;
    (consumed == rest.len()).then(|| i64::from(value))
}

/// Peels the next complete *raw* frame (length prefix included) off `buffer`.
///
/// Returns `Ok(None)` while the buffer only holds an incomplete frame; this
/// is the MITM logger's streaming splitter and deliberately does not look
/// inside the frame (it must work on encrypted streams too).
///
/// # Errors
/// [`umicraft_protocol::ProtocolError::VarIntTooLong`] or
/// [`umicraft_protocol::ProtocolError::PacketTooLarge`] on malformed length
/// declarations.
pub fn take_raw_frame(
    buffer: &mut Vec<u8>,
) -> Result<Option<Vec<u8>>, umicraft_protocol::ProtocolError> {
    use umicraft_protocol::framing::MAX_PACKET_LEN;
    use umicraft_protocol::varint::take_var_u32;

    let mut consumed = 0usize;
    let Some(raw_len) = take_var_u32(buffer, &mut consumed)? else {
        return Ok(None);
    };
    let packet_len = raw_len as usize;
    if packet_len > MAX_PACKET_LEN {
        return Err(umicraft_protocol::ProtocolError::PacketTooLarge {
            size: packet_len,
            limit: MAX_PACKET_LEN,
        });
    }
    let total = consumed + packet_len;
    if buffer.len() < total {
        return Ok(None);
    }
    let frame = buffer[..total].to_vec();
    buffer.drain(..total);
    Ok(Some(frame))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_round_trip() {
        assert_eq!(Direction::try_from(0), Ok(Direction::ClientToServer));
        assert_eq!(Direction::try_from(1), Ok(Direction::ServerToClient));
        assert_eq!(Direction::try_from(2), Err(2));
    }

    #[test]
    fn capture_file_round_trip_keeps_order_and_bytes() {
        let mut capture = Capture::default();
        capture.add(Direction::ServerToClient, 1, vec![0x02, 0x03]);
        capture.add(Direction::ClientToServer, 2, vec![0x77]);
        capture.add(Direction::ServerToClient, 3, vec![0x00; 40]);

        let parsed = Capture::from_bytes(&capture.to_bytes()).unwrap();
        assert_eq!(parsed.entries, capture.entries);
    }

    #[test]
    fn corrupted_captures_error_cleanly() {
        assert!(Capture::from_bytes(b"nope").is_err());
        assert!(Capture::from_bytes(&[]).is_err());
        // Truncated entry (claims 300 bytes, has 2).
        let mut bytes = b"UMCAPT01".to_vec();
        bytes.push(1);
        bytes.extend(0u64.to_be_bytes());
        bytes.extend(300u32.to_be_bytes());
        bytes.extend([1, 2]);
        assert!(Capture::from_bytes(&bytes).is_err());
    }

    #[test]
    fn take_raw_frame_splits_streams() {
        // Two frames: [len 2][ab] and [len 1][c]; len bytes use the varint.
        let mut buffer = vec![0x02, b'a', b'b', 0x01, b'c'];
        assert_eq!(
            take_raw_frame(&mut buffer).unwrap().unwrap(),
            vec![0x02, b'a', b'b']
        );
        assert_eq!(
            take_raw_frame(&mut buffer).unwrap().unwrap(),
            vec![0x01, b'c']
        );
        assert!(take_raw_frame(&mut buffer).unwrap().is_none());
    }

    #[test]
    fn take_raw_frame_waits_on_partial() {
        let mut buffer = vec![0x05, b'a', b'b']; // declares 5 bytes, holds 2
        assert!(take_raw_frame(&mut buffer).unwrap().is_none());
        buffer.extend(*b"cde");
        assert_eq!(
            take_raw_frame(&mut buffer).unwrap().unwrap(),
            vec![0x05, b'a', b'b', b'c', b'd', b'e']
        );
    }

    #[test]
    fn describe_reads_raw_keepalive_ids() {
        use umicraft_protocol::varint::write_var_u32;

        let mut body = vec![0x00];
        write_var_u32(&mut body, 424_242).unwrap();
        let mut frame = Vec::new();
        write_var_u32(&mut frame, body.len() as u32).unwrap();
        frame.extend(&body);

        let mut compressed = false;
        assert_eq!(
            describe_frame(
                &CaptureEntry {
                    direction: Direction::ServerToClient,
                    timestamp_ms: 1,
                    data: frame,
                },
                &mut compressed
            ),
            "Keep Alive: id=424242 body_len=4"
        );
        assert!(!compressed);
    }

    #[test]
    fn describe_keeps_handshakes_out_of_the_keepalive_slot() {
        // Handshake body starts with 0x00 too, but continues beyond one varint.
        let mut body = vec![0x00, 0x2F, 0x09];
        body.extend(*b"127.0.0.1");
        body.extend([0x63, 0xDD, 0x02]); // port 25597, next state 2
        let mut frame = Vec::new();
        umicraft_protocol::varint::write_var_u32(&mut frame, body.len() as u32).unwrap();
        frame.extend(&body);

        let mut compressed = false;
        assert_eq!(
            describe_frame(
                &CaptureEntry {
                    direction: Direction::ClientToServer,
                    timestamp_ms: 1,
                    data: frame,
                },
                &mut compressed
            ),
            "packet id=0x00 body_len=15"
        );
        assert!(!compressed);
    }

    #[test]
    fn describe_flips_to_compression_after_set_compression() {
        use umicraft_protocol::framing::Framing;

        // Set Compression with threshold 256 (two-byte varint), raw framing:
        // [len 3][0x03 id][0x80 0x02 threshold].
        let frame = vec![0x03, 0x03, 0x80, 0x02];
        let mut compressed = false;
        assert_eq!(
            describe_frame(
                &CaptureEntry {
                    direction: Direction::ServerToClient,
                    timestamp_ms: 1,
                    data: frame.clone(),
                },
                &mut compressed
            ),
            "Set Compression: threshold=256 body_len=3 [compression on from the next frame]"
        );
        assert!(compressed);

        // Following frames carry the compression-framed layout; write one
        // through the framing layer (threshold 1 compresses even tiny
        // bodies) and check the inflated packet id surfaces.
        let mut framed = Vec::new();
        Framing::Compressed { threshold: 1 }
            .write_frame(&mut framed, &[0x08])
            .unwrap();
        let frame = framed;
        assert_eq!(
            describe_frame(
                &CaptureEntry {
                    direction: Direction::ServerToClient,
                    timestamp_ms: 2,
                    data: frame,
                },
                &mut compressed
            ),
            "packet id=0x08 body_len=1"
        );
        assert!(compressed);
    }
}

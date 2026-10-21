// SPDX-License-Identifier: MIT OR Apache-2.0

//! The tokio connection: framing + optional compression/encryption plumbing.
//!
//! Layer ordering matches the 1.8 protocol: a packet body is built with id +
//! fields, optionally zlib-framed ([`umicraft_protocol::Framing`]), then — as a
//! whole frame — encrypted with the CFB8 stream cipher before hitting the
//! socket. Reads run the reverse: decrypt raw socket bytes, accumulate into a
//! pending buffer, and peel whole frames off it, so a truncated TCP read is
//! simply "wait for more bytes" (frames only peek at complete varints).

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use umicraft_protocol::encryption::{AesCfb8Stream, ConnectionCiphers};
use umicraft_protocol::framing::MAX_PACKET_LEN;
use umicraft_protocol::varint::take_var_u32;
use umicraft_protocol::{EncodablePacket, Framing, ProtocolError};

use crate::error::NetError;

/// A connected (or connecting) 1.8 client socket, half-duplicated for
/// concurrent-safe sequential use.
pub struct Connection {
    host: String,
    port: u16,
    read_half: OwnedReadHalf,
    write_half: OwnedWriteHalf,
    framing: Framing,
    read_cipher: Option<AesCfb8Stream>,
    write_cipher: Option<AesCfb8Stream>,
    /// Decrypted bytes not yet peeled into a complete frame.
    pending: Vec<u8>,
    /// Reusable socket read scratch (peeked decrypt target).
    recv_buf: Vec<u8>,
}

impl Connection {
    /// Opens a TCP connection after resolving `host` (DNS-resolvable or IP).
    ///
    /// # Errors
    /// [`NetError::Io`] on connect failure.
    pub async fn connect(host: &str, port: u16) -> Result<Self, NetError> {
        let stream = TcpStream::connect((host, port)).await?;
        let (read_half, write_half) = stream.into_split();
        Ok(Self {
            host: host.to_string(),
            port,
            read_half,
            write_half,
            framing: Framing::Raw,
            read_cipher: None,
            write_cipher: None,
            pending: Vec::new(),
            recv_buf: vec![0u8; 8192],
        })
    }

    /// The host part of the connect target (handshake default address).
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The port of the connect target.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Applies a `Set Compression` threshold. A threshold of 0 (or negative)
    /// disables the compression layer entirely, per the community docs.
    ///
    /// Left public for test instrumentation (fake servers that negotiate
    /// compression mid-stream).
    pub fn set_compression_threshold(&mut self, threshold: i32) {
        self.framing = if threshold <= 0 {
            Framing::Raw
        } else {
            Framing::Compressed {
                threshold: threshold as u32,
            }
        };
    }

    /// Switches the connection to encrypted transport with `shared_secret`
    /// (16 bytes; key and IV are both the secret, per the 1.8 protocol).
    ///
    /// # Errors
    /// [`NetError::Protocol`] when the secret length is wrong.
    pub fn enable_encryption(&mut self, shared_secret: &[u8]) -> Result<(), NetError> {
        let ciphers = ConnectionCiphers::derive(shared_secret)?;
        self.write_cipher = Some(ciphers.encrypt);
        self.read_cipher = Some(ciphers.decrypt);
        Ok(())
    }

    /// Sends one packet: builds id + body, applies the framing/encryption
    /// layers, and flushes to the socket.
    ///
    /// # Errors
    /// [`NetError`] on encode or transport failure.
    pub async fn write_packet<PKT: EncodablePacket>(
        &mut self,
        packet: &PKT,
    ) -> Result<(), NetError> {
        let mut body = Vec::with_capacity(64);
        body.push(packet.packet_id());
        EncodablePacket::encode_body(packet, &mut body)?;
        let mut framed = Vec::with_capacity(body.len() + 10);
        self.framing.write_frame(&mut framed, &body)?;
        if let Some(cipher) = self.write_cipher.as_mut() {
            cipher.encrypt(&mut framed);
        }
        self.write_half.write_all(&framed).await?;
        Ok(())
    }

    /// Reads the next packet as `(id, body)`; unknown ids are returned to the
    /// caller for a per-state decision (the play state tolerates unknowable
    /// packet spam from real servers).
    ///
    /// # Errors
    /// [`NetError::ConnectionClosed`] on EOF, [`NetError::Protocol`] on
    /// malformed frames, [`NetError::Io`] on socket failure.
    pub async fn read_packet(&mut self) -> Result<(u8, Vec<u8>), NetError> {
        let packet = self.read_framed_packet().await?;
        // `take_framed_packet` rejects zero-length packets, so the id exists.
        let id = packet[0];
        Ok((id, packet[1..].to_vec()))
    }

    async fn read_framed_packet(&mut self) -> Result<Vec<u8>, NetError> {
        loop {
            if let Some(packet) = take_framed_packet(&mut self.pending, self.framing)? {
                return Ok(packet);
            }
            let n = self.read_half.read(self.recv_buf.as_mut_slice()).await?;
            if n == 0 {
                return Err(NetError::ConnectionClosed);
            }
            let chunk = &mut self.recv_buf[..n];
            if let Some(cipher) = self.read_cipher.as_mut() {
                cipher.decrypt(chunk);
            }
            self.pending.extend_from_slice(chunk);
        }
    }
}

/// Peels one complete framed packet (uncompressed) off `pending`, preferring
/// `Ok(None)` when the buffer holds an incomplete frame.
///
/// # Errors
/// [`ProtocolError`] on genuinely malformed declarations (too large, zero
/// length, broken compression content).
fn take_framed_packet(
    pending: &mut Vec<u8>,
    framing: Framing,
) -> Result<Option<Vec<u8>>, ProtocolError> {
    let mut consumed = 0usize;
    let Some(raw_len) = take_var_u32(pending, &mut consumed)? else {
        return Ok(None);
    };
    let packet_len = raw_len as usize;
    if packet_len > MAX_PACKET_LEN {
        return Err(ProtocolError::PacketTooLarge {
            size: packet_len,
            limit: MAX_PACKET_LEN,
        });
    }
    if packet_len == 0 {
        // Same refusal as `Framing::read_frame`: no packet id can follow.
        return Err(ProtocolError::Invariant("zero-length packet".to_string()));
    }
    let total = consumed + packet_len;
    if pending.len() < total {
        return Ok(None);
    }
    let packet = pending[consumed..total].to_vec();
    pending.drain(..total);
    Ok(Some(framing.unwrap_body(&packet)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_protocol::State;

    fn push_var(pending: &mut Vec<u8>, value: u32) {
        umicraft_protocol::varint::write_var_u32(pending, value).unwrap();
    }

    #[test]
    fn incomplete_frames_wait_for_more_bytes() {
        let mut pending = Vec::new();
        push_var(&mut pending, 10); // declares a 10-byte body...
                                    // ...but only 3 arrive: no frame is peeled, no error raised.
        pending.extend_from_slice(&[1, 2, 3]);
        assert!(take_framed_packet(&mut pending, Framing::Raw)
            .unwrap()
            .is_none());

        // The rest completes it.
        pending.extend_from_slice(&[4, 5, 6, 7, 8, 9, 10]);
        let packet = take_framed_packet(&mut pending, Framing::Raw)
            .unwrap()
            .unwrap();
        assert_eq!(packet, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        assert!(pending.is_empty());
    }

    #[test]
    fn multiple_frames_are_peeled_in_order() {
        let mut pending = Vec::new();
        push_var(&mut pending, 1);
        pending.push(0x41);
        push_var(&mut pending, 2);
        pending.extend_from_slice(&[0x42, 0x43]);
        assert_eq!(
            take_framed_packet(&mut pending, Framing::Raw).unwrap(),
            Some(vec![0x41])
        );
        assert_eq!(
            take_framed_packet(&mut pending, Framing::Raw).unwrap(),
            Some(vec![0x42, 0x43])
        );
    }

    #[test]
    fn compression_layer_is_unwrapped_by_take() {
        let mut pending = Vec::new();
        // Build [len][uncompressed-len 0][body] by hand for the passthrough.
        let body = vec![0x07u8; 40];
        let mut packet = umicraft_protocol::varint::var_u32_to_vec(0);
        packet.extend(&body);
        push_var(&mut pending, packet.len() as u32);
        pending.extend(&packet);
        let peeled = take_framed_packet(&mut pending, Framing::Compressed { threshold: 256 })
            .unwrap()
            .unwrap();
        assert_eq!(peeled, body);

        // Truly compressed body of a repetitive payload also round-trips.
        let framing_write = Framing::Compressed { threshold: 16 };
        let mut buffer = Vec::new();
        framing_write.write_frame(&mut buffer, &body).unwrap();
        pending.extend_from_slice(&buffer);
        assert_eq!(
            take_framed_packet(&mut pending, Framing::Compressed { threshold: 16 })
                .unwrap()
                .unwrap(),
            body
        );
    }

    #[test]
    fn hostile_declarations_error() {
        let mut pending = Vec::new();
        push_var(&mut pending, (MAX_PACKET_LEN as u64 + 1) as u32);
        assert!(matches!(
            take_framed_packet(&mut pending, Framing::Raw),
            Err(ProtocolError::PacketTooLarge { .. })
        ));

        let mut pending = Vec::new();
        push_var(&mut pending, 0);
        assert!(matches!(
            take_framed_packet(&mut pending, Framing::Raw),
            Err(ProtocolError::Invariant(_))
        ));
    }

    /// The unexpected-id reporting shape used by higher layers.
    #[test]
    fn unexpected_packet_error_is_typed() {
        let err = NetError::UnexpectedPacket {
            state: State::Login,
            id: 0x42,
        };
        assert!(err.to_string().contains("0x42"));
    }
}

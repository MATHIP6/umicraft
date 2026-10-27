// SPDX-License-Identifier: MIT OR Apache-2.0

//! Serverbound packet traces: the play-state packets a client sent, grouped
//! by client tick (ROADMAP.md P0, parity harness).
//!
//! A trace is extracted from a `.umcapt` capture recorded by
//! `umicraft-packet-logger` between a client and a local offline-mode 1.8.8
//! server. The same extraction runs on a capture of the vanilla client and on
//! a capture of Umicraft, so [`crate::tick_diff`] can compare them.
//!
//! # Following the connection state
//!
//! The capture holds raw frames of both directions in arrival order. The
//! extraction replays the connection state machine on them: the handshake
//! picks status or login, the clientbound login `Set Compression` (0x03)
//! switches both directions to the compressed framing, and `Login Success`
//! (0x02) enters the play state. A clientbound `Encryption Request` (0x01)
//! ends the extraction with [`TraceError::Encrypted`]: an online-mode stream
//! cannot be read by a proxy. Status connections (the server-list ping) are
//! skipped, so a capture may start with them. A capture must hold **one**
//! play session; frames of a second login are ignored.
//!
//! # Tick boundaries
//!
//! The serverbound stream carries no tick marker, so the boundaries are
//! inferred from the vanilla client's own rule (MCP
//! `EntityPlayerSP.onUpdate` and `onUpdateWalkingPlayer`): on foot, every
//! client tick ends with exactly one movement packet (0x03 `Player`, 0x04
//! `Player Position`, 0x05 `Player Look` or 0x06 `Player Position And
//! Look`); while riding, it ends with 0x05 followed by 0x0C `Steer Vehicle`.
//! The mouse and keyboard actions of `Minecraft.runTick` (`clickMouse`,
//! `rightClickMouse`, `sendClickBlockToController`) run before
//! `theWorld.updateEntities()`, so they belong to the tick whose movement
//! packet follows them.
//!
//! `Keep Alive` (0x00) answers are not part of any tick: vanilla sends them
//! from the network thread (`NetHandlerPlayClient.handleKeepAlive` does not
//! enqueue on the main thread), so their position in the stream is timing,
//! not behaviour. They are kept apart in [`Trace::keep_alives`].
//!
//! The ticks start after the client's first `Player Position And Look`
//! (0x06), its answer to the server's initial placement; the packets before
//! it (settings, brand) are kept in [`Trace::preamble`].

use std::fmt;

use thiserror::Error;
use umicraft_protocol::framing::Framing;
use umicraft_protocol::packets::play::serverbound::{DiggingStatus, EntityActionKind};
use umicraft_protocol::varint::take_var_u32;
use umicraft_protocol::ProtocolError;

use crate::capture::{Capture, CaptureEntry, Direction};

/// `Keep Alive` (0x00, serverbound, play).
const KEEP_ALIVE: u8 = 0x00;
/// `Player` (0x03, serverbound, play): no position or look change.
const PLAYER: u8 = 0x03;
/// `Player Position And Look` (0x06, serverbound, play).
const POSITION_LOOK: u8 = 0x06;
/// `Player Look` (0x05, serverbound, play).
const PLAYER_LOOK: u8 = 0x05;
/// `Player Digging` (0x07, serverbound, play).
const PLAYER_DIGGING: u8 = 0x07;
/// `Entity Action` (0x0B, serverbound, play).
const ENTITY_ACTION: u8 = 0x0B;
/// `Steer Vehicle` (0x0C, serverbound, play).
const STEER_VEHICLE: u8 = 0x0C;

/// Why a capture could not be turned into a trace.
#[derive(Debug, Error)]
pub enum TraceError {
    /// A frame could not be unwrapped (bad length prefix or compression).
    #[error("capture frame {index}: {source}")]
    Frame {
        /// Index of the frame in the capture.
        index: usize,
        /// The framing error.
        #[source]
        source: ProtocolError,
    },
    /// A handshake frame could not be parsed.
    #[error("capture frame {index}: malformed handshake")]
    Handshake {
        /// Index of the frame in the capture.
        index: usize,
    },
    /// The server asked for encryption; the stream cannot be read.
    #[error("capture frame {index}: the server requested encryption (online mode); record against an offline-mode server")]
    Encrypted {
        /// Index of the frame in the capture.
        index: usize,
    },
    /// The capture never reached the play state.
    #[error("the capture holds no play session")]
    NoPlaySession,
}

/// One serverbound play packet of a trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TracePacket {
    /// Wire packet id.
    pub id: u8,
    /// Capture timestamp in milliseconds.
    pub timestamp_ms: u64,
    /// Packet body without its id.
    pub body: Vec<u8>,
}

impl TracePacket {
    /// Whether the packet is a movement packet (0x03–0x06), the end of an on-foot tick.
    #[must_use]
    pub fn is_movement(&self) -> bool {
        (PLAYER..=POSITION_LOOK).contains(&self.id)
    }

    /// The comparison token of the packet: its id plus, for the packets
    /// whose meaning depends on a sub-code, that sub-code.
    ///
    /// `Entity Action` and `Player Digging` are one id for several distinct
    /// actions (start sprint, stop sneak, start digging…); comparing ids alone
    /// would call a sprint start equal to a sneak stop.
    #[must_use]
    pub fn token(&self) -> Token {
        Token {
            id: self.id,
            detail: self.detail(),
        }
    }

    fn detail(&self) -> Option<&'static str> {
        match self.id {
            ENTITY_ACTION => {
                // Entity id (VarInt) then the action ordinal (VarInt).
                let mut consumed = 0;
                take_var_u32(&self.body, &mut consumed).ok().flatten()?;
                let mut action_len = 0;
                let code = take_var_u32(self.body.get(consumed..)?, &mut action_len)
                    .ok()
                    .flatten()?;
                Some(entity_action_name(EntityActionKind::from_code(
                    u8::try_from(code).ok()?,
                )?))
            }
            PLAYER_DIGGING => {
                let code = *self.body.first()?;
                Some(digging_name(DiggingStatus::from_code(code)?))
            }
            _ => None,
        }
    }
}

/// What two traces compare for one packet: its id and optional sub-code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Token {
    /// Wire packet id.
    pub id: u8,
    /// Sub-code name (entity action, digging status), when the id has one.
    pub detail: Option<&'static str>,
}

impl fmt::Display for Token {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "0x{:02X}", self.id)?;
        if let Some(detail) = self.detail {
            write!(formatter, ":{detail}")?;
        }
        Ok(())
    }
}

/// The packets of one client tick, the last one being its movement packet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tick {
    /// Packets in send order.
    pub packets: Vec<TracePacket>,
}

impl Tick {
    /// The tokens of the tick's packets, in send order.
    #[must_use]
    pub fn tokens(&self) -> Vec<Token> {
        self.packets.iter().map(TracePacket::token).collect()
    }

    /// The tick's tokens joined by spaces, e.g. `0x0B:start_sprint 0x04`.
    #[must_use]
    pub fn signature(&self) -> String {
        let tokens: Vec<String> = self.tokens().iter().map(Token::to_string).collect();
        tokens.join(" ")
    }
}

/// A client's serverbound play traffic, grouped by tick.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Trace {
    /// Packets sent before the answer to the initial placement.
    pub preamble: Vec<TracePacket>,
    /// The ticks, in order.
    pub ticks: Vec<Tick>,
    /// Packets after the last movement packet (an unfinished tick).
    pub trailing: Vec<TracePacket>,
    /// `Keep Alive` answers, which belong to no tick.
    pub keep_alives: Vec<TracePacket>,
}

impl Trace {
    /// Extracts the trace of the play session recorded in `capture`.
    ///
    /// # Errors
    /// [`TraceError`] when a frame cannot be unwrapped, the session is
    /// encrypted, or the capture never reaches the play state.
    pub fn from_capture(capture: &Capture) -> Result<Self, TraceError> {
        Ok(Self::from_packets(serverbound_play_packets(capture)?))
    }

    /// Groups serverbound play packets into ticks (see the module docs).
    #[must_use]
    pub fn from_packets(packets: Vec<TracePacket>) -> Self {
        let mut trace = Self::default();
        let mut started = false;
        let mut current = Vec::new();
        let mut iter = packets.into_iter().peekable();
        while let Some(packet) = iter.next() {
            if packet.id == KEEP_ALIVE {
                trace.keep_alives.push(packet);
                continue;
            }
            if !started {
                started = packet.id == POSITION_LOOK;
                trace.preamble.push(packet);
                continue;
            }
            let ends_tick = packet.is_movement();
            let riding = packet.id == PLAYER_LOOK
                && iter.peek().is_some_and(|next| next.id == STEER_VEHICLE);
            current.push(packet);
            if riding {
                if let Some(steer) = iter.next() {
                    current.push(steer);
                }
            }
            if ends_tick {
                trace.ticks.push(Tick {
                    packets: std::mem::take(&mut current),
                });
            }
        }
        trace.trailing = current;
        trace
    }
}

/// Connection state of the replayed state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Handshake,
    Status,
    Login,
    Play,
    /// A second login after the recorded play session.
    Done,
}

/// Returns the serverbound play packets of the first play session in
/// `capture`, in capture order.
///
/// # Errors
/// See [`Trace::from_capture`].
pub fn serverbound_play_packets(capture: &Capture) -> Result<Vec<TracePacket>, TraceError> {
    let mut state = State::Handshake;
    let mut framing = Framing::Raw;
    let mut played = false;
    let mut packets = Vec::new();
    for (index, entry) in capture.entries.iter().enumerate() {
        if state == State::Done {
            break;
        }
        let body =
            frame_body(entry, framing).map_err(|source| TraceError::Frame { index, source })?;
        let Some((&id, rest)) = body.split_first() else {
            continue;
        };
        match (state, entry.direction) {
            (State::Handshake, Direction::ClientToServer) if id == 0x00 => {
                state = match handshake_next_state(rest) {
                    Some(1) => State::Status,
                    Some(2) => State::Login,
                    _ => return Err(TraceError::Handshake { index }),
                };
            }
            // A status exchange ends with the client's next handshake: the
            // status request is a bare id, a handshake carries fields.
            (State::Status, Direction::ClientToServer) if id == 0x00 && !rest.is_empty() => {
                state = match handshake_next_state(rest) {
                    Some(1) => State::Status,
                    Some(2) => State::Login,
                    _ => return Err(TraceError::Handshake { index }),
                };
            }
            (State::Login, Direction::ServerToClient) => match id {
                0x01 => return Err(TraceError::Encrypted { index }),
                0x02 => {
                    state = State::Play;
                    played = true;
                }
                0x03 => {
                    let mut consumed = 0;
                    let threshold = take_var_u32(rest, &mut consumed)
                        .map_err(|source| TraceError::Frame { index, source })?
                        .unwrap_or(0);
                    framing = Framing::Compressed { threshold };
                }
                _ => {}
            },
            (State::Play, Direction::ClientToServer) => packets.push(TracePacket {
                id,
                timestamp_ms: entry.timestamp_ms,
                body: rest.to_vec(),
            }),
            (State::Play, Direction::ServerToClient) if id == 0x40 => {
                // Play `Disconnect`: anything after it is a new connection.
                state = State::Done;
            }
            _ => {}
        }
    }
    if played {
        Ok(packets)
    } else {
        Err(TraceError::NoPlaySession)
    }
}

/// Unwraps one captured frame into its packet body (id first).
fn frame_body(entry: &CaptureEntry, framing: Framing) -> Result<Vec<u8>, ProtocolError> {
    let mut consumed = 0;
    let declared = take_var_u32(&entry.data, &mut consumed)?
        .ok_or_else(|| ProtocolError::Invariant("truncated frame length".to_string()))?;
    let payload = &entry.data[consumed..];
    if declared as usize != payload.len() {
        return Err(ProtocolError::Invariant(format!(
            "frame declares {declared} bytes but holds {}",
            payload.len()
        )));
    }
    framing.unwrap_body(payload)
}

/// Reads the `next state` field of a handshake body (after its id).
///
/// Layout: protocol version (VarInt), server address (VarInt-prefixed
/// string), port (u16), next state (VarInt).
fn handshake_next_state(body: &[u8]) -> Option<u32> {
    let mut cursor = 0;
    let mut consumed = 0;
    take_var_u32(body, &mut consumed).ok().flatten()?;
    cursor += consumed;
    consumed = 0;
    let address_len = take_var_u32(body.get(cursor..)?, &mut consumed)
        .ok()
        .flatten()?;
    cursor += consumed + address_len as usize + 2;
    consumed = 0;
    take_var_u32(body.get(cursor..)?, &mut consumed)
        .ok()
        .flatten()
}

fn entity_action_name(kind: EntityActionKind) -> &'static str {
    match kind {
        EntityActionKind::StartSneaking => "start_sneak",
        EntityActionKind::StopSneaking => "stop_sneak",
        EntityActionKind::StopSleeping => "stop_sleep",
        EntityActionKind::StartSprinting => "start_sprint",
        EntityActionKind::StopSprinting => "stop_sprint",
        EntityActionKind::RidingJump => "riding_jump",
        EntityActionKind::OpenInventory => "open_inventory",
    }
}

fn digging_name(status: DiggingStatus) -> &'static str {
    match status {
        DiggingStatus::StartDestroyBlock => "start_dig",
        DiggingStatus::AbortDestroyBlock => "abort_dig",
        DiggingStatus::StopDestroyBlock => "stop_dig",
        DiggingStatus::DropAllItems => "drop_stack",
        DiggingStatus::DropItem => "drop_item",
        DiggingStatus::ReleaseUseItem => "release_use",
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use umicraft_protocol::varint::write_var_u32;

    /// Frames `body` (id first) with `framing`.
    pub(crate) fn frame(framing: Framing, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        framing.write_frame(&mut out, body).unwrap();
        out
    }

    fn handshake(next_state: u32) -> Vec<u8> {
        let mut body = vec![0x00];
        write_var_u32(&mut body, 47).unwrap();
        write_var_u32(&mut body, 9).unwrap();
        body.extend(*b"localhost");
        body.extend(25565u16.to_be_bytes());
        write_var_u32(&mut body, next_state).unwrap();
        body
    }

    /// A capture of a status ping, then a compressed login and the given
    /// serverbound play bodies.
    pub(crate) fn session_capture(play: &[Vec<u8>]) -> Capture {
        let compressed = Framing::Compressed { threshold: 256 };
        let mut capture = Capture::default();
        let mut time = 0;
        let mut add = |capture: &mut Capture, direction, data| {
            time += 1;
            capture.add(direction, time, data);
        };
        add(
            &mut capture,
            Direction::ClientToServer,
            frame(Framing::Raw, &handshake(1)),
        );
        add(
            &mut capture,
            Direction::ClientToServer,
            frame(Framing::Raw, &[0x00]),
        );
        add(
            &mut capture,
            Direction::ServerToClient,
            frame(Framing::Raw, &[0x00, 0x02, b'{', b'}']),
        );
        add(
            &mut capture,
            Direction::ClientToServer,
            frame(Framing::Raw, &handshake(2)),
        );
        add(
            &mut capture,
            Direction::ClientToServer,
            frame(Framing::Raw, &[0x00, 0x01, b'a']),
        );
        add(
            &mut capture,
            Direction::ServerToClient,
            frame(Framing::Raw, &[0x03, 0x80, 0x02]),
        );
        add(
            &mut capture,
            Direction::ServerToClient,
            frame(compressed, &[0x02, 0x00, 0x00]),
        );
        for body in play {
            add(
                &mut capture,
                Direction::ClientToServer,
                frame(compressed, body),
            );
        }
        capture
    }

    #[test]
    fn play_packets_are_extracted_through_status_and_compression() {
        let capture = session_capture(&[vec![0x15, 1, 2], vec![0x04, 9]]);
        let packets = serverbound_play_packets(&capture).unwrap();
        let ids: Vec<u8> = packets.iter().map(|packet| packet.id).collect();
        assert_eq!(ids, [0x15, 0x04]);
        assert_eq!(packets[1].body, [9]);
    }

    #[test]
    fn an_encrypted_login_is_refused() {
        let mut capture = Capture::default();
        capture.add(
            Direction::ClientToServer,
            0,
            frame(Framing::Raw, &handshake(2)),
        );
        capture.add(
            Direction::ServerToClient,
            1,
            frame(Framing::Raw, &[0x01, 0x00]),
        );
        assert!(matches!(
            serverbound_play_packets(&capture),
            Err(TraceError::Encrypted { index: 1 })
        ));
    }

    #[test]
    fn a_capture_without_login_has_no_play_session() {
        let mut capture = Capture::default();
        capture.add(
            Direction::ClientToServer,
            0,
            frame(Framing::Raw, &handshake(1)),
        );
        assert!(matches!(
            serverbound_play_packets(&capture),
            Err(TraceError::NoPlaySession)
        ));
    }

    #[test]
    fn a_second_session_after_a_disconnect_is_ignored() {
        let mut capture = session_capture(&[vec![0x04]]);
        let compressed = Framing::Compressed { threshold: 256 };
        capture.add(
            Direction::ServerToClient,
            50,
            frame(compressed, &[0x40, 0x00]),
        );
        capture.add(
            Direction::ClientToServer,
            51,
            frame(Framing::Raw, &handshake(2)),
        );
        assert_eq!(serverbound_play_packets(&capture).unwrap().len(), 1);
    }

    fn packet(id: u8, body: &[u8]) -> TracePacket {
        TracePacket {
            id,
            timestamp_ms: 0,
            body: body.to_vec(),
        }
    }

    #[test]
    fn ticks_end_at_movement_packets_after_the_placement_answer() {
        let trace = Trace::from_packets(vec![
            packet(0x15, &[]),
            packet(0x06, &[]),
            packet(0x00, &[]),
            packet(0x0B, &[1, 3, 0]),
            packet(0x04, &[]),
            packet(0x03, &[]),
            packet(0x0A, &[]),
        ]);
        assert_eq!(trace.preamble.len(), 2);
        assert_eq!(trace.keep_alives.len(), 1);
        assert_eq!(trace.ticks.len(), 2);
        assert_eq!(trace.ticks[0].signature(), "0x0B:start_sprint 0x04");
        assert_eq!(trace.ticks[1].signature(), "0x03");
        assert_eq!(trace.trailing.len(), 1);
    }

    #[test]
    fn a_riding_tick_keeps_its_steer_vehicle() {
        let trace = Trace::from_packets(vec![
            packet(0x06, &[]),
            packet(0x05, &[]),
            packet(0x0C, &[]),
            packet(0x03, &[]),
        ]);
        assert_eq!(trace.ticks.len(), 2);
        assert_eq!(trace.ticks[0].signature(), "0x05 0x0C");
    }

    #[test]
    fn digging_tokens_carry_their_status() {
        assert_eq!(packet(0x07, &[0]).token().to_string(), "0x07:start_dig");
        assert_eq!(packet(0x07, &[]).token().to_string(), "0x07");
    }
}

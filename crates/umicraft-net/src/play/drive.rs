// SPDX-License-Identifier: MIT OR Apache-2.0

//! The one async loop of the play state (ADR-0010).
//!
//! [`drive`] races the socket against the caller's commands. The race only
//! involves cancel-safe futures (`Connection::read_packet` and
//! `Receiver::recv`); every write happens after the `select!` released the
//! connection, so a command arriving never interrupts a half-written frame.

use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::mpsc::Receiver;
use umicraft_protocol::PlayServerbound;

use super::session::{PlayEvent, PlaySession};
use crate::connection::Connection;
use crate::error::NetError;

/// Live statistics of a play session, shared with the caller.
///
/// The caller creates them (usually behind an `Arc`), [`drive`] updates them
/// and any thread may read them while the session runs, e.g. the debug
/// overlay. The 1.8 server limits the packets a player may send (PandaSpigot
/// kicks above 500 packets in seven seconds), so the sent count belongs in the
/// acceptance reports.
#[derive(Debug, Default)]
pub struct SessionCounters {
    received: AtomicU64,
    sent: AtomicU64,
    keepalives_answered: AtomicU64,
}

impl SessionCounters {
    /// The clientbound packets read from the socket.
    #[must_use]
    pub fn packets_received(&self) -> u64 {
        self.received.load(Ordering::Relaxed)
    }

    /// The serverbound packets written to the socket, replies and commands.
    #[must_use]
    pub fn packets_sent(&self) -> u64 {
        self.sent.load(Ordering::Relaxed)
    }

    /// The keepalives answered with their id.
    #[must_use]
    pub fn keepalives_answered(&self) -> u64 {
        self.keepalives_answered.load(Ordering::Relaxed)
    }
}

/// How a play session ended without an I/O or protocol failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEnd {
    /// The server sent a play `Disconnect` with this readable reason.
    Kicked(String),
    /// The caller dropped every command sender: the client is leaving.
    CommandsClosed,
}

/// What the loop received.
enum Incoming {
    Command(Option<PlayServerbound>),
    Packet(u8, Vec<u8>),
}

/// Runs the play state of a logged-in connection until it ends.
///
/// Each clientbound packet goes through `session`; its replies are written at
/// once, its event is handed to `on_event`. The `commands` are written in the
/// order they arrive. `on_event` runs on the loop's task: a callback that
/// blocks (for example a full bounded channel) slows the socket down, which
/// is the intended backpressure.
///
/// Dropping the returned future is only safe together with the connection:
/// it may stop between two writes of a reply. To stop a session and keep the
/// connection, drop the command senders instead.
///
/// # Errors
/// [`NetError`] when the socket fails or a handled packet is malformed (spec
/// NET-13); the connection must then be dropped.
pub async fn drive(
    connection: &mut Connection,
    session: &mut PlaySession,
    commands: &mut Receiver<PlayServerbound>,
    counters: &SessionCounters,
    mut on_event: impl FnMut(PlayEvent),
) -> Result<SessionEnd, NetError> {
    loop {
        // The read future borrows the connection, so nothing is written
        // until the select released it.
        let incoming = tokio::select! {
            biased;
            command = commands.recv() => Incoming::Command(command),
            packet = connection.read_packet() => {
                let (id, body) = packet?;
                Incoming::Packet(id, body)
            }
        };
        match incoming {
            Incoming::Command(None) => return Ok(SessionEnd::CommandsClosed),
            Incoming::Command(Some(command)) => {
                connection.write_packet(&command).await?;
                counters.sent.fetch_add(1, Ordering::Relaxed);
            }
            Incoming::Packet(id, body) => {
                counters.received.fetch_add(1, Ordering::Relaxed);
                let reaction = session.handle(id, &body)?;
                for reply in reaction.replies() {
                    connection.write_packet(reply).await?;
                    counters.sent.fetch_add(1, Ordering::Relaxed);
                    if matches!(reply, PlayServerbound::KeepAlive(_)) {
                        counters.keepalives_answered.fetch_add(1, Ordering::Relaxed);
                    }
                }
                if let Some(reason) = reaction.kick_reason() {
                    return Ok(SessionEnd::Kicked(reason.to_string()));
                }
                if let Some(event) = reaction.into_event() {
                    on_event(event);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::play::PlayOptions;
    use tokio::net::TcpListener;
    use tokio::sync::mpsc;
    use umicraft_protocol::packets::play::clientbound;
    use umicraft_protocol::packets::play::serverbound::Chat;
    use umicraft_protocol::{EncodablePacket, PlayClientbound, ProtocolError};

    /// A connected client/server pair over loopback, both in raw framing.
    async fn pair() -> (Connection, Connection) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let client = Connection::connect("127.0.0.1", addr.port());
        let (accepted, client) = tokio::join!(listener.accept(), client);
        let (stream, _) = accepted.expect("accept");
        (
            client.expect("connect"),
            Connection::adopt(stream, "127.0.0.1", addr.port()),
        )
    }

    async fn read_serverbound(server: &mut Connection) -> PlayServerbound {
        let (id, body) = server.read_packet().await.expect("server reads");
        PlayServerbound::decode(id, &mut body.as_slice()).expect("server decodes")
    }

    #[tokio::test]
    async fn keepalives_are_answered_events_forwarded_and_closing_the_commands_ends() {
        let (mut client, mut server) = pair().await;
        let server = tokio::spawn(async move {
            let keepalive = PlayClientbound::KeepAlive(clientbound::KeepAlive { id: 5 });
            server.write_packet(&keepalive).await.expect("write");
            let time = PlayClientbound::TimeUpdate(clientbound::TimeUpdate {
                world_age: 1,
                time_of_day: 6_000,
            });
            server.write_packet(&time).await.expect("write");
            read_serverbound(&mut server).await
        });

        let (sender, mut commands) = mpsc::channel(4);
        let mut sender = Some(sender);
        let mut session = PlaySession::new(PlayOptions::default());
        let counters = SessionCounters::default();
        let mut events = Vec::new();
        let end = drive(
            &mut client,
            &mut session,
            &mut commands,
            &counters,
            |event| {
                events.push(event);
                // The caller leaves once it saw the clock.
                sender = None;
            },
        )
        .await
        .expect("clean end");

        assert_eq!(end, SessionEnd::CommandsClosed);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], PlayEvent::TimeUpdate(_)));
        assert!(matches!(
            server.await.expect("server task"),
            PlayServerbound::KeepAlive(answer) if answer.id == 5
        ));
        assert_eq!(counters.packets_received(), 2);
        assert_eq!(counters.packets_sent(), 1);
        assert_eq!(counters.keepalives_answered(), 1);
    }

    #[tokio::test]
    async fn commands_are_written_and_a_disconnect_ends_with_the_reason() {
        let (mut client, mut server) = pair().await;
        let server = tokio::spawn(async move {
            let command = read_serverbound(&mut server).await;
            let kick = PlayClientbound::Disconnect(clientbound::Disconnect {
                reason: r#"{"text":"bye"}"#.to_string(),
            });
            server.write_packet(&kick).await.expect("write");
            command
        });

        let (sender, mut commands) = mpsc::channel(4);
        sender
            .send(PlayServerbound::Chat(Chat {
                message: "hello".to_string(),
            }))
            .await
            .expect("queue the command");
        let mut session = PlaySession::new(PlayOptions::default());
        let counters = SessionCounters::default();
        let end = drive(&mut client, &mut session, &mut commands, &counters, |_| {})
            .await
            .expect("clean end");

        assert_eq!(end, SessionEnd::Kicked("bye".to_string()));
        assert!(matches!(
            server.await.expect("server task"),
            PlayServerbound::Chat(chat) if chat.message == "hello"
        ));
        assert_eq!(counters.packets_sent(), 1);
        assert_eq!(counters.keepalives_answered(), 0);
    }

    /// A `Time Update` (0x03) whose body stops after 4 of its 16 bytes.
    struct TruncatedTimeUpdate;

    impl EncodablePacket for TruncatedTimeUpdate {
        fn packet_id(&self) -> u8 {
            clientbound::TimeUpdate::ID
        }

        fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
            writer.write_all(&[0, 0, 0, 1])?;
            Ok(())
        }
    }

    #[tokio::test]
    async fn a_malformed_packet_ends_the_session_with_an_error() {
        let (mut client, mut server) = pair().await;
        server
            .write_packet(&TruncatedTimeUpdate)
            .await
            .expect("write");

        let (_sender, mut commands) = mpsc::channel(1);
        let mut session = PlaySession::new(PlayOptions::default());
        let result = drive(
            &mut client,
            &mut session,
            &mut commands,
            &SessionCounters::default(),
            |_| {},
        )
        .await;
        assert!(matches!(result, Err(NetError::Protocol(_))), "{result:?}");
    }
}

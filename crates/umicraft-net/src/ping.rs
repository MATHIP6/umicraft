// SPDX-License-Identifier: MIT OR Apache-2.0

//! Server list ping (state `Status`): handshake → request → response →
//! ping/pong latency probe (spec NET-1).

use std::time::{Duration, Instant};

use tokio::time;
use umicraft_protocol::packets::status::serverbound;
use umicraft_protocol::{
    Handshake, State, StatusClientbound, StatusServerbound, PROTOCOL_VERSION_1_8,
};

use crate::connection::Connection;
use crate::error::NetError;
use crate::status::ServerStatus;

/// Result of a completed server list ping.
#[derive(Debug, Clone)]
pub struct PingResult {
    /// Parsed status payload.
    pub status: ServerStatus,
    /// Round-trip time between `Ping` and its echoed `Pong`.
    pub latency: Duration,
}

/// Pings `host:port` with the 1.8 server list protocol.
///
/// `budget` bounds each await (connect, response, pong) individually.
///
/// # Errors
/// [`NetError::TimedOut`] when a step exceeds `budget`, transport/protocol
/// errors otherwise. Anything but `Response`/`Pong` in the status state is an
/// [`NetError::UnexpectedPacket`].
pub async fn ping(host: &str, port: u16, budget: Duration) -> Result<PingResult, NetError> {
    async fn time_budget<T>(
        budget: Duration,
        operation: &'static str,
        future: impl std::future::Future<Output = Result<T, NetError>>,
    ) -> Result<T, NetError> {
        match time::timeout(budget, future).await {
            Ok(inner) => inner,
            Err(_) => Err(NetError::TimedOut { operation }),
        }
    }

    let mut conn = time_budget(budget, "connect", Connection::connect(host, port)).await?;
    conn.write_packet(&Handshake {
        protocol_version: PROTOCOL_VERSION_1_8,
        server_address: host.to_string(),
        server_port: port,
        next_state: 1,
    })
    .await?;
    conn.write_packet(&StatusServerbound::StatusRequest(serverbound::Request))
        .await?;

    let (id, body) = time_budget(budget, "status response", conn.read_packet()).await?;
    let json = match StatusClientbound::decode(id, &mut body.as_slice())? {
        StatusClientbound::Response(response) => response.json,
        StatusClientbound::Pong(_) => {
            return Err(NetError::UnexpectedPacket {
                state: State::Status,
                id,
            });
        }
    };
    let status: ServerStatus = serde_json::from_str(&json)?;

    conn.write_packet(&StatusServerbound::Ping(serverbound::Ping {
        time: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since_epoch| since_epoch.as_millis() as i64),
    }))
    .await?;

    let started = Instant::now();
    let (id, body) = time_budget(budget, "pong", conn.read_packet()).await?;
    match StatusClientbound::decode(id, &mut body.as_slice())? {
        StatusClientbound::Pong(_) => Ok(PingResult {
            status,
            latency: started.elapsed(),
        }),
        StatusClientbound::Response(_) => Err(NetError::UnexpectedPacket {
            state: State::Status,
            id,
        }),
    }
}

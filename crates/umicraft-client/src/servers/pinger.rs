// SPDX-License-Identifier: MIT OR Apache-2.0

//! Background status pings for the server list.
//!
//! A [`Pinger`] owns one thread running a Tokio current-thread runtime (the
//! same arrangement as `network.rs`). [`Pinger::refresh`] hands it the whole
//! list; every entry is pinged concurrently with `umicraft_net::ping::ping`
//! and each finished ping comes back as a [`PingResult`] that the menu polls
//! with [`Pinger::poll`] once per frame.
//!
//! A refresh cancels the pings still in flight and bumps the generation
//! number; results of older generations are dropped by `poll`, so the screen
//! never shows a reply for an entry that has since moved or been edited.
//!
//! Everything the server sends is untrusted: texts are cut to fixed lengths,
//! the favicon is size-capped and checked for the PNG signature, and
//! unparseable replies become [`PingOutcome::Unreachable`], never a panic.

use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::JoinHandle;
use std::time::Duration;

use base64::Engine;
use thiserror::Error;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::task::JoinSet;
use umicraft_net::ping::{ping, PingResult as NetPing};
use umicraft_net::status::ServerStatus;
use umicraft_protocol::PROTOCOL_VERSION_1_8;

use super::book::ServerAddress;

/// Budget for each await of a ping (connect, response, pong).
///
/// Vanilla relies on the 30 s Netty read timeout, which leaves a dead server
/// spinning in the list for half a minute. Five seconds covers a slow
/// transatlantic connect while keeping the refresh button responsive.
pub const PING_TIMEOUT: Duration = Duration::from_secs(5);

/// Largest accepted decoded favicon (vanilla icons are 64x64, a few KiB).
pub const MAX_FAVICON_BYTES: usize = 64 * 1024;

/// Longest kept MOTD, in characters (a vanilla MOTD is two lines of ~60).
const MAX_MOTD_CHARS: usize = 512;
/// Longest kept version name, in characters.
const MAX_VERSION_CHARS: usize = 64;
/// Longest kept player-sample line, in characters.
const MAX_SAMPLE_CHARS: usize = 64;
/// Most player-sample lines kept.
const MAX_SAMPLE_LINES: usize = 16;
/// The prefix vanilla servers put before the base64 PNG.
const FAVICON_PREFIX: &str = "data:image/png;base64,";
/// First eight bytes of every PNG file.
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// What the list needs to show for a server that answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInfo {
    /// MOTD flattened from chat JSON; `§` codes are left for the renderer.
    pub motd: String,
    /// Players online (negative or absent values become 0).
    pub players_online: u32,
    /// Player cap.
    pub players_max: u32,
    /// Advertised version name, e.g. `Paper 1.8.8`.
    pub version_name: String,
    /// Advertised protocol number (`None` when the server omits it).
    pub protocol: Option<i32>,
    /// Round-trip time of the status ping.
    pub latency_ms: u32,
    /// Decoded favicon, a validated PNG of at most [`MAX_FAVICON_BYTES`].
    pub favicon_png: Option<Vec<u8>>,
    /// Hover list lines (player names or server-chosen text).
    pub player_sample: Vec<String>,
}

/// The result of pinging one server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PingOutcome {
    /// The server answered and speaks protocol 47 (or did not say).
    Online(Box<ServerInfo>),
    /// The server answered but advertises another protocol; vanilla greys
    /// the entry and shows the version name in red.
    VersionMismatch(Box<ServerInfo>),
    /// Connecting or reading failed; carries a short human-readable reason.
    Unreachable(String),
}

/// A finished ping, as delivered by [`Pinger::poll`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PingResult {
    /// The caller's id of the entry (see [`PingTarget::id`]).
    pub id: usize,
    /// Generation of the refresh that produced it.
    pub generation: u64,
    /// What happened.
    pub outcome: PingOutcome,
}

/// One entry to ping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PingTarget {
    /// Caller-chosen id echoed in the result (typically the list index).
    pub id: usize,
    /// Where to connect.
    pub address: ServerAddress,
}

/// The ping thread could not be started.
#[derive(Debug, Error)]
#[error("could not start the server ping thread: {0}")]
pub struct PingerStartError(#[from] std::io::Error);

/// One request to the ping thread.
struct Batch {
    generation: u64,
    targets: Vec<PingTarget>,
}

/// Handle to the background ping thread.
///
/// Dropping it cancels the pings in flight and joins the thread.
#[derive(Debug)]
pub struct Pinger {
    requests: Option<UnboundedSender<Batch>>,
    results: Receiver<PingResult>,
    generation: u64,
    thread: Option<JoinHandle<()>>,
}

impl Pinger {
    /// Starts the ping thread with the default [`PING_TIMEOUT`].
    ///
    /// # Errors
    /// [`PingerStartError`] when the OS refuses to create the thread.
    pub fn new() -> Result<Self, PingerStartError> {
        Self::with_timeout(PING_TIMEOUT)
    }

    /// Starts the ping thread with a custom per-step timeout (tests).
    ///
    /// # Errors
    /// [`PingerStartError`] when the OS refuses to create the thread.
    pub fn with_timeout(timeout: Duration) -> Result<Self, PingerStartError> {
        let (requests, inbox) = unbounded_channel();
        let (outbox, results) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("server-ping".to_string())
            .spawn(move || run_thread(inbox, &outbox, timeout))?;
        Ok(Self {
            requests: Some(requests),
            results,
            generation: 0,
            thread: Some(thread),
        })
    }

    /// Cancels pings in flight and pings `targets` anew.
    ///
    /// Returns the new generation. Results that were already queued for the
    /// previous generation are discarded by [`Pinger::poll`].
    pub fn refresh(&mut self, targets: Vec<PingTarget>) -> u64 {
        self.generation += 1;
        let batch = Batch {
            generation: self.generation,
            targets,
        };
        if let Some(requests) = &self.requests {
            // A closed channel means the thread died; polling then simply
            // yields nothing and the entries stay "pinging".
            let _ = requests.send(batch);
        }
        self.generation
    }

    /// The generation of the latest [`Pinger::refresh`] (0 before the first).
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Takes the next current-generation result, if one has arrived.
    ///
    /// Never blocks; call it once per frame until it returns `None`.
    pub fn poll(&self) -> Option<PingResult> {
        loop {
            match self.results.try_recv() {
                Ok(result) if result.generation == self.generation => return Some(result),
                Ok(_stale) => {}
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return None,
            }
        }
    }
}

impl Drop for Pinger {
    fn drop(&mut self) {
        // Closing the request channel ends the thread's loop; the runtime
        // then drops every unfinished ping.
        self.requests = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run_thread(
    mut inbox: tokio::sync::mpsc::UnboundedReceiver<Batch>,
    outbox: &mpsc::Sender<PingResult>,
    timeout: Duration,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::warn!(%error, "server ping runtime failed to start");
            return;
        }
    };
    runtime.block_on(async {
        let mut running = JoinSet::new();
        loop {
            tokio::select! {
                batch = inbox.recv() => {
                    let Some(batch) = batch else { break };
                    running.abort_all();
                    for target in batch.targets {
                        let outbox = outbox.clone();
                        let generation = batch.generation;
                        running.spawn(async move {
                            let outcome = ping_one(&target.address, timeout).await;
                            let _ = outbox.send(PingResult { id: target.id, generation, outcome });
                        });
                    }
                }
                // Reap finished tasks; `join_next` is pending forever when the set is empty.
                Some(_) = running.join_next(), if !running.is_empty() => {}
            }
        }
    });
}

async fn ping_one(address: &ServerAddress, timeout: Duration) -> PingOutcome {
    match ping(&address.host, address.port, timeout).await {
        Ok(reply) => outcome_from_reply(&reply),
        Err(error) => PingOutcome::Unreachable(error.to_string()),
    }
}

/// Converts a completed ping into the outcome shown by the list.
fn outcome_from_reply(reply: &NetPing) -> PingOutcome {
    let info = Box::new(info_from_status(&reply.status, reply.latency));
    match info.protocol {
        Some(protocol) if protocol != PROTOCOL_VERSION_1_8 => PingOutcome::VersionMismatch(info),
        _ => PingOutcome::Online(info),
    }
}

fn info_from_status(status: &ServerStatus, latency: Duration) -> ServerInfo {
    let players = status.players.as_ref();
    let count = |value: Option<i64>| u32::try_from(value.unwrap_or(0).max(0)).unwrap_or(u32::MAX);
    ServerInfo {
        motd: truncate(&status.motd_text(), MAX_MOTD_CHARS),
        players_online: count(players.and_then(|block| block.online)),
        players_max: count(players.and_then(|block| block.max)),
        version_name: truncate(
            status
                .version
                .as_ref()
                .and_then(|version| version.name.as_deref())
                .unwrap_or_default(),
            MAX_VERSION_CHARS,
        ),
        protocol: status.version.as_ref().and_then(|version| version.protocol),
        latency_ms: u32::try_from(latency.as_millis()).unwrap_or(u32::MAX),
        favicon_png: status.favicon.as_deref().and_then(decode_favicon),
        player_sample: players
            .and_then(|block| block.sample.as_ref())
            .map(|sample| {
                sample
                    .iter()
                    .filter_map(|entry| entry.name.as_deref())
                    .take(MAX_SAMPLE_LINES)
                    .map(|name| truncate(name, MAX_SAMPLE_CHARS))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

/// Decodes a `data:image/png;base64,...` favicon.
///
/// Returns `None` when the prefix is wrong, the base64 is invalid, the image
/// is larger than [`MAX_FAVICON_BYTES`] or the bytes are not a PNG. The size
/// is checked on the encoded text first, so a hostile multi-megabyte string
/// is never decoded. Vanilla breaks lines with `\n` in some servers' output,
/// so whitespace is stripped.
fn decode_favicon(data_uri: &str) -> Option<Vec<u8>> {
    let encoded = data_uri.strip_prefix(FAVICON_PREFIX)?;
    if encoded.len() > MAX_FAVICON_BYTES / 3 * 4 + 4 {
        return None;
    }
    let compact: String = encoded.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(compact)
        .ok()?;
    (bytes.len() <= MAX_FAVICON_BYTES && bytes.starts_with(&PNG_SIGNATURE)).then_some(bytes)
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    const WAIT: Duration = Duration::from_secs(10);

    fn varint(mut value: u32) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                return out;
            }
            out.push(byte | 0x80);
        }
    }

    fn frame(id: u8, body: &[u8]) -> Vec<u8> {
        let mut payload = vec![id];
        payload.extend_from_slice(body);
        let mut out = varint(payload.len() as u32);
        out.extend(payload);
        out
    }

    fn response_frame(json: &str) -> Vec<u8> {
        let mut body = varint(json.len() as u32);
        body.extend_from_slice(json.as_bytes());
        frame(0x00, &body)
    }

    /// Reads one length-prefixed packet and returns its id.
    async fn read_id(stream: &mut tokio::net::TcpStream) -> Option<u8> {
        let mut length = 0u32;
        for shift in 0..5 {
            let byte = stream.read_u8().await.ok()?;
            length |= u32::from(byte & 0x7F) << (7 * shift);
            if byte & 0x80 == 0 {
                break;
            }
        }
        let mut packet = vec![0; length as usize];
        stream.read_exact(&mut packet).await.ok()?;
        packet.first().copied()
    }

    /// Serves every connection with `reply` (a raw response frame, or garbage).
    async fn fake_server(reply: Vec<u8>) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let reply = reply.clone();
                tokio::spawn(async move {
                    // handshake, then status request
                    read_id(&mut stream).await?;
                    read_id(&mut stream).await?;
                    stream.write_all(&reply).await.ok()?;
                    // ping: answer with a pong frame (id 0x01, i64 payload)
                    if read_id(&mut stream).await == Some(0x01) {
                        stream.write_all(&frame(0x01, &[0; 8])).await.ok()?;
                    }
                    Some(())
                });
            }
        });
        address
    }

    fn target(id: usize, address: SocketAddr) -> PingTarget {
        PingTarget {
            id,
            address: ServerAddress {
                host: address.ip().to_string(),
                port: address.port(),
            },
        }
    }

    /// Pings one address through a real `Pinger` and waits for the result.
    fn ping_through_pinger(address: SocketAddr) -> PingResult {
        let mut pinger = Pinger::with_timeout(Duration::from_secs(2)).unwrap();
        pinger.refresh(vec![target(7, address)]);
        let deadline = std::time::Instant::now() + WAIT;
        while std::time::Instant::now() < deadline {
            if let Some(result) = pinger.poll() {
                return result;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no ping result within {WAIT:?}");
    }

    fn png_uri(extra: usize) -> String {
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.extend(std::iter::repeat_n(0u8, extra));
        format!(
            "{FAVICON_PREFIX}{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
    }

    #[test]
    fn online_server_is_summarised() {
        let json = format!(
            r#"{{"version":{{"name":"Paper 1.8.8","protocol":47}},
            "players":{{"online":3,"max":20,"sample":[{{"name":"Ann","id":"x"}}]}},
            "description":{{"text":"§aHello","extra":[{{"text":" world"}}]}},
            "favicon":"{}"}}"#,
            png_uri(10)
        );
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let address = runtime.block_on(fake_server(response_frame(&json)));
        let result = ping_through_pinger(address);
        assert_eq!((result.id, result.generation), (7, 1));
        let PingOutcome::Online(info) = result.outcome else {
            panic!("expected Online, got {:?}", result.outcome);
        };
        assert_eq!(info.motd, "§aHello world");
        assert_eq!((info.players_online, info.players_max), (3, 20));
        assert_eq!(info.version_name, "Paper 1.8.8");
        assert_eq!(info.protocol, Some(47));
        assert_eq!(info.player_sample, ["Ann"]);
        assert_eq!(info.favicon_png.map(|png| png.len()), Some(18));
    }

    #[test]
    fn other_protocol_is_a_version_mismatch() {
        let json = r#"{"version":{"name":"1.12.2","protocol":340},"players":{"online":0,"max":1}}"#;
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let address = runtime.block_on(fake_server(response_frame(json)));
        let result = ping_through_pinger(address);
        assert!(
            matches!(result.outcome, PingOutcome::VersionMismatch(ref info) if info.protocol == Some(340)),
            "{:?}",
            result.outcome
        );
    }

    #[test]
    fn garbage_reply_is_unreachable() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let address = runtime.block_on(fake_server(response_frame("{ not json")));
        let result = ping_through_pinger(address);
        assert!(matches!(result.outcome, PingOutcome::Unreachable(_)));

        let address = runtime.block_on(fake_server(vec![0xFF; 12]));
        let result = ping_through_pinger(address);
        assert!(matches!(result.outcome, PingOutcome::Unreachable(_)));
    }

    #[test]
    fn closed_port_is_unreachable() {
        let address = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap()
        };
        let result = ping_through_pinger(address);
        assert!(matches!(result.outcome, PingOutcome::Unreachable(_)));
    }

    #[test]
    fn huge_favicon_is_dropped_but_the_server_stays_online() {
        let json = format!(
            r#"{{"version":{{"name":"x","protocol":47}},"favicon":"{}"}}"#,
            png_uri(MAX_FAVICON_BYTES + 1)
        );
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let address = runtime.block_on(fake_server(response_frame(&json)));
        let result = ping_through_pinger(address);
        let PingOutcome::Online(info) = result.outcome else {
            panic!("expected Online, got {:?}", result.outcome);
        };
        assert_eq!(info.favicon_png, None);
    }

    #[test]
    fn favicon_validation() {
        assert!(decode_favicon(&png_uri(100)).is_some());
        assert!(decode_favicon(&png_uri(MAX_FAVICON_BYTES)).is_none());
        assert!(decode_favicon("data:image/jpeg;base64,AAAA").is_none());
        assert!(decode_favicon("data:image/png;base64,!!!!").is_none());
        let not_png = base64::engine::general_purpose::STANDARD.encode(b"GIF89a-not-a-png");
        assert!(decode_favicon(&format!("{FAVICON_PREFIX}{not_png}")).is_none());
        let huge = format!("{FAVICON_PREFIX}{}", "A".repeat(5_000_000));
        assert!(decode_favicon(&huge).is_none());
    }

    #[test]
    fn hostile_numbers_and_texts_are_clamped() {
        let status: ServerStatus = serde_json::from_str(&format!(
            r#"{{"players":{{"online":-5,"max":99999999999,"sample":[{}]}},
            "description":"{}","version":{{"name":"{}"}}}}"#,
            vec![r#"{"name":"p"}"#; 40].join(","),
            "m".repeat(10_000),
            "v".repeat(10_000),
        ))
        .unwrap();
        let info = info_from_status(&status, Duration::from_secs(u64::MAX / 1_000_000));
        assert_eq!((info.players_online, info.players_max), (0, u32::MAX));
        assert_eq!(info.motd.chars().count(), MAX_MOTD_CHARS);
        assert_eq!(info.version_name.chars().count(), MAX_VERSION_CHARS);
        assert_eq!(info.player_sample.len(), MAX_SAMPLE_LINES);
        assert_eq!(info.protocol, None);
    }

    #[test]
    fn stale_generations_are_dropped() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let json = r#"{"version":{"name":"x","protocol":47}}"#;
        let address = runtime.block_on(fake_server(response_frame(json)));
        let mut pinger = Pinger::with_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(pinger.generation(), 0);
        pinger.refresh(vec![target(1, address)]);
        let second = pinger.refresh(vec![target(2, address)]);
        assert_eq!(second, 2);
        let deadline = std::time::Instant::now() + WAIT;
        while std::time::Instant::now() < deadline {
            if let Some(result) = pinger.poll() {
                assert_eq!((result.id, result.generation), (2, 2));
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no result for the current generation");
    }
}

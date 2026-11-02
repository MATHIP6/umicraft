// SPDX-License-Identifier: MIT OR Apache-2.0

//! MITM TCP proxy that forwards raw bytes between a local listener and an
//! upstream server while recording complete framed packets per direction
//! into a `.umcapt` file (spec M1: packet logger tool + capture/replay work).
//!
//! ```text
//! umicraft-packet-logger <listen_addr> <upstream_addr> [--out FILE]
//! ```
//!
//! Frames are split by the wire `[packet len][bytes]` rule *without* looking
//! inside them, so logging works on compressed or encrypted streams alike.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use umicraft_testkit::capture::{take_raw_frame, Capture, Direction};

const USAGE: &str =
    "usage: umicraft-packet-logger <listen_addr> <upstream_addr> [--out FILE.umcapt]";

struct Args {
    listen: String,
    upstream: String,
    out: PathBuf,
}

fn parse_args(argv: Vec<String>) -> Result<Args, String> {
    let mut positional = Vec::new();
    let mut out = PathBuf::from("umicraft-capture.umcapt");
    let mut index = 0;
    while index < argv.len() {
        match argv[index].as_str() {
            "--help" | "-h" => return Err(USAGE.to_string()),
            "--out" => {
                out = argv
                    .get(index + 1)
                    .ok_or("missing value after --out")?
                    .into();
                index += 2;
            }
            other => {
                positional.push(other.to_string());
                index += 1;
            }
        }
    }
    match positional.len() {
        2 => Ok(Args {
            listen: positional[0].clone(),
            upstream: positional[1].clone(),
            out,
        }),
        _ => Err(format!("need exactly two addresses\n{USAGE}")),
    }
}

/// Copies socket → socket while recording complete frames per direction.
///
/// Forwarding passes everything through byte-exact; only the *recording*
/// needs framing. Malformed length streams end the pump (connection must be
/// garbage anyway from that point).
async fn pump(
    mut source: tokio::net::tcp::OwnedReadHalf,
    mut target: tokio::net::tcp::OwnedWriteHalf,
    capture: Arc<Mutex<Capture>>,
    direction: Direction,
) {
    let mut pending: Vec<u8> = Vec::new();
    let mut chunk = vec![0u8; 16 * 1024];
    loop {
        let Ok(n) = source.read(chunk.as_mut_slice()).await else {
            return;
        };
        if n == 0 {
            return;
        }
        pending.extend_from_slice(&chunk[..n]);
        loop {
            match take_raw_frame(&mut pending) {
                Ok(Some(frame)) => {
                    if target.write_all(&frame).await.is_err() {
                        return;
                    }
                    capture.lock().expect("capture mutex is not poisoned").add(
                        direction,
                        ms_now(),
                        frame,
                    );
                }
                Ok(None) => break, // wait for more bytes
                Err(err) => {
                    eprintln!("corrupt frame stream on {direction:?}: {err}");
                    return;
                }
            }
        }
    }
}

/// Wall-clock milliseconds; 0 when the clock goes backwards.
fn ms_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |span| span.as_millis() as u64)
}

/// Writes the capture to `path` and returns how many frames it holds.
///
/// The lock is taken once: the logger used to lock the mutex again while the
/// guard of the first lock was still alive (a temporary of the `match`
/// scrutinee), so it saved the file and then hung on `Ctrl+C`.
fn save_capture(capture: &Mutex<Capture>, path: &std::path::Path) -> std::io::Result<usize> {
    let capture = capture.lock().expect("capture mutex is not poisoned");
    capture.save(path)?;
    Ok(capture.entries.len())
}

/// Binds, forwards, and records; ends on socket close or `Ctrl+C`.
async fn run(args: &Args, capture: Arc<Mutex<Capture>>) -> std::io::Result<()> {
    let listener = TcpListener::bind(&args.listen).await?;
    println!(
        "proxying {listen} -> {upstream}",
        listen = args.listen,
        upstream = args.upstream
    );
    loop {
        let (client_stream, _peer) = listener.accept().await?;
        let Ok(server_stream) = TcpStream::connect(&args.upstream).await else {
            println!("upstream {} unreachable, closing", args.upstream);
            continue;
        };
        // Both legs without Nagle, so the proxy does not move packets in time.
        if client_stream.set_nodelay(true).is_err() || server_stream.set_nodelay(true).is_err() {
            println!("could not set TCP_NODELAY, closing");
            continue;
        }
        let (client_read, client_write) = client_stream.into_split();
        let (server_read, server_write) = server_stream.into_split();
        tokio::spawn(pump(
            client_read,
            server_write,
            Arc::clone(&capture),
            Direction::ClientToServer,
        ));
        tokio::spawn(pump(
            server_read,
            client_write,
            Arc::clone(&capture),
            Direction::ServerToClient,
        ));
    }
}

fn main() {
    let args = match parse_args(std::env::args().skip(1).collect()) {
        Ok(args) => args,
        Err(usage) => {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    };
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("failed to start async runtime: {err}");
            std::process::exit(1);
        }
    };

    let capture = Arc::new(Mutex::new(Capture::default()));
    let result = runtime.block_on(async {
        tokio::select! {
            outcome = run(&args, Arc::clone(&capture)) => outcome,
            _ = tokio::signal::ctrl_c() => Ok(()),
        }
    });

    match save_capture(&capture, &args.out) {
        Ok(frames) => println!("recorded {frames} frames -> {}", args.out.display()),
        Err(err) => {
            eprintln!("failed to save capture: {err}");
            std::process::exit(1);
        }
    }

    if let Err(err) = result {
        eprintln!("proxy stopped: {err}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test: saving twice the lock of the capture must be free
    /// again, and the count is the number of frames.
    #[test]
    fn saving_a_capture_returns_its_frame_count_and_releases_the_lock() {
        let capture = Mutex::new(Capture::default());
        capture
            .lock()
            .expect("not poisoned")
            .add(Direction::ClientToServer, 1, vec![1, 0]);
        let dir = std::env::temp_dir().join(format!("umicraft-logger-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("test.umcapt");
        assert_eq!(save_capture(&capture, &path).expect("saved"), 1);
        assert_eq!(save_capture(&capture, &path).expect("saved again"), 1);
        std::fs::remove_dir_all(dir).expect("cleanup");
    }
}

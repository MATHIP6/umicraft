// SPDX-License-Identifier: MIT OR Apache-2.0

//! Replay of recorded `.umcapt` directions over TCP (spec M1 capture +
//! replay): pick one direction's frames and stream them byte-exact to
//! whichever side connects, optionally pacing by the recorded gaps.

use std::time::Duration;

use tokio::io::{AsyncWrite, AsyncWriteExt};
use umicraft_protocol::ProtocolError;

use crate::capture::{CaptureEntry, Direction};

/// Parses the replay CLI's direction shorthand.
///
/// # Errors
/// A message suggesting `c2s` or `s2c` for anything else.
pub fn parse_direction(token: &str) -> Result<Direction, String> {
    match token {
        "c2s" => Ok(Direction::ClientToServer),
        "s2c" => Ok(Direction::ServerToClient),
        other => Err(format!(
            "unknown direction {other:?}: use c2s (client-to-server) or s2c"
        )),
    }
}

/// Keeps only the frames flowing in `direction`, in order.
#[must_use]
pub fn select_direction(entries: &[CaptureEntry], direction: Direction) -> Vec<CaptureEntry> {
    entries
        .iter()
        .filter(|entry| entry.direction == direction)
        .cloned()
        .collect()
}

/// Writes all frames to `sink` as one continuous byte stream and closes
/// the write half. With `pace_cap_ms`, recorded frame gaps are honored up
/// to that cap (wall-clock gaps across sessions can be minutes; the cap
/// keeps replays useful).
///
/// # Errors
/// [`ProtocolError`] on write/flush/shutdown failure.
pub async fn replay_to<W: AsyncWrite + Unpin>(
    sink: &mut W,
    entries: &[CaptureEntry],
    pace_cap_ms: Option<u64>,
) -> Result<(), ProtocolError> {
    let mut previous_ms = entries.first().map_or(0, |entry| entry.timestamp_ms);
    for entry in entries {
        if let Some(cap) = pace_cap_ms {
            let delta = entry.timestamp_ms.saturating_sub(previous_ms).min(cap);
            if delta > 0 {
                tokio::time::sleep(Duration::from_millis(delta)).await;
            }
        }
        previous_ms = entry.timestamp_ms;
        sink.write_all(&entry.data).await?;
    }
    // End the replayed stream cleanly: the frame count alone does not tell
    // the reading side that the last packet is complete.
    sink.flush().await?;
    sink.shutdown().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::Capture;
    use std::time::Instant;

    /// Frame bytes including their length varint.
    fn frame(body: &[u8]) -> CaptureEntry {
        use umicraft_protocol::varint::write_var_u32;

        let mut data = Vec::new();
        write_var_u32(&mut data, body.len() as u32).unwrap();
        data.extend(body);
        CaptureEntry {
            direction: Direction::ServerToClient,
            timestamp_ms: 0,
            data,
        }
    }

    #[tokio::test]
    async fn replay_streams_selected_direction_byte_exact() {
        let mut capture = Capture::default();
        let mut c2s = frame(b"c-side");
        c2s.direction = Direction::ClientToServer;
        capture.add_entry(c2s);
        let mut s2c = frame(b"a");
        s2c.timestamp_ms = 10;
        capture.add_entry(s2c);
        let mut s2c2 = frame(b"bbbb");
        s2c2.timestamp_ms = 30;
        capture.add_entry(s2c2);

        let selected = select_direction(&capture.entries, Direction::ServerToClient);
        assert_eq!(selected.len(), 2);

        let mut sink = Vec::new();
        replay_to(&mut sink, &selected, None).await.unwrap();
        let mut expected = Vec::new();
        expected.extend([0x01, b'a']);
        expected.extend([0x04, b'b', b'b', b'b', b'b']);
        assert_eq!(sink, expected);
    }

    #[tokio::test]
    async fn replay_pacing_matches_recorded_gaps_up_to_the_cap() {
        let mut first = frame(b"a");
        first.timestamp_ms = 1_000;
        let mut second = frame(b"b");
        second.timestamp_ms = 1_080;
        let entries = vec![first, second];

        let started = Instant::now();
        let mut sink = Vec::new();
        // Recorded gap 80 ms, cap 20 ms: 20 ms early is the cap's win.
        replay_to(&mut sink, &entries, Some(20)).await.unwrap();
        let elapsed = started.elapsed();
        assert!(
            elapsed >= Duration::from_millis(20) && elapsed < Duration::from_secs(1),
            "expected roughly the capped-pacing wait of 20 ms, took {elapsed:?}"
        );
        assert_eq!(sink, [0x01, b'a', 0x01, b'b']);
    }
}

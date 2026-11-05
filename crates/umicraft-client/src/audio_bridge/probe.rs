// SPDX-License-Identifier: MIT OR Apache-2.0

//! `--audio-probe`: plays a few sound events on the real device and exits.
//!
//! It is the manual check of the whole chain (assets -> `sounds.json` -> decode
//! -> mixer -> device) on a machine with speakers; run it with
//! `RUST_LOG=umicraft_audio=debug` to see what the audio thread logs.

use std::thread::sleep;
use std::time::Duration;

use umicraft_audio::{Listener, SoundRequest};

use super::ui_sounds::press_request;
use super::AudioBridge;
use crate::error::ClientError;
use crate::options::SoundOptions;

/// How long each step of the probe lasts, and the tail that lets the last
/// sound finish before the thread stops.
const STEP: Duration = Duration::from_millis(700);

/// The probe steps: a label for the report and the request to play. The
/// listener stands at the origin facing south (`+z`), so a source at `-x` is
/// heard on the right (`Listener::pan_to`).
fn steps() -> Vec<(&'static str, SoundRequest)> {
    vec![
        ("button click (centre)", press_request()),
        (
            "stone dug 6 blocks to the right",
            SoundRequest::new("dig.stone").at([-6.0, 0.0, 0.0]),
        ),
        (
            "grass step 6 blocks to the left",
            SoundRequest::new("step.grass").at([6.0, 0.0, 0.0]),
        ),
        (
            "pop, 12 blocks ahead (quieter)",
            SoundRequest::new("random.pop").at([0.0, 0.0, 12.0]),
        ),
        (
            "explosion, 40 blocks away (inaudible: past 16 blocks)",
            SoundRequest::new("random.explode").at([40.0, 0.0, 0.0]),
        ),
    ]
}

/// Plays the probe on the system device and prints a report.
///
/// # Errors
///
/// [`ClientError::Assets`] when the sounds are not available (no downloaded
/// assets); a missing audio device only mutes the probe, as it does the game.
pub fn run() -> Result<(), ClientError> {
    let bridge = AudioBridge::start(&SoundOptions::default());
    if !bridge.is_enabled() {
        return Err(ClientError::Assets(
            "no sounds found: run `umicraft-client --setup-assets` first".to_owned(),
        ));
    }
    bridge.set_listener(Listener {
        position: [0.0; 3],
        yaw: 0.0,
    });
    for (label, request) in steps() {
        println!("playing: {label}");
        bridge.play(request);
        sleep(STEP);
    }
    println!("done; {} message(s) dropped (0 expected)", bridge.dropped());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_step_names_an_event_and_the_far_one_is_out_of_range() {
        let steps = steps();
        assert!(steps.iter().all(|(_, request)| !request.event.is_empty()));
        let (_, far) = steps.last().expect("steps");
        let listener = Listener {
            position: [0.0; 3],
            yaw: 0.0,
        };
        let distance = listener.distance_to(far.position.expect("positioned"));
        assert!(distance > 16.0 * f64::from(far.volume.max(1.0)));
    }
}

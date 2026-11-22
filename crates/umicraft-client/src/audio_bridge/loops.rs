// SPDX-License-Identifier: MIT OR Apache-2.0

//! Keeps the looping sounds of the audio thread in step with the game.
//!
//! The game lists, in every frame, the loops it wants heard
//! ([`LoopingSound`]). [`LoopSync`] remembers what it last sent: a new key
//! starts a loop, a key whose volume or position changed updates it, and a
//! key that left the list ends it (the cart was removed, the player got out).
//! A message the full queue dropped is not remembered, so it is sent again on
//! the next frame.

use std::collections::HashMap;

use umicraft_audio::{AudioHandle, SoundRequest};
use umicraft_game::sound::looping::LoopingSound;

/// The loops last sent to the audio thread, by key.
#[derive(Debug, Default)]
pub struct LoopSync {
    sent: HashMap<u64, LoopingSound>,
}

impl LoopSync {
    /// Sends what changed between the loops last sent and `wanted`.
    pub fn sync(&mut self, handle: &AudioHandle, wanted: &[LoopingSound]) {
        self.sent.retain(|key, _| {
            wanted.iter().any(|sound| sound.key == *key) || !handle.end_loop(*key)
        });
        for sound in wanted {
            if self.sent.get(&sound.key) == Some(sound) {
                continue;
            }
            if handle.set_loop(sound.key, request_of(sound)) {
                self.sent.insert(sound.key, *sound);
            }
        }
    }
}

/// The request of a looping sound: its event, volume and, when it has one,
/// its position.
fn request_of(sound: &LoopingSound) -> SoundRequest {
    let request = SoundRequest::new(sound.name).with_volume(sound.volume);
    match sound.position {
        Some(position) => request.at(position),
        None => request,
    }
}

#[cfg(test)]
mod tests {
    use umicraft_audio::{channel, AudioMessage};

    use super::*;

    fn rolling(key: u64, volume: f32) -> LoopingSound {
        LoopingSound {
            key,
            name: "minecart.base",
            position: Some([1.0, 2.0, 3.0]),
            volume,
        }
    }

    #[test]
    fn new_loops_start_changed_ones_update_and_gone_ones_end() {
        let (handle, receiver) = channel(16);
        let mut sync = LoopSync::default();
        sync.sync(&handle, &[rolling(1, 0.0), rolling(2, 0.1)]);
        sync.sync(&handle, &[rolling(1, 0.0), rolling(2, 0.3)]);
        sync.sync(&handle, &[rolling(2, 0.3)]);
        let messages: Vec<String> = receiver
            .try_iter()
            .map(|message| match message {
                AudioMessage::Loop { key, request } => format!("loop {key} {}", request.volume),
                AudioMessage::EndLoop(key) => format!("end {key}"),
                other => format!("{other:?}"),
            })
            .collect();
        assert_eq!(messages, ["loop 1 0", "loop 2 0.1", "loop 2 0.3", "end 1"]);
    }

    #[test]
    fn a_dropped_message_is_sent_again() {
        let (handle, receiver) = channel(1);
        let mut sync = LoopSync::default();
        sync.sync(&handle, &[rolling(1, 0.2), rolling(2, 0.2)]);
        assert_eq!(receiver.try_iter().count(), 1, "the queue held one");
        sync.sync(&handle, &[rolling(1, 0.2), rolling(2, 0.2)]);
        assert_eq!(receiver.try_iter().count(), 1, "the other one, now");
        sync.sync(&handle, &[rolling(1, 0.2), rolling(2, 0.2)]);
        assert_eq!(receiver.try_iter().count(), 0, "nothing changed");
    }
}

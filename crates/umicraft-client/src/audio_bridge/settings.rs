// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sound options, pushed to the audio thread.
//!
//! The options page edits `SoundOptions`; the engine multiplies a category
//! volume by the master volume itself (`CategoryVolumes::effective`, the rule of
//! `SoundManager.getSoundCategoryVolume`), so this sends the raw slider values.

use umicraft_audio::{AudioHandle, SoundCategory};

use crate::options::SoundOptions;

/// Remembers what the audio thread was last told, so that only the sliders that
/// moved send a message.
#[derive(Debug, Default)]
pub struct VolumeSync {
    sent: Option<SoundOptions>,
}

impl VolumeSync {
    /// Sends every category whose volume differs from the last sync (all of
    /// them the first time). A message the full queue drops is sent again on
    /// the next call, so the audio thread cannot stay out of date.
    pub fn sync(&mut self, handle: &AudioHandle, options: &SoundOptions) {
        let mut complete = true;
        for category in SoundCategory::ALL {
            let volume = options.volume(category);
            let known = self.sent.as_ref().map(|sent| sent.volume(category));
            if known != Some(volume) {
                complete &= handle.set_volume(category, volume);
            }
        }
        if complete {
            self.sent = Some(options.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::Receiver;

    use umicraft_audio::{channel, AudioMessage};

    use super::*;

    fn volumes(receiver: &Receiver<AudioMessage>) -> Vec<(SoundCategory, f32)> {
        receiver
            .try_iter()
            .filter_map(|message| match message {
                AudioMessage::Volume(category, volume) => Some((category, volume)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_first_sync_sends_every_category() {
        let (handle, receiver) = channel(32);
        let mut sync = VolumeSync::default();
        let mut options = SoundOptions::default();
        options.set_volume(SoundCategory::Music, 0.25);
        sync.sync(&handle, &options);
        let sent = volumes(&receiver);
        assert_eq!(sent.len(), 9);
        assert!(sent.contains(&(SoundCategory::Music, 0.25)));
        assert!(sent.contains(&(SoundCategory::Master, 1.0)));
    }

    #[test]
    fn only_the_slider_that_moved_is_sent() {
        let (handle, receiver) = channel(32);
        let mut sync = VolumeSync::default();
        let mut options = SoundOptions::default();
        sync.sync(&handle, &options);
        volumes(&receiver);
        sync.sync(&handle, &options);
        assert!(volumes(&receiver).is_empty(), "nothing changed");
        options.set_volume(SoundCategory::Master, 0.5);
        sync.sync(&handle, &options);
        assert_eq!(volumes(&receiver), [(SoundCategory::Master, 0.5)]);
    }

    #[test]
    fn a_dropped_message_is_retried() {
        let (handle, receiver) = channel(1);
        let mut sync = VolumeSync::default();
        let options = SoundOptions::default();
        sync.sync(&handle, &options);
        assert_eq!(volumes(&receiver).len(), 1, "the queue held one message");
        sync.sync(&handle, &options);
        assert_eq!(volumes(&receiver).len(), 1, "the rest is sent again");
    }

    #[test]
    fn master_scales_the_category_on_the_engine_side() {
        // The bridge sends raw values; the product is the engine's rule.
        let mut engine = umicraft_audio::CategoryVolumes::default();
        engine.set(SoundCategory::Master, 0.5);
        engine.set(SoundCategory::Blocks, 0.5);
        assert_eq!(engine.effective(SoundCategory::Blocks), 0.25);
    }
}

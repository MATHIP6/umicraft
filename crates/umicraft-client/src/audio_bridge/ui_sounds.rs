// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sounds of the menus.
//!
//! Vanilla plays `gui.button.press` when a button is pressed
//! (`GuiButton.playPressSound`, called by `GuiScreen.mouseClicked` right before
//! `actionPerformed`). The option sliders (`GuiOptionSlider`,
//! `GuiScreenOptionsSounds.Button`) extend `GuiButton`, so a press on a slider
//! clicks once too, but dragging it does not. List rows (`GuiSlot`) are not
//! buttons and stay silent. The UI crate counts the presses
//! (`Ui::take_button_presses`); the window passes the count here.

use umicraft_audio::SoundRequest;

use super::AudioBridge;

/// The event of a button press (`sounds.json`, `random/click`).
pub const BUTTON_PRESS: &str = "gui.button.press";

/// The most presses played for one frame: a frame holds a few clicks at most,
/// and a burst must not flood the queue.
const MAX_PRESSES_PER_FRAME: u32 = 2;

/// The request of one button press: at the listener, full volume and pitch
/// (`PositionedSoundRecord.create(location, 1.0F)`).
#[must_use]
pub fn press_request() -> SoundRequest {
    SoundRequest::new(BUTTON_PRESS)
}

impl AudioBridge {
    /// Plays the click of `presses` buttons pressed in the last UI frame.
    pub fn play_button_presses(&self, presses: u32) {
        for _ in 0..presses.min(MAX_PRESSES_PER_FRAME) {
            self.play(press_request());
        }
    }
}

#[cfg(test)]
mod tests {
    use umicraft_audio::channel;

    use super::*;

    #[test]
    fn a_press_is_an_unpositioned_full_volume_click() {
        let request = press_request();
        assert_eq!(&*request.event, "gui.button.press");
        assert_eq!(request.position, None);
        assert_eq!((request.volume, request.pitch), (1.0, 1.0));
    }

    #[test]
    fn each_press_plays_once_up_to_the_frame_limit() {
        let (handle, receiver) = channel(16);
        let bridge = AudioBridge::with_handle(handle);
        bridge.play_button_presses(0);
        assert_eq!(receiver.try_iter().count(), 0);
        bridge.play_button_presses(1);
        assert_eq!(receiver.try_iter().count(), 1);
        bridge.play_button_presses(50);
        assert_eq!(receiver.try_iter().count(), 2);
    }

    #[test]
    fn a_disabled_bridge_plays_nothing_and_does_not_panic() {
        AudioBridge::disabled().play_button_presses(3);
    }
}

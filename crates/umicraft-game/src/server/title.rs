// SPDX-License-Identifier: MIT OR Apache-2.0

//! The on-screen title and subtitle with their fade timer.
//!
//! Source: MCP `NetHandlerPlayClient.handleTitle` and
//! `GuiIngame.displayTitle` / `updateTick` (the `titlesTimer`). Texts are the
//! chat JSON of the packet; the chat renderer arrives with the HUD.

use umicraft_protocol::packets::play::clientbound::title::Title as TitlePacket;

/// `GuiIngame` default fade-in, stay and fade-out, in ticks.
const DEFAULT_FADE_IN: i32 = 10;
const DEFAULT_STAY: i32 = 70;
const DEFAULT_FADE_OUT: i32 = 20;

/// The title overlay state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleState {
    title: String,
    subtitle: String,
    timer: i32,
    fade_in: i32,
    stay: i32,
    fade_out: i32,
}

impl Default for TitleState {
    fn default() -> Self {
        Self {
            title: String::new(),
            subtitle: String::new(),
            timer: 0,
            fade_in: DEFAULT_FADE_IN,
            stay: DEFAULT_STAY,
            fade_out: DEFAULT_FADE_OUT,
        }
    }
}

impl TitleState {
    /// Applies a title packet (0x45).
    ///
    /// A title restarts the timer, a subtitle only sets its text, negative
    /// times keep the old value, `Clear` hides the texts and `Reset` also
    /// restores the default times. (The reference `Reset` momentarily shows
    /// an empty title; the visible result is identical.)
    pub fn apply(&mut self, packet: &TitlePacket) {
        match packet {
            TitlePacket::Title { json } => {
                self.title.clone_from(json);
                self.timer = self.total();
            }
            TitlePacket::Subtitle { json } => self.subtitle.clone_from(json),
            TitlePacket::Times {
                fade_in,
                stay,
                fade_out,
            } => self.set_times(*fade_in, *stay, *fade_out),
            TitlePacket::Clear => self.hide(),
            TitlePacket::Reset => {
                self.hide();
                self.set_times(DEFAULT_FADE_IN, DEFAULT_STAY, DEFAULT_FADE_OUT);
            }
        }
    }

    fn set_times(&mut self, fade_in: i32, stay: i32, fade_out: i32) {
        for (field, value) in [
            (&mut self.fade_in, fade_in),
            (&mut self.stay, stay),
            (&mut self.fade_out, fade_out),
        ] {
            if value >= 0 {
                *field = value;
            }
        }
        if self.timer > 0 {
            self.timer = self.total();
        }
    }

    fn hide(&mut self) {
        self.title.clear();
        self.subtitle.clear();
        self.timer = 0;
    }

    fn total(&self) -> i32 {
        self.fade_in
            .saturating_add(self.stay)
            .saturating_add(self.fade_out)
    }

    /// Counts the timer down by one tick and clears the texts when it ends.
    pub fn tick(&mut self) {
        if self.timer > 0 {
            self.timer -= 1;
            if self.timer <= 0 {
                self.hide();
            }
        }
    }

    /// The title chat JSON ("" when none).
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The subtitle chat JSON ("" when none).
    pub fn subtitle(&self) -> &str {
        &self.subtitle
    }

    /// Remaining ticks of the display, 0 when hidden.
    pub fn timer(&self) -> i32 {
        self.timer
    }

    /// Whether the overlay is being shown.
    pub fn is_visible(&self) -> bool {
        self.timer > 0
    }

    /// The fade-in, stay and fade-out durations in ticks.
    pub fn times(&self) -> (i32, i32, i32) {
        (self.fade_in, self.stay, self.fade_out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn title(json: &str) -> TitlePacket {
        TitlePacket::Title {
            json: json.to_owned(),
        }
    }

    #[test]
    fn a_title_starts_the_timer_with_the_default_times() {
        let mut state = TitleState::default();
        state.apply(&title("\"Hi\""));
        assert_eq!(state.title(), "\"Hi\"");
        assert_eq!(state.timer(), 100);
        assert!(state.is_visible());
    }

    #[test]
    fn a_subtitle_does_not_start_the_timer() {
        let mut state = TitleState::default();
        state.apply(&TitlePacket::Subtitle {
            json: "\"sub\"".to_owned(),
        });
        assert_eq!(state.subtitle(), "\"sub\"");
        assert!(!state.is_visible());
    }

    #[test]
    fn tick_counts_down_then_hides() {
        let mut state = TitleState::default();
        state.apply(&TitlePacket::Times {
            fade_in: 1,
            stay: 1,
            fade_out: 1,
        });
        state.apply(&title("t"));
        state.tick();
        state.tick();
        assert_eq!(state.timer(), 1);
        assert_eq!(state.title(), "t");
        state.tick();
        assert_eq!(state.title(), "");
        assert_eq!(state.timer(), 0);
        state.tick();
        assert_eq!(state.timer(), 0);
    }

    #[test]
    fn times_ignore_negatives_and_rescale_a_running_timer() {
        let mut state = TitleState::default();
        state.apply(&title("t"));
        state.apply(&TitlePacket::Times {
            fade_in: -1,
            stay: 5,
            fade_out: 0,
        });
        assert_eq!(state.times(), (10, 5, 0));
        assert_eq!(state.timer(), 15);
    }

    #[test]
    fn clear_hides_and_reset_restores_default_times() {
        let mut state = TitleState::default();
        state.apply(&TitlePacket::Times {
            fade_in: 1,
            stay: 2,
            fade_out: 3,
        });
        state.apply(&title("t"));
        state.apply(&TitlePacket::Clear);
        assert!(!state.is_visible());
        assert_eq!(state.times(), (1, 2, 3));
        state.apply(&title("t"));
        state.apply(&TitlePacket::Reset);
        assert_eq!(state.title(), "");
        assert_eq!(state.times(), (10, 70, 20));
    }

    #[test]
    fn huge_times_do_not_overflow() {
        let mut state = TitleState::default();
        state.apply(&TitlePacket::Times {
            fade_in: i32::MAX,
            stay: i32::MAX,
            fade_out: i32::MAX,
        });
        state.apply(&title("t"));
        assert_eq!(state.timer(), i32::MAX);
    }
}

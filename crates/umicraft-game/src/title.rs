// SPDX-License-Identifier: MIT OR Apache-2.0

//! The title, the subtitle and the action bar (M10, GAM-7).
//!
//! Both are timers advanced once per game tick (20 Hz) and read by the HUD
//! with the partial tick for a smooth fade. The rules follow MCP
//! `GuiIngame` (`displayTitle`, `func_175177_a`, `updateTick`, the title and
//! overlay-message parts of `renderGameOverlay`) and
//! `NetHandlerPlayClient.handleTitle`.
//!
//! The texts stay raw chat JSON; the renderer flattens them. The action bar
//! is drawn from the *unformatted* text in 1.8
//! (`setRecordPlaying(IChatComponent)` uses `getUnformattedText`).

use umicraft_protocol::packets::play::clientbound::Title;

/// Default fade-in duration, in ticks.
pub const DEFAULT_FADE_IN: i32 = 10;
/// Default time the title stays fully visible, in ticks.
pub const DEFAULT_STAY: i32 = 70;
/// Default fade-out duration, in ticks.
pub const DEFAULT_FADE_OUT: i32 = 20;
/// How long an action bar message stays up, in ticks.
pub const ACTION_BAR_TICKS: i32 = 60;
/// A text below this alpha (out of 255) is not drawn.
const MIN_VISIBLE_ALPHA: i32 = 8;

/// The title and subtitle shown in the middle of the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleState {
    title: String,
    subtitle: String,
    fade_in: i32,
    stay: i32,
    fade_out: i32,
    /// Ticks left; the title is hidden at zero.
    remaining: i32,
}

impl Default for TitleState {
    fn default() -> Self {
        Self {
            title: String::new(),
            subtitle: String::new(),
            fade_in: DEFAULT_FADE_IN,
            stay: DEFAULT_STAY,
            fade_out: DEFAULT_FADE_OUT,
            remaining: 0,
        }
    }
}

impl TitleState {
    /// Creates a state that shows nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn total(&self) -> i32 {
        self.fade_in + self.stay + self.fade_out
    }

    /// Applies a `Title` packet.
    pub fn apply(&mut self, packet: &Title) {
        match packet {
            Title::Title(json) => {
                self.title.clone_from(json);
                self.remaining = self.total();
            }
            Title::Subtitle(json) => self.subtitle.clone_from(json),
            Title::Times {
                fade_in,
                stay,
                fade_out,
            } => {
                // A negative value means "unchanged" in the reference client.
                if *fade_in >= 0 {
                    self.fade_in = *fade_in;
                }
                if *stay >= 0 {
                    self.stay = *stay;
                }
                if *fade_out >= 0 {
                    self.fade_out = *fade_out;
                }
                if self.remaining > 0 {
                    self.remaining = self.total();
                }
            }
            Title::Clear => self.clear(),
            Title::Reset => {
                self.clear();
                self.fade_in = DEFAULT_FADE_IN;
                self.stay = DEFAULT_STAY;
                self.fade_out = DEFAULT_FADE_OUT;
            }
        }
    }

    fn clear(&mut self) {
        self.title.clear();
        self.subtitle.clear();
        self.remaining = 0;
    }

    /// Advances the timer by one game tick; the texts vanish when it ends.
    pub fn tick(&mut self) {
        if self.remaining > 0 {
            self.remaining -= 1;
            if self.remaining <= 0 {
                self.title.clear();
                self.subtitle.clear();
            }
        }
    }

    /// The title as chat JSON, while it is displayed.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        (self.remaining > 0 && !self.title.is_empty()).then_some(self.title.as_str())
    }

    /// The subtitle as chat JSON, while it is displayed.
    #[must_use]
    pub fn subtitle(&self) -> Option<&str> {
        (self.remaining > 0 && !self.subtitle.is_empty()).then_some(self.subtitle.as_str())
    }

    /// The opacity in `0.0..=1.0` for the current tick and `partial_tick`, or
    /// `0.0` when the text is hidden or too faint to draw.
    #[must_use]
    pub fn alpha(&self, partial_tick: f32) -> f32 {
        if self.remaining <= 0 {
            return 0.0;
        }
        let left = self.remaining as f32 - partial_tick;
        let mut alpha = 255;
        if self.remaining > self.fade_out + self.stay {
            let elapsed = self.total() as f32 - left;
            alpha = (elapsed * 255.0 / self.fade_in.max(1) as f32) as i32;
        }
        if self.remaining <= self.fade_out {
            alpha = (left * 255.0 / self.fade_out.max(1) as f32) as i32;
        }
        let alpha = alpha.clamp(0, 255);
        if alpha > MIN_VISIBLE_ALPHA {
            alpha as f32 / 255.0
        } else {
            0.0
        }
    }
}

/// The one-line message above the hotbar.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActionBar {
    message: String,
    remaining: i32,
}

impl ActionBar {
    /// Creates an empty action bar.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Shows a message for [`ACTION_BAR_TICKS`] ticks.
    pub fn show(&mut self, json: &str) {
        self.message = json.to_string();
        self.remaining = ACTION_BAR_TICKS;
    }

    /// Advances the timer by one game tick.
    pub fn tick(&mut self) {
        if self.remaining > 0 {
            self.remaining -= 1;
        }
    }

    /// The message as chat JSON, while it is displayed.
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        (self.remaining > 0).then_some(self.message.as_str())
    }

    /// The opacity in `0.0..=1.0`: opaque until the last 20 ticks, then fading.
    #[must_use]
    pub fn alpha(&self, partial_tick: f32) -> f32 {
        if self.remaining <= 0 {
            return 0.0;
        }
        let left = self.remaining as f32 - partial_tick;
        let alpha = ((left * 255.0 / 20.0) as i32).clamp(0, 255);
        if alpha > MIN_VISIBLE_ALPHA {
            alpha as f32 / 255.0
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn title(json: &str) -> Title {
        Title::Title(json.to_string())
    }

    fn run(state: &mut TitleState, ticks: i32) {
        for _ in 0..ticks {
            state.tick();
        }
    }

    #[test]
    fn a_title_shows_for_the_default_hundred_ticks_then_vanishes() {
        let mut state = TitleState::new();
        state.apply(&title(r#"{"text":"Hi"}"#));
        assert_eq!(state.title(), Some(r#"{"text":"Hi"}"#));
        run(&mut state, 99);
        assert!(state.title().is_some());
        state.tick();
        assert_eq!(state.title(), None);
        assert_eq!(state.alpha(0.0), 0.0);
    }

    #[test]
    fn the_subtitle_does_not_start_the_timer() {
        let mut state = TitleState::new();
        state.apply(&Title::Subtitle("sub".to_string()));
        assert_eq!(state.subtitle(), None);
        state.apply(&title("t"));
        assert_eq!(state.subtitle(), Some("sub"));
    }

    #[test]
    fn times_change_the_duration_and_restart_a_running_timer() {
        let mut state = TitleState::new();
        state.apply(&title("t"));
        run(&mut state, 50);
        state.apply(&Title::Times {
            fade_in: 0,
            stay: 10,
            fade_out: 0,
        });
        run(&mut state, 9);
        assert!(state.title().is_some());
        state.tick();
        assert_eq!(state.title(), None);
    }

    #[test]
    fn negative_times_keep_the_previous_value() {
        let mut state = TitleState::new();
        state.apply(&Title::Times {
            fade_in: -1,
            stay: 5,
            fade_out: -1,
        });
        state.apply(&title("t"));
        run(&mut state, 34);
        assert!(state.title().is_some());
        state.tick();
        assert_eq!(state.title(), None);
    }

    #[test]
    fn clear_hides_the_title_and_reset_also_restores_the_timings() {
        let mut state = TitleState::new();
        state.apply(&Title::Times {
            fade_in: 0,
            stay: 5,
            fade_out: 0,
        });
        state.apply(&title("t"));
        state.apply(&Title::Clear);
        assert_eq!(state.title(), None);

        state.apply(&title("t"));
        run(&mut state, 5);
        assert_eq!(state.title(), None, "clear keeps the custom timings");

        state.apply(&Title::Reset);
        state.apply(&title("t"));
        run(&mut state, 99);
        assert!(state.title().is_some(), "reset restores 10 + 70 + 20");
    }

    #[test]
    fn the_title_fades_in_stays_opaque_then_fades_out() {
        let mut state = TitleState::new();
        state.apply(&title("t"));
        let early = state.alpha(0.0);
        run(&mut state, 5);
        let mid_fade = state.alpha(0.0);
        run(&mut state, 5);
        let opaque = state.alpha(0.0);
        assert!(early < mid_fade && mid_fade < opaque);
        assert_eq!(opaque, 1.0);

        run(&mut state, 70 + 10);
        let fading = state.alpha(0.0);
        assert!(fading > 0.0 && fading < 1.0, "fading was {fading}");
    }

    #[test]
    fn the_action_bar_shows_for_sixty_ticks_and_fades_in_the_last_twenty() {
        let mut bar = ActionBar::new();
        assert_eq!(bar.message(), None);
        bar.show(r#"{"text":"Now playing"}"#);
        assert_eq!(bar.message(), Some(r#"{"text":"Now playing"}"#));
        assert_eq!(bar.alpha(0.0), 1.0);
        for _ in 0..50 {
            bar.tick();
        }
        let fading = bar.alpha(0.0);
        assert!(fading > 0.0 && fading < 1.0, "fading was {fading}");
        for _ in 0..10 {
            bar.tick();
        }
        assert_eq!(bar.message(), None);
        assert_eq!(bar.alpha(0.0), 0.0);
    }
}

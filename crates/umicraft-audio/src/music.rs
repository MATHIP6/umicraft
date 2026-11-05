// SPDX-License-Identifier: MIT OR Apache-2.0

//! The background music scheduler, a port of MCP `MusicTicker`.
//!
//! It is a pure, tick-driven state machine: the caller feeds it the current
//! [`MusicSituation`], the music volume and whether the track it started is
//! still audible, once per game tick (20 Hz). It answers with a [`MusicUpdate`]
//! (stop the running track, start a new one) and never touches the audio
//! thread itself. Randomness comes from a [`SoundRng`], so it is deterministic.

use crate::category::SoundCategory;
use crate::index::SoundRng;
use crate::request::SoundRequest;

/// Ticks to wait before the very first track (`timeUntilNextMusic = 100`).
const INITIAL_DELAY: i64 = 100;
/// "Nothing scheduled": a track is running (`Integer.MAX_VALUE`).
const PLAYING: i64 = i64::MAX;

/// What the player is doing, `Minecraft.getAmbientMusicType` without the boss
/// bar nuance (the caller picks [`Self::Credits`] itself).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusicSituation {
    /// No world loaded (`music.menu`).
    Menu,
    /// Survival overworld (`music.game`).
    Game,
    /// Creative mode with flight allowed (`music.game.creative`).
    Creative,
    /// In the Nether (`music.game.nether`).
    Nether,
    /// In the End (`music.game.end`).
    End,
    /// The end credits (`music.game.end.credits`), played once.
    Credits,
}

impl MusicSituation {
    /// The `sounds.json` event of this situation's tracks.
    #[must_use]
    pub const fn event(self) -> &'static str {
        match self {
            Self::Menu => "music.menu",
            Self::Game => "music.game",
            Self::Creative => "music.game.creative",
            Self::Nether => "music.game.nether",
            Self::End => "music.game.end",
            Self::Credits => "music.game.end.credits",
        }
    }

    /// The `(min, max)` pause in ticks between two tracks.
    #[must_use]
    pub const fn delay_range(self) -> (u32, u32) {
        match self {
            Self::Menu => (20, 600),
            Self::Game => (12_000, 24_000),
            Self::Creative | Self::Nether => (1_200, 3_600),
            Self::End => (6_000, 24_000),
            Self::Credits => (u32::MAX, u32::MAX),
        }
    }
}

/// What the caller must do after one [`MusicTicker::update`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MusicUpdate {
    /// Stop the track started earlier (the situation changed or the volume is 0).
    pub stop: bool,
    /// Start this track; handle `stop` first.
    pub play: Option<SoundRequest>,
}

/// The music scheduler. See the [module docs](self).
#[derive(Debug, Clone)]
pub struct MusicTicker {
    /// The situation of the track in progress, if one was started.
    current: Option<MusicSituation>,
    /// Ticks until the next track may start; [`PLAYING`] while one runs.
    ticks_until_next: i64,
}

impl Default for MusicTicker {
    fn default() -> Self {
        Self::new()
    }
}

impl MusicTicker {
    /// A ticker that waits 100 ticks before the first track.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            current: None,
            ticks_until_next: INITIAL_DELAY,
        }
    }

    /// Runs one game tick (`MusicTicker.update`).
    ///
    /// `music_volume` is the music category volume with the master volume
    /// applied; at `0` nothing plays and a running track is stopped (the
    /// vanilla sound manager skips silent sounds, which resets the timer the
    /// same way). `track_playing` tells whether the track started earlier is
    /// still audible; it is ignored while none runs.
    pub fn update(
        &mut self,
        situation: MusicSituation,
        music_volume: f32,
        track_playing: bool,
        rng: &mut impl SoundRng,
    ) -> MusicUpdate {
        let mut update = MusicUpdate::default();
        if music_volume <= 0.0 {
            update.stop = self.stop();
            return update;
        }
        let (min, max) = situation.delay_range();
        if let Some(current) = self.current {
            if current != situation {
                update.stop = true;
                self.ticks_until_next = random_in(rng, 0, min / 2);
            }
            if update.stop || !track_playing {
                self.current = None;
                // `min` with the pending value keeps a short switch delay.
                self.ticks_until_next = self.ticks_until_next.min(random_in(rng, min, max));
            }
        }
        if self.current.is_none() {
            let due = self.ticks_until_next <= 0;
            self.ticks_until_next = self.ticks_until_next.saturating_sub(1);
            if due {
                update.play = Some(self.start(situation));
            }
        }
        update
    }

    /// Stops the running track at once and allows the next one immediately
    /// (`func_181557_a`, used when a world closes). Returns whether one ran.
    pub fn stop(&mut self) -> bool {
        let was_playing = self.current.take().is_some();
        if was_playing {
            self.ticks_until_next = 0;
        }
        was_playing
    }

    fn start(&mut self, situation: MusicSituation) -> SoundRequest {
        self.current = Some(situation);
        self.ticks_until_next = PLAYING;
        let mut request = SoundRequest::new(situation.event());
        request.category = Some(SoundCategory::Music);
        request
    }
}

/// `MathHelper.getRandomIntegerInRange`: `min` when the range is empty.
fn random_in(rng: &mut impl SoundRng, min: u32, max: u32) -> i64 {
    if min >= max {
        return i64::from(min);
    }
    // `max - min + 1` overflows only for the full u32 range, which no type uses.
    i64::from(min) + i64::from(rng.next_below(max - min + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::SeededRng;

    /// Returns `value` clamped into the bound, to test exact edge values.
    struct Const(u32);

    impl SoundRng for Const {
        fn next_below(&mut self, bound: u32) -> u32 {
            self.0.min(bound - 1)
        }
    }

    use MusicSituation::{Creative, Credits, Game, Menu};

    /// Ticks (without a running track) until a track starts.
    fn run_until_play(
        ticker: &mut MusicTicker,
        situation: MusicSituation,
        rng: &mut impl SoundRng,
    ) -> u32 {
        for tick in 0..100_000 {
            if ticker.update(situation, 1.0, false, rng).play.is_some() {
                return tick;
            }
        }
        panic!("no track started");
    }

    #[test]
    fn first_track_starts_after_the_initial_delay() {
        let mut ticker = MusicTicker::new();
        assert_eq!(run_until_play(&mut ticker, Menu, &mut Const(0)), 100);
    }

    #[test]
    fn request_names_the_event_in_the_music_category() {
        let mut ticker = MusicTicker::new();
        let mut rng = Const(0);
        let play = loop {
            if let Some(play) = ticker.update(Game, 1.0, false, &mut rng).play {
                break play;
            }
        };
        assert_eq!(&*play.event, "music.game");
        assert_eq!(play.category, Some(SoundCategory::Music));
    }

    #[test]
    fn running_track_is_left_alone() {
        let mut ticker = MusicTicker::new();
        let mut rng = Const(0);
        run_until_play(&mut ticker, Menu, &mut rng);
        for _ in 0..1_000 {
            let update = ticker.update(Menu, 1.0, true, &mut rng);
            assert_eq!(update, MusicUpdate::default());
        }
    }

    #[test]
    fn finished_track_waits_a_delay_within_bounds() {
        for seed in 0..20 {
            let mut ticker = MusicTicker::new();
            let mut rng = SeededRng::new(seed);
            run_until_play(&mut ticker, Game, &mut rng);
            // The track ends: the first call draws the delay (the pending
            // value is the "playing" sentinel, so the draw wins in full).
            let wait = run_until_play(&mut ticker, Game, &mut rng);
            assert!((12_000..=24_000).contains(&wait), "{wait}");
        }
    }

    #[test]
    fn menu_to_game_stops_and_shortens_the_wait() {
        let mut ticker = MusicTicker::new();
        let mut rng = Const(u32::MAX);
        run_until_play(&mut ticker, Menu, &mut rng);
        let update = ticker.update(Game, 1.0, true, &mut rng);
        assert!(update.stop);
        // The switch draws random(0, 6000), smaller than the finished-track draw.
        let wait = run_until_play(&mut ticker, Game, &mut rng);
        assert!(wait <= 6_000, "{wait}");
    }

    #[test]
    fn menu_to_game_with_minimal_draw_restarts_at_once() {
        let mut ticker = MusicTicker::new();
        let mut rng = Const(0);
        run_until_play(&mut ticker, Menu, &mut rng);
        let update = ticker.update(Game, 1.0, true, &mut rng);
        assert!(update.stop);
        assert_eq!(update.play.map(|r| r.event), Some("music.game".into()));
    }

    #[test]
    fn volume_zero_never_plays_and_stops_the_track() {
        let mut ticker = MusicTicker::new();
        let mut rng = Const(0);
        for _ in 0..500 {
            let update = ticker.update(Menu, 0.0, false, &mut rng);
            assert_eq!(update, MusicUpdate::default());
        }
        run_until_play(&mut ticker, Menu, &mut rng);
        assert!(ticker.update(Menu, 0.0, true, &mut rng).stop);
        // Raising the volume again starts music at once.
        assert!(ticker.update(Menu, 1.0, false, &mut rng).play.is_some());
    }

    #[test]
    fn explicit_stop_allows_immediate_restart() {
        let mut ticker = MusicTicker::new();
        let mut rng = Const(0);
        assert!(!ticker.stop());
        run_until_play(&mut ticker, Creative, &mut rng);
        assert!(ticker.stop());
        assert!(ticker.update(Creative, 1.0, false, &mut rng).play.is_some());
    }

    #[test]
    fn credits_never_repeat_and_have_their_event() {
        assert_eq!(Credits.delay_range(), (u32::MAX, u32::MAX));
        assert_eq!(Credits.event(), "music.game.end.credits");
        let mut ticker = MusicTicker::new();
        let mut rng = Const(0);
        run_until_play(&mut ticker, Credits, &mut rng);
        let end = ticker.update(Credits, 1.0, false, &mut rng);
        assert_eq!(end, MusicUpdate::default());
    }

    #[test]
    fn random_in_covers_both_ends() {
        assert_eq!(random_in(&mut Const(0), 5, 9), 5);
        assert_eq!(random_in(&mut Const(u32::MAX), 5, 9), 9);
        assert_eq!(random_in(&mut Const(0), 7, 7), 7);
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The background music, driven at 20 Hz from the render thread.
//!
//! Vanilla calls `MusicTicker.update` once per client tick from
//! `Minecraft.runTick`, with `getAmbientMusicType` choosing the situation. The
//! scheduler itself is `umicraft_audio::music::MusicTicker`; this module only
//! feeds it: it turns the frame clock into ticks, the world facts into a
//! [`MusicSituation`], and the answer into non-blocking `AudioHandle` calls.
//! Whether the track is still audible comes from `AudioHandle::music_playing`,
//! an atomic the audio thread keeps current, so nothing here waits.

use std::time::{Duration, Instant};

use umicraft_audio::music::{MusicSituation, MusicTicker};
use umicraft_audio::{AudioHandle, SeededRng, SoundCategory};
use umicraft_game::gamemode::GameMode;

use super::AudioBridge;
use crate::options::SoundOptions;

/// One client tick.
const TICK: Duration = Duration::from_millis(50);
/// The most ticks run for one call: after a long stall (a dragged window) the
/// timer should not fast-forward by minutes.
const MAX_TICKS_PER_CALL: u32 = 4;

/// Where the player is, as far as the music cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusicWorld {
    /// No world is loaded: title, server list, connecting.
    Menu,
    /// In a world, in `dimension` (`0` overworld, `-1` Nether, `1` End).
    Playing {
        /// The dimension id.
        dimension: i8,
        /// The game mode of the local player.
        gamemode: GameMode,
    },
}

impl MusicWorld {
    /// The situation (`Minecraft.getAmbientMusicType`). Creative music needs a
    /// creative player; a spectator also flies but hears the survival music, as
    /// `isCreativeMode` is false for him.
    #[must_use]
    pub fn situation(self) -> MusicSituation {
        match self {
            Self::Menu => MusicSituation::Menu,
            Self::Playing { dimension: -1, .. } => MusicSituation::Nether,
            Self::Playing { dimension: 1, .. } => MusicSituation::End,
            Self::Playing {
                gamemode: GameMode::Creative,
                ..
            } => MusicSituation::Creative,
            Self::Playing { .. } => MusicSituation::Game,
        }
    }
}

/// The ticker plus the clock that paces it.
#[derive(Debug)]
pub struct MusicDriver {
    ticker: MusicTicker,
    rng: SeededRng,
    next_tick: Option<Instant>,
}

impl Default for MusicDriver {
    fn default() -> Self {
        Self::new(1)
    }
}

impl MusicDriver {
    /// A driver whose track picks follow `seed`.
    #[must_use]
    pub fn new(seed: u32) -> Self {
        Self {
            ticker: MusicTicker::new(),
            rng: SeededRng::new(seed),
            next_tick: None,
        }
    }

    /// How many 20 Hz ticks are due at `now`; the first call starts the clock.
    fn due_ticks(&mut self, now: Instant) -> u32 {
        let mut next = *self.next_tick.get_or_insert(now);
        let mut due = 0;
        while next <= now && due < MAX_TICKS_PER_CALL {
            next += TICK;
            due += 1;
        }
        // Past the cap the clock restarts instead of replaying the backlog.
        self.next_tick = Some(if next <= now { now + TICK } else { next });
        due
    }

    /// Runs the ticks due at `now` and applies their answers to `handle`.
    ///
    /// `music_volume` is the master volume times the music slider.
    pub fn update(
        &mut self,
        now: Instant,
        world: MusicWorld,
        music_volume: f32,
        handle: &AudioHandle,
    ) {
        for _ in 0..self.due_ticks(now) {
            let update = self.ticker.update(
                world.situation(),
                music_volume,
                handle.music_playing(),
                &mut self.rng,
            );
            if update.stop {
                handle.stop_music();
            }
            if let Some(request) = update.play {
                handle.play_music(request);
            }
        }
    }
}

impl AudioBridge {
    /// Feeds the music scheduler; call it every frame, it paces itself at 20 Hz.
    /// Does nothing on a silent bridge.
    pub fn update_music(&mut self, now: Instant, world: MusicWorld, options: &SoundOptions) {
        let Some(handle) = &self.handle else {
            return;
        };
        let volume = options.volume(SoundCategory::Master) * options.volume(SoundCategory::Music);
        self.music.update(now, world, volume, handle);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::Receiver;

    use umicraft_audio::{channel, AudioMessage};

    use super::*;

    const GAME: MusicWorld = MusicWorld::Playing {
        dimension: 0,
        gamemode: GameMode::Survival,
    };

    /// Runs the driver for `ticks` ticks of 50 ms starting at `*clock`.
    fn run_on(
        driver: &mut MusicDriver,
        clock: &mut Instant,
        ticks: u32,
        world: MusicWorld,
        handle: &AudioHandle,
    ) {
        for _ in 0..ticks {
            driver.update(*clock, world, 1.0, handle);
            *clock += TICK;
        }
    }

    fn messages(receiver: &Receiver<AudioMessage>) -> Vec<String> {
        receiver
            .try_iter()
            .map(|message| match message {
                AudioMessage::Play(request) => format!("play {}", request.event),
                AudioMessage::StopMusic => "stop".to_owned(),
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn situations_follow_get_ambient_music_type() {
        let in_world = |dimension, gamemode| {
            MusicWorld::Playing {
                dimension,
                gamemode,
            }
            .situation()
        };
        assert_eq!(MusicWorld::Menu.situation(), MusicSituation::Menu);
        assert_eq!(in_world(0, GameMode::Survival), MusicSituation::Game);
        assert_eq!(in_world(0, GameMode::Adventure), MusicSituation::Game);
        assert_eq!(in_world(0, GameMode::Spectator), MusicSituation::Game);
        assert_eq!(in_world(0, GameMode::Creative), MusicSituation::Creative);
        assert_eq!(in_world(-1, GameMode::Creative), MusicSituation::Nether);
        assert_eq!(in_world(1, GameMode::Survival), MusicSituation::End);
    }

    #[test]
    fn the_clock_yields_one_tick_per_50_ms_and_caps_a_backlog() {
        let mut driver = MusicDriver::default();
        let start = Instant::now();
        assert_eq!(
            driver.due_ticks(start),
            1,
            "the first call starts the clock"
        );
        assert_eq!(driver.due_ticks(start + Duration::from_millis(10)), 0);
        assert_eq!(driver.due_ticks(start + Duration::from_millis(60)), 1);
        assert_eq!(driver.due_ticks(start + Duration::from_millis(210)), 3);
        let stalled = start + Duration::from_secs(60);
        assert_eq!(driver.due_ticks(stalled), MAX_TICKS_PER_CALL);
        assert_eq!(driver.due_ticks(stalled + Duration::from_millis(10)), 0);
    }

    #[test]
    fn menu_music_starts_after_the_initial_delay() {
        let (handle, receiver) = channel(8);
        let mut driver = MusicDriver::default();
        let mut clock = Instant::now();
        run_on(&mut driver, &mut clock, 100, MusicWorld::Menu, &handle);
        assert!(messages(&receiver).is_empty(), "100 ticks of silence first");
        run_on(&mut driver, &mut clock, 2, MusicWorld::Menu, &handle);
        assert_eq!(messages(&receiver), ["play music.menu"]);
        assert!(handle.music_playing(), "the track counts at once");
    }

    #[test]
    fn a_running_track_is_not_restarted_and_a_new_world_stops_it() {
        let (handle, receiver) = channel(8);
        let mut driver = MusicDriver::default();
        let mut clock = Instant::now();
        run_on(&mut driver, &mut clock, 105, MusicWorld::Menu, &handle);
        assert_eq!(messages(&receiver), ["play music.menu"]);
        run_on(&mut driver, &mut clock, 500, MusicWorld::Menu, &handle);
        assert!(
            messages(&receiver).is_empty(),
            "still playing, nothing to do"
        );
        run_on(&mut driver, &mut clock, 1, GAME, &handle);
        assert_eq!(messages(&receiver), ["stop"]);
    }

    #[test]
    fn zero_music_volume_never_plays() {
        let (handle, receiver) = channel(8);
        let mut driver = MusicDriver::default();
        let mut clock = Instant::now();
        for _ in 0..300 {
            driver.update(clock, MusicWorld::Menu, 0.0, &handle);
            clock += TICK;
        }
        assert!(messages(&receiver).is_empty());
    }

    #[test]
    fn a_silent_bridge_ignores_the_music() {
        let mut bridge = AudioBridge::disabled();
        bridge.update_music(Instant::now(), MusicWorld::Menu, &SoundOptions::default());
    }

    #[test]
    fn the_bridge_multiplies_master_and_music_sliders() {
        let (handle, receiver) = channel(8);
        let mut bridge = AudioBridge::with_handle(handle);
        let options = SoundOptions {
            master: 0.0,
            ..SoundOptions::default()
        };
        let mut clock = Instant::now();
        for _ in 0..300 {
            bridge.update_music(clock, MusicWorld::Menu, &options);
            clock += TICK;
        }
        assert!(messages(&receiver).is_empty(), "master 0 mutes the music");
    }
}

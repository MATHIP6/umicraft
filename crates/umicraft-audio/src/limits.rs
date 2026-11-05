// SPDX-License-Identifier: MIT OR Apache-2.0

//! How many sounds play at once, and sounds that start later.
//!
//! Vanilla 1.8 opens its sound system with the paulscode defaults of 28 normal
//! and 4 streaming channels (`SoundSystemConfig`, which `SoundManager` leaves
//! unchanged); a sound that finds no free channel is dropped.
//! `playDelayedSound` stores a sound with the tick at which
//! it starts and `updateAllSounds` plays it once `playTime >= start`.

/// The channel count of the sound system.
pub const NORMAL_SOURCES: usize = 28;
/// The streaming channel count (music and long records).
pub const STREAMING_SOURCES: usize = 4;

/// Counts the busy sources of both kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLimit {
    normal: usize,
    streaming: usize,
    max_normal: usize,
    max_streaming: usize,
}

impl Default for SourceLimit {
    fn default() -> Self {
        Self::new(NORMAL_SOURCES, STREAMING_SOURCES)
    }
}

impl SourceLimit {
    /// A limiter with the given capacities.
    #[must_use]
    pub const fn new(max_normal: usize, max_streaming: usize) -> Self {
        Self {
            normal: 0,
            streaming: 0,
            max_normal,
            max_streaming,
        }
    }

    /// Reserves a source; `false` when none is free (the sound is dropped).
    pub fn try_acquire(&mut self, streaming: bool) -> bool {
        let (busy, max) = self.counter(streaming);
        if *busy >= max {
            return false;
        }
        *busy += 1;
        true
    }

    /// Gives a source back; extra releases are ignored.
    pub fn release(&mut self, streaming: bool) {
        let (busy, _) = self.counter(streaming);
        *busy = busy.saturating_sub(1);
    }

    /// The number of busy sources of a kind.
    #[must_use]
    pub const fn busy(&self, streaming: bool) -> usize {
        if streaming {
            self.streaming
        } else {
            self.normal
        }
    }

    fn counter(&mut self, streaming: bool) -> (&mut usize, usize) {
        if streaming {
            (&mut self.streaming, self.max_streaming)
        } else {
            (&mut self.normal, self.max_normal)
        }
    }
}

/// Sounds waiting for their start tick (`SoundManager.delayedSounds`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelayedSounds<T> {
    now: u64,
    waiting: Vec<(u64, T)>,
}

impl<T> Default for DelayedSounds<T> {
    fn default() -> Self {
        Self {
            now: 0,
            waiting: Vec::new(),
        }
    }
}

impl<T> DelayedSounds<T> {
    /// Stores `item` to start `delay` ticks from now.
    pub fn schedule(&mut self, item: T, delay: u32) {
        self.waiting.push((self.now + u64::from(delay), item));
    }

    /// Advances one tick and hands every due item, in scheduling order, to `play`.
    pub fn tick(&mut self, mut play: impl FnMut(T)) {
        self.now += 1;
        let now = self.now;
        let mut index = 0;
        while index < self.waiting.len() {
            if self.waiting[index].0 <= now {
                play(self.waiting.remove(index).1);
            } else {
                index += 1;
            }
        }
    }

    /// The number of sounds still waiting.
    #[must_use]
    pub fn len(&self) -> usize {
        self.waiting.len()
    }

    /// Whether no sound is waiting.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.waiting.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limit_drops_the_sound_after_the_capacity() {
        let mut limit = SourceLimit::new(2, 1);
        assert!(limit.try_acquire(false));
        assert!(limit.try_acquire(false));
        assert!(!limit.try_acquire(false));
        assert!(limit.try_acquire(true), "streaming has its own channels");
        assert!(!limit.try_acquire(true));
    }

    #[test]
    fn release_frees_a_slot_and_never_underflows() {
        let mut limit = SourceLimit::new(1, 1);
        assert!(limit.try_acquire(false));
        limit.release(false);
        limit.release(false);
        assert_eq!(limit.busy(false), 0);
        assert!(limit.try_acquire(false));
    }

    #[test]
    fn defaults_are_the_paulscode_channels() {
        let mut limit = SourceLimit::default();
        for _ in 0..NORMAL_SOURCES {
            assert!(limit.try_acquire(false));
        }
        assert!(!limit.try_acquire(false));
    }

    #[test]
    fn delayed_sounds_start_on_their_tick() {
        let mut delayed = DelayedSounds::default();
        delayed.schedule("a", 2);
        delayed.schedule("b", 1);
        let mut played = Vec::new();
        delayed.tick(|s| played.push(s));
        assert_eq!(played, ["b"]);
        delayed.tick(|s| played.push(s));
        assert_eq!(played, ["b", "a"]);
        assert!(delayed.is_empty());
    }

    #[test]
    fn a_zero_delay_plays_on_the_next_tick() {
        let mut delayed = DelayedSounds::default();
        delayed.schedule(1, 0);
        let mut played = Vec::new();
        delayed.tick(|s| played.push(s));
        assert_eq!(played, [1]);
    }
}

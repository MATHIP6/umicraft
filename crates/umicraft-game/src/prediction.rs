// SPDX-License-Identifier: MIT OR Apache-2.0

//! The pending-input history used to reconcile the client with the server.
//!
//! The client predicts its own movement, but the server owns the truth. When
//! the server sends an authoritative position, the client has to rebuild the
//! trajectory it missed: it restores the state of the tick the correction
//! refers to and replays the inputs that came after it.
//!
//! The history is a fixed capacity ring buffer: recording a tick never
//! allocates, so the prediction stays usable in the 20 Hz hot path.

use glam::DVec3;

use crate::input::InputIntent;

/// The number of pending ticks the history keeps.
///
/// A round trip of a few hundred milliseconds is a handful of ticks, so 128
/// ticks (6.4 s) is generous.
pub const HISTORY_CAPACITY: usize = 128;

/// One recorded tick: the intent that was applied and the state it produced.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PendingTick {
    /// The intent that was applied.
    pub intent: InputIntent,
    /// The player position after the tick.
    pub position: DVec3,
    /// The player velocity after the tick.
    pub velocity: DVec3,
    /// Whether the player was on the ground after the tick.
    pub on_ground: bool,
    /// The horizontal view angle after the tick, in radians.
    pub yaw: f32,
    /// The vertical view angle after the tick, in radians.
    pub pitch: f32,
}

impl Default for PendingTick {
    fn default() -> Self {
        Self {
            intent: InputIntent::default(),
            position: DVec3::ZERO,
            velocity: DVec3::ZERO,
            on_ground: false,
            yaw: 0.0,
            pitch: 0.0,
        }
    }
}

/// A fixed capacity ring buffer of pending ticks.
///
/// The oldest tick is at index 0, the newest at index [`PredictionHistory::len`] - 1.
/// When the buffer is full the oldest tick is dropped.
#[derive(Debug, Clone)]
pub struct PredictionHistory {
    entries: [PendingTick; HISTORY_CAPACITY],
    len: usize,
    head: usize,
}

impl Default for PredictionHistory {
    fn default() -> Self {
        Self {
            entries: [PendingTick::default(); HISTORY_CAPACITY],
            len: 0,
            head: 0,
        }
    }
}

impl PredictionHistory {
    /// Creates an empty history.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a tick; the oldest entry is dropped when the buffer is full.
    pub fn push(&mut self, tick: PendingTick) {
        if self.len < HISTORY_CAPACITY {
            self.entries[(self.head + self.len) % HISTORY_CAPACITY] = tick;
            self.len += 1;
        } else {
            self.entries[self.head] = tick;
            self.head = (self.head + 1) % HISTORY_CAPACITY;
        }
    }

    /// Forgets every recorded tick.
    pub fn clear(&mut self) {
        self.len = 0;
        self.head = 0;
    }

    /// The number of recorded ticks.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// True when no tick is recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The recorded tick at `index`, counting from the oldest one.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&PendingTick> {
        if index < self.len {
            Some(&self.entries[(self.head + index) % HISTORY_CAPACITY])
        } else {
            None
        }
    }

    /// The recorded ticks, oldest first.
    pub fn iter(&self) -> impl Iterator<Item = &PendingTick> {
        (0..self.len).filter_map(move |index| self.get(index))
    }

    /// The index of the recorded tick whose position is closest to
    /// `position`.
    ///
    /// The search returns `None` when no recorded tick is within `tolerance`
    /// of the given position, which means the correction does not refer to
    /// any of the pending ticks. Ties are won by the newest tick.
    #[must_use]
    pub fn find_base(&self, position: DVec3, tolerance: f64) -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for (index, tick) in self.iter().enumerate() {
            let distance = (tick.position - position).length();
            if distance <= tolerance && best.is_none_or(|(_, best)| distance < best) {
                best = Some((index, distance));
            }
        }
        best.map(|(index, _)| index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick(position: DVec3) -> PendingTick {
        PendingTick {
            position,
            ..PendingTick::default()
        }
    }

    #[test]
    fn the_history_keeps_the_ticks_in_order() {
        let mut history = PredictionHistory::new();
        for index in 0..5 {
            history.push(tick(DVec3::new(index as f64, 0.0, 0.0)));
        }
        assert_eq!(history.len(), 5);
        assert_eq!(history.get(0).unwrap().position.x, 0.0);
        assert_eq!(history.get(4).unwrap().position.x, 4.0);
        assert!(history.get(5).is_none());
    }

    #[test]
    fn the_history_drops_the_oldest_tick_when_it_is_full() {
        let mut history = PredictionHistory::new();
        for index in 0..HISTORY_CAPACITY + 3 {
            history.push(tick(DVec3::new(index as f64, 0.0, 0.0)));
        }
        assert_eq!(history.len(), HISTORY_CAPACITY);
        assert_eq!(history.get(0).unwrap().position.x, 3.0);
        assert_eq!(
            history.get(HISTORY_CAPACITY - 1).unwrap().position.x,
            (HISTORY_CAPACITY + 2) as f64
        );
    }

    #[test]
    fn the_base_is_the_closest_recorded_tick() {
        let mut history = PredictionHistory::new();
        for index in 0..5 {
            history.push(tick(DVec3::new(index as f64, 0.0, 0.0)));
        }
        assert_eq!(history.find_base(DVec3::new(2.1, 0.0, 0.0), 0.5), Some(2));
        assert_eq!(history.find_base(DVec3::new(2.0, 0.0, 0.0), 0.5), Some(2));
        assert_eq!(history.find_base(DVec3::new(20.0, 0.0, 0.0), 0.5), None);
    }

    #[test]
    fn clearing_empties_the_history() {
        let mut history = PredictionHistory::new();
        history.push(tick(DVec3::ZERO));
        history.clear();
        assert!(history.is_empty());
        assert!(history.get(0).is_none());
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The weighted random pick of an event (`SoundEventAccessorComposite.cloneEntry`).
//!
//! The total weight of an event is the sum of its entries; one random integer
//! below it is drawn and the entry weights are subtracted in order until the
//! value goes negative. An `event` entry weighs as much as the event it names
//! (`SoundHandler`'s anonymous accessor) and is resolved recursively.

use super::{EventSounds, PickedSound, SoundEntry, SoundIndex};

/// How deep `event` entries may nest. Vanilla would overflow the stack on a
/// cycle; here the pick gives up instead.
const MAX_DEPTH: u32 = 8;

/// A source of random integers, injectable so tests are deterministic.
pub trait SoundRng {
    /// A uniform integer in `0..bound`; `bound` is never `0`.
    fn next_below(&mut self, bound: u32) -> u32;
}

/// A small seeded generator (SplitMix64), the default [`SoundRng`].
#[derive(Debug, Clone)]
pub struct SeededRng(u64);

impl SeededRng {
    /// A generator that always yields the same sequence for the same seed.
    #[must_use]
    pub const fn new(seed: u32) -> Self {
        Self(seed as u64)
    }
}

impl SoundRng for SeededRng {
    fn next_below(&mut self, bound: u32) -> u32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        // The remainder is below `bound`, so it fits in u32.
        (z % u64::from(bound)) as u32
    }
}

impl SoundIndex {
    /// Picks one sound of `event`; `None` when the event is unknown or has no
    /// playable entry (`SoundHandler.missing_sound`).
    pub fn pick(&self, event: &str, rng: &mut impl SoundRng) -> Option<PickedSound> {
        let sounds = self.event(event)?;
        let mut picked = self.pick_in(sounds, rng, MAX_DEPTH)?;
        picked.category = sounds.category;
        Some(picked)
    }

    fn pick_in(
        &self,
        sounds: &EventSounds,
        rng: &mut impl SoundRng,
        depth: u32,
    ) -> Option<PickedSound> {
        let total = self.total_weight(sounds, depth);
        if total == 0 {
            return None;
        }
        let mut remaining = i64::from(rng.next_below(total));
        for entry in &sounds.entries {
            remaining -= i64::from(self.entry_weight(entry, depth));
            if remaining < 0 {
                return self.pick_entry(entry, sounds, rng, depth);
            }
        }
        None
    }

    fn pick_entry(
        &self,
        entry: &SoundEntry,
        owner: &EventSounds,
        rng: &mut impl SoundRng,
        depth: u32,
    ) -> Option<PickedSound> {
        match entry {
            SoundEntry::File {
                path,
                volume,
                pitch,
                stream,
                ..
            } => Some(PickedSound {
                path: path.clone(),
                volume: *volume,
                pitch: *pitch,
                stream: *stream,
                category: owner.category,
            }),
            SoundEntry::Event { event } => {
                let nested = self.event(event)?;
                self.pick_in(nested, rng, depth.checked_sub(1)?)
            }
        }
    }

    fn total_weight(&self, sounds: &EventSounds, depth: u32) -> u32 {
        sounds
            .entries
            .iter()
            .map(|entry| self.entry_weight(entry, depth))
            .fold(0, u32::saturating_add)
    }

    fn entry_weight(&self, entry: &SoundEntry, depth: u32) -> u32 {
        match entry {
            SoundEntry::File { weight, .. } => *weight,
            SoundEntry::Event { event } => match (self.event(event), depth.checked_sub(1)) {
                (Some(nested), Some(depth)) => self.total_weight(nested, depth),
                _ => 0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::SoundCategory;

    /// Replays fixed values in order (one per draw, so a nested event takes a
    /// second one, as in vanilla), to test the subtraction walk exactly.
    struct Fixed(Vec<u32>);

    impl SoundRng for Fixed {
        fn next_below(&mut self, bound: u32) -> u32 {
            let value = self.0.remove(0);
            assert!(value < bound, "{value} out of 0..{bound}");
            value
        }
    }

    fn index() -> SoundIndex {
        SoundIndex::parse(
            r#"{
            "e": {"category": "block", "sounds": [
                {"name": "a", "weight": 3, "volume": 0.5},
                {"name": "b", "pitch": 2.0},
                {"name": "other", "type": "event"}
            ]},
            "other": {"category": "player", "sounds": [{"name": "c", "weight": 2}]},
            "loop": {"sounds": [{"name": "loop", "type": "event"}]},
            "empty": {"sounds": []}
        }"#,
        )
        .expect("valid")
    }

    #[test]
    fn walks_the_weights_in_order() {
        let index = index();
        // Total weight 3 + 1 + 2 = 6: 0..3 -> a, 3 -> b, 4..6 -> c.
        let path = |draws: &[u32]| {
            index
                .pick("e", &mut Fixed(draws.to_vec()))
                .expect("picks")
                .path
        };
        assert_eq!(path(&[0]), "a");
        assert_eq!(path(&[2]), "a");
        assert_eq!(path(&[3]), "b");
        // The nested event "other" (weight 2, only "c") draws again.
        assert_eq!(path(&[4, 0]), "c");
        assert_eq!(path(&[5, 1]), "c");
    }

    #[test]
    fn keeps_the_entry_values_and_the_starting_category() {
        let index = index();
        let a = index.pick("e", &mut Fixed(vec![0])).expect("picks");
        assert_eq!((a.volume, a.pitch, a.stream), (0.5, 1.0, false));
        assert_eq!(a.resource_path(), "sounds/a.ogg");
        let c = index.pick("e", &mut Fixed(vec![5, 0])).expect("picks");
        assert_eq!(c.category, SoundCategory::Blocks);
    }

    #[test]
    fn unknown_empty_and_cyclic_events_pick_nothing() {
        let index = index();
        let mut rng = SeededRng::new(1);
        assert!(index.pick("missing", &mut rng).is_none());
        assert!(index.pick("empty", &mut rng).is_none());
        assert!(index.pick("loop", &mut rng).is_none());
    }

    #[test]
    fn seeded_picks_are_deterministic_and_follow_the_weights() {
        let index = index();
        let draw = |seed| {
            let mut rng = SeededRng::new(seed);
            (0..600)
                .map(|_| index.pick("e", &mut rng).expect("picks").path)
                .collect::<Vec<_>>()
        };
        let first = draw(7);
        assert_eq!(first, draw(7));
        let count = |name: &str| first.iter().filter(|p| *p == name).count();
        // Expected 300 / 100 / 200 of 600.
        assert!((240..360).contains(&count("a")), "a {}", count("a"));
        assert!((50..150).contains(&count("b")), "b {}", count("b"));
        assert!((150..250).contains(&count("c")), "c {}", count("c"));
    }
}

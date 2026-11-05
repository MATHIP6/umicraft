// SPDX-License-Identifier: MIT OR Apache-2.0

//! A bounded least-recently-used cache of decoded sounds.
//!
//! The bound is in bytes of PCM, not in entries, because a music track is
//! thousands of times larger than a footstep. Sounds are `Arc`-shared, so
//! evicting one never cuts a voice that still plays it.

use std::collections::HashMap;

use crate::decode::DecodedSound;

/// The default bound: 64 MiB of PCM.
pub const DEFAULT_CAPACITY_BYTES: usize = 64 * 1024 * 1024;

/// Decoded sounds keyed by resource path, evicted least recently used first.
#[derive(Debug)]
pub struct SoundCache {
    capacity: usize,
    used: usize,
    tick: u64,
    entries: HashMap<String, (u64, DecodedSound)>,
}

impl Default for SoundCache {
    fn default() -> Self {
        Self::new(DEFAULT_CAPACITY_BYTES)
    }
}

impl SoundCache {
    /// A cache holding at most `capacity` bytes of PCM.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            used: 0,
            tick: 0,
            entries: HashMap::new(),
        }
    }

    /// The cached sound for `key`, marking it as just used.
    pub fn get(&mut self, key: &str) -> Option<DecodedSound> {
        self.tick += 1;
        let tick = self.tick;
        let (last_used, sound) = self.entries.get_mut(key)?;
        *last_used = tick;
        Some(sound.clone())
    }

    /// Stores a sound, evicting the least recently used ones until it fits.
    /// A sound larger than the whole cache is not stored.
    pub fn insert(&mut self, key: String, sound: DecodedSound) {
        let size = sound.byte_size();
        if size > self.capacity {
            return;
        }
        if let Some((_, old)) = self.entries.remove(&key) {
            self.used -= old.byte_size();
        }
        while self.used + size > self.capacity {
            if !self.evict_oldest() {
                break;
            }
        }
        self.tick += 1;
        self.used += size;
        self.entries.insert(key, (self.tick, sound));
    }

    /// The bytes of PCM currently held.
    #[must_use]
    pub const fn used_bytes(&self) -> usize {
        self.used
    }

    /// The number of cached sounds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn evict_oldest(&mut self) -> bool {
        let Some(oldest) = self
            .entries
            .iter()
            .min_by_key(|(_, (used, _))| *used)
            .map(|(key, _)| key.clone())
        else {
            return false;
        };
        if let Some((_, sound)) = self.entries.remove(&oldest) {
            self.used -= sound.byte_size();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sound of `frames` frames, `frames * 8` bytes.
    fn sound(frames: usize) -> DecodedSound {
        DecodedSound {
            sample_rate: 44_100,
            frames: vec![[0.0, 0.0]; frames].into(),
        }
    }

    #[test]
    fn stores_and_returns_sounds() {
        let mut cache = SoundCache::new(1_000);
        cache.insert("a".into(), sound(10));
        assert_eq!(cache.get("a").expect("cached").frames.len(), 10);
        assert!(cache.get("b").is_none());
        assert_eq!(cache.used_bytes(), 80);
    }

    #[test]
    fn evicts_the_least_recently_used() {
        let mut cache = SoundCache::new(200); // two 10-frame sounds fit, three do not
        cache.insert("a".into(), sound(10));
        cache.insert("b".into(), sound(10));
        cache.get("a"); // b is now the oldest
        cache.insert("c".into(), sound(10));
        assert!(cache.get("b").is_none());
        assert!(cache.get("a").is_some());
        assert!(cache.get("c").is_some());
        assert!(cache.used_bytes() <= 200);
    }

    #[test]
    fn replacing_a_key_does_not_leak_bytes() {
        let mut cache = SoundCache::new(1_000);
        cache.insert("a".into(), sound(10));
        cache.insert("a".into(), sound(20));
        assert_eq!((cache.len(), cache.used_bytes()), (1, 160));
    }

    #[test]
    fn a_sound_larger_than_the_cache_is_not_kept() {
        let mut cache = SoundCache::new(50);
        cache.insert("big".into(), sound(10));
        assert!(cache.is_empty());
    }
}

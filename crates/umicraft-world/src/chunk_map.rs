// SPDX-License-Identifier: MIT OR Apache-2.0

//! Maps and sets keyed by [`ChunkPos`] with a cheap hasher (F2-4e).
//!
//! The standard `HashMap` hashes with SipHash, which is built to resist
//! chosen-key attacks and costs about ten times more than a chunk position
//! needs. The renderer, the world and the mesh queue look chunks up many times per
//! frame. [`ChunkHasher`] mixes the two coordinates with a multiply and a
//! finalizer. It is **not** collision-resistant: use it only for chunk
//! positions, whose worst case (a hostile server choosing positions that
//! collide) costs its own client some speed and nothing else.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

use crate::ChunkPos;

/// A `HashMap` keyed by chunk position.
pub type ChunkMap<V> = HashMap<ChunkPos, V, BuildHasherDefault<ChunkHasher>>;
/// A `HashSet` of chunk positions.
pub type ChunkSet = HashSet<ChunkPos, BuildHasherDefault<ChunkHasher>>;

/// The odd constant of the 64-bit golden ratio, used by FxHash and others.
const MULTIPLIER: u64 = 0x9E37_79B9_7F4A_7C15;

/// A fast hasher for small integer keys.
#[derive(Debug, Default, Clone, Copy)]
pub struct ChunkHasher(u64);

impl ChunkHasher {
    fn mix(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(MULTIPLIER);
    }
}

impl Hasher for ChunkHasher {
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.mix(u64::from(*byte));
        }
    }

    fn write_i32(&mut self, value: i32) {
        self.mix(u64::from(value as u32));
    }

    fn write_u32(&mut self, value: u32) {
        self.mix(u64::from(value));
    }

    fn write_u64(&mut self, value: u64) {
        self.mix(value);
    }

    fn write_usize(&mut self, value: usize) {
        self.mix(value as u64);
    }

    /// The multiply leaves the entropy in the high bits, and the table also
    /// reads the low ones: fold them together (a splitmix64 step).
    fn finish(&self) -> u64 {
        let mut h = self.0;
        h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        h ^ (h >> 31)
    }
}

#[cfg(test)]
mod tests {
    use std::hash::BuildHasher;

    use super::*;

    fn hash(pos: ChunkPos) -> u64 {
        BuildHasherDefault::<ChunkHasher>::default().hash_one(pos)
    }

    #[test]
    fn the_hash_is_deterministic_and_order_sensitive() {
        assert_eq!(hash(ChunkPos::new(3, -7)), hash(ChunkPos::new(3, -7)));
        assert_ne!(hash(ChunkPos::new(3, -7)), hash(ChunkPos::new(-7, 3)));
    }

    #[test]
    fn a_grid_of_neighbours_spreads_over_the_low_bits() {
        // Table indices come from the low bits: a 64 x 64 grid must not pile up.
        let mut buckets = [0_u32; 256];
        for x in -32..32 {
            for z in -32..32 {
                buckets[(hash(ChunkPos::new(x, z)) & 255) as usize] += 1;
            }
        }
        let busiest = buckets.iter().copied().max().unwrap_or(0);
        // 4096 keys over 256 buckets: 16 each on average.
        assert!(busiest < 40, "busiest bucket holds {busiest}");
        assert!(buckets.iter().all(|&count| count > 0));
    }

    #[test]
    fn the_aliases_work_as_ordinary_collections() {
        let mut map = ChunkMap::default();
        map.insert(ChunkPos::new(1, 2), "a");
        assert_eq!(map.get(&ChunkPos::new(1, 2)), Some(&"a"));
        let mut set = ChunkSet::default();
        assert!(set.insert(ChunkPos::new(0, 0)));
        assert!(!set.insert(ChunkPos::new(0, 0)));
    }
}

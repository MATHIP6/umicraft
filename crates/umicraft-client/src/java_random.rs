// SPDX-License-Identifier: MIT OR Apache-2.0

//! The 48-bit linear congruential generator of `java.util.Random`, so the
//! seeded draws of the reference (the scatter of stacked blocks, the arrows
//! stuck in a body) land where they do there.

/// A `java.util.Random` with a given seed.
pub(crate) struct JavaRandom(u64);

impl JavaRandom {
    const MULTIPLIER: u64 = 0x5_DEEC_E66D;
    const MASK: u64 = (1 << 48) - 1;

    /// `new Random(seed)`.
    pub(crate) fn new(seed: u64) -> Self {
        Self((seed ^ Self::MULTIPLIER) & Self::MASK)
    }

    /// `Random.next(bits)`: the top `bits` of the next state.
    fn next(&mut self, bits: u32) -> u32 {
        self.0 = (self.0.wrapping_mul(Self::MULTIPLIER).wrapping_add(0xB)) & Self::MASK;
        (self.0 >> (48 - bits)) as u32
    }

    /// `Random.nextInt(bound)`, for a positive `bound`.
    pub(crate) fn next_int(&mut self, bound: u32) -> u32 {
        if bound.is_power_of_two() {
            return ((u64::from(bound) * u64::from(self.next(31))) >> 31) as u32;
        }
        loop {
            let bits = self.next(31);
            let value = bits % bound;
            // The reference rejects the draws that would bias the last values.
            if bits.wrapping_sub(value).wrapping_add(bound - 1) < 1 << 31 {
                return value;
            }
        }
    }

    /// `Random.nextFloat`: 24 random bits as a fraction.
    pub(crate) fn next_float(&mut self) -> f32 {
        self.next(24) as f32 / (1u32 << 24) as f32
    }

    /// `nextFloat() * 2 - 1`, the `-1..1` draw of the scatter.
    pub(crate) fn next_signed_float(&mut self) -> f32 {
        self.next_float() * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Values of `new Random(id)` printed by a JDK: `nextInt(16)`,
    /// `nextInt(1)`, then three `nextFloat`.
    #[test]
    fn the_draws_match_the_reference_implementation() {
        let mut random = JavaRandom::new(1);
        assert_eq!(random.next_int(16), 11);
        assert_eq!(random.next_int(1), 0);
        assert_eq!(random.next_float(), 0.410_080_8);
        assert_eq!(random.next_float(), 0.407_439_77);
        assert_eq!(random.next_float(), 0.207_714_8);
        assert_eq!(random.next_int(16), 0);

        let mut random = JavaRandom::new(5);
        assert_eq!(random.next_int(16), 11);
        assert_eq!(random.next_int(1), 0);
        assert_eq!(random.next_float(), 0.088_258_386);
    }

    #[test]
    fn a_bound_that_is_not_a_power_of_two_stays_in_range() {
        let mut random = JavaRandom::new(1660);
        // The ghast tentacle lengths: `nextInt(7) + 8` nine times.
        let lengths: Vec<u32> = (0..9).map(|_| random.next_int(7) + 8).collect();
        assert_eq!(lengths, vec![8, 13, 9, 11, 11, 10, 12, 9, 12]);
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The 48-bit linear congruential generator of `java.util.Random`.
//!
//! Several parts of the reference draw from seeded generators whose exact
//! sequence is visible on screen: the scatter of stacked blocks, the arrows
//! stuck in a body, the end portal colour ramp, the enchanting book and the
//! 1500 stars of the sky. One shared port keeps those places identical.
//!
//! Source: the `java.util.Random` specification (`seed ^ 0x5DEECE66D`, then
//! `seed * 0x5DEECE66D + 0xB` modulo 2^48, taking the top bits).

/// A `java.util.Random` with a given seed.
#[derive(Debug, Clone)]
pub struct JavaRandom {
    state: u64,
    /// The second value of the last polar-method pair (`haveNextNextGaussian`).
    spare_gaussian: Option<f64>,
}

impl JavaRandom {
    const MULTIPLIER: u64 = 0x5_DEEC_E66D;
    const MASK: u64 = (1 << 48) - 1;

    /// `new Random(seed)`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            state: (seed ^ Self::MULTIPLIER) & Self::MASK,
            spare_gaussian: None,
        }
    }

    /// `Random.next(bits)`: the top `bits` of the next state.
    fn next(&mut self, bits: u32) -> u32 {
        self.state = (self.state.wrapping_mul(Self::MULTIPLIER).wrapping_add(0xB)) & Self::MASK;
        (self.state >> (48 - bits)) as u32
    }

    /// `Random.nextInt(bound)`, for a positive `bound`.
    pub fn next_int(&mut self, bound: u32) -> u32 {
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
    pub fn next_float(&mut self) -> f32 {
        self.next(24) as f32 / (1u32 << 24) as f32
    }

    /// `nextFloat() * 2 - 1`, the `-1..1` draw of the scatter.
    pub fn next_signed_float(&mut self) -> f32 {
        self.next_float() * 2.0 - 1.0
    }

    /// `Random.nextDouble`: 53 random bits (26 then 27) as a fraction.
    pub fn next_double(&mut self) -> f64 {
        let high = u64::from(self.next(26));
        let low = u64::from(self.next(27));
        ((high << 27) + low) as f64 / (1u64 << 53) as f64
    }

    /// `Random.nextGaussian`: the polar method, which yields two values per
    /// pair of draws and keeps the second for the next call.
    pub fn next_gaussian(&mut self) -> f64 {
        if let Some(spare) = self.spare_gaussian.take() {
            return spare;
        }
        loop {
            let v1 = 2.0 * self.next_double() - 1.0;
            let v2 = 2.0 * self.next_double() - 1.0;
            let s = v1 * v1 + v2 * v2;
            if s < 1.0 && s != 0.0 {
                let multiplier = (-2.0 * s.ln() / s).sqrt();
                self.spare_gaussian = Some(v2 * multiplier);
                return v1 * multiplier;
            }
        }
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

    #[test]
    fn seed_zero_matches_the_known_sequences() {
        // `new Random(0).nextFloat()` is 0.73096776.
        assert!((JavaRandom::new(0).next_float() - 0.730_967_76).abs() < 1e-7);
        // `new Random(0).nextInt(10)` is 0, 8, 9, 7, 5; `nextInt(4)` starts with 2.
        let mut random = JavaRandom::new(0);
        let drawn: Vec<u32> = (0..5).map(|_| random.next_int(10)).collect();
        assert_eq!(drawn, [0, 8, 9, 7, 5]);
        assert_eq!(JavaRandom::new(0).next_int(4), 2);
    }

    #[test]
    fn next_double_is_the_known_first_value_of_seed_zero() {
        // `new Random(0).nextDouble()` is 0.730967787376657.
        let first = JavaRandom::new(0).next_double();
        assert!((first - 0.730_967_787_376_657).abs() < 1e-15, "{first}");
    }

    #[test]
    fn next_gaussian_matches_the_known_values_of_seed_zero() {
        // `new Random(0).nextGaussian()` twice: 0.8025330637390305, then
        // the spare of the same pair, -0.9015460884175122.
        let mut random = JavaRandom::new(0);
        assert!((random.next_gaussian() - 0.802_533_063_739_030_5).abs() < 1e-12);
        assert!((random.next_gaussian() + 0.901_546_088_417_512_2).abs() < 1e-12);
    }
}

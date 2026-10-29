// SPDX-License-Identifier: MIT OR Apache-2.0

//! Limb swing: the phase and amplitude that drive walking animations.

/// The walking animation state of a living entity.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LimbSwing {
    /// Accumulated phase (MCP `limbSwing`).
    pub swing: f32,
    /// Smoothed amplitude in `0..=1` (MCP `limbSwingAmount`).
    pub amount: f32,
    /// The amplitude one tick ago (MCP `prevLimbSwingAmount`).
    pub previous_amount: f32,
}

/// How fast the amplitude follows the speed (MCP `moveEntityWithHeading`).
const AMOUNT_SMOOTHING: f32 = 0.4;

impl LimbSwing {
    /// Advances one tick for an entity that moved `horizontal_speed` blocks.
    ///
    /// The target amplitude is `min(speed * 4, 1)`; the phase grows by the
    /// *smoothed* amplitude, so legs never swing faster than the entity walks
    /// (MCP `EntityLivingBase.moveEntityWithHeading`, last lines).
    pub fn update(&mut self, horizontal_speed: f32) {
        self.previous_amount = self.amount;
        let target = (horizontal_speed * 4.0).min(1.0);
        self.amount += (target - self.amount) * AMOUNT_SMOOTHING;
        self.swing += self.amount;
    }

    /// The amplitude `alpha` of the way into the current tick.
    #[must_use]
    pub fn amount_at(&self, alpha: f32) -> f32 {
        self.previous_amount + (self.amount - self.previous_amount) * alpha
    }

    /// The phase `alpha` of the way into the current tick
    /// (MCP `limbSwing - limbSwingAmount * (1 - partialTicks)`).
    #[must_use]
    pub fn swing_at(&self, alpha: f32) -> f32 {
        self.swing - self.amount * (1.0 - alpha)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_amplitude_chases_the_clamped_speed() {
        let mut limbs = LimbSwing::default();
        limbs.update(10.0);
        assert!((limbs.amount - 0.4).abs() < 1e-6, "clamped target 1.0");
        limbs.update(10.0);
        assert!((limbs.amount - 0.64).abs() < 1e-6);
        assert!((limbs.previous_amount - 0.4).abs() < 1e-6);
    }

    #[test]
    fn the_phase_accumulates_the_smoothed_amplitude() {
        let mut limbs = LimbSwing::default();
        limbs.update(10.0);
        limbs.update(10.0);
        // 0.4 + 0.64, not 2 * (10 * 4) as the raw speed would give.
        assert!((limbs.swing - 1.04).abs() < 1e-6);
    }

    #[test]
    fn a_standing_entity_winds_down_without_stopping_dead() {
        let mut limbs = LimbSwing {
            swing: 3.0,
            amount: 1.0,
            previous_amount: 1.0,
        };
        limbs.update(0.0);
        assert!((limbs.amount - 0.6).abs() < 1e-6);
        assert!((limbs.swing - 3.6).abs() < 1e-6);
    }

    #[test]
    fn the_render_phase_lags_by_the_remaining_tick() {
        let limbs = LimbSwing {
            swing: 5.0,
            amount: 0.5,
            previous_amount: 0.3,
        };
        assert!((limbs.swing_at(1.0) - 5.0).abs() < 1e-6);
        assert!((limbs.swing_at(0.0) - 4.5).abs() < 1e-6);
        assert!((limbs.amount_at(0.5) - 0.4).abs() < 1e-6);
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The potion damage values the creative inventory lists (MCP
//! `ItemPotion.getSubItems`).
//!
//! Vanilla tries every combination of the four brewing bits, the splash bit
//! and the two "upgrade" bits, asks `PotionHelper.getPotionEffects` for the
//! effects and keeps one damage value per distinct effect list (a
//! `LinkedHashMap` keyed by the list: the first insertion fixes the position,
//! a later duplicate only replaces the damage value). The requirement
//! expressions are evaluated by [`potion_color`](super::potion_color); this
//! module adds the durations, which are what tell `Speed` from `Speed
//! (extended)` apart.

use super::potion_color::{evaluate, RECIPES};

/// `ItemPotion` damage bit of a drinkable potion.
const DRINKABLE_BIT: i32 = 8192;
/// `ItemPotion` damage bit of a splash potion.
const SPLASH_BIT: i32 = 16384;
/// The "extended" (bit 5) and "level II" (bit 6) upgrade flags.
const UPGRADE_BITS: [i32; 3] = [0, 32, 64];
/// Potion ids whose effect is instant (`PotionHealth.isInstant`).
const INSTANT_IDS: [u8; 2] = [6, 7];

/// One active effect of a potion (`PotionEffect` as far as `equals` looks).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Effect {
    id: u8,
    duration: i32,
    amplifier: i32,
    splash: bool,
}

/// `Potion.getEffectiveness()` of the potions in [`RECIPES`]: bad effects
/// start at 0.5 and `setEffectiveness` overrides regeneration and poison.
fn effectiveness(id: u8) -> f64 {
    match id {
        10 | 19 => 0.25,
        2 | 18 => 0.5,
        _ => 1.0,
    }
}

/// Java's `Math.round` for the positive values used here.
fn round(value: f64) -> i32 {
    (value + 0.5).floor() as i32
}

/// `PotionHelper.getPotionEffects(damage, false)`.
fn effects(damage: i32) -> Vec<Effect> {
    let splash = damage & SPLASH_BIT != 0;
    RECIPES
        .iter()
        .filter_map(|recipe| {
            let strength = evaluate(recipe.requirement, damage);
            if strength <= 0 {
                return None;
            }
            let amplifier = recipe
                .amplifier
                .map_or(0, |expression| evaluate(expression, damage).max(0));
            let duration = if INSTANT_IDS.contains(&recipe.id) {
                1
            } else {
                let base = (1200 * (strength * 3 + (strength - 1) * 2)) >> amplifier;
                let scaled = round(f64::from(base) * effectiveness(recipe.id));
                if splash {
                    round(f64::from(scaled) * 0.75 + 0.5)
                } else {
                    scaled
                }
            };
            Some(Effect {
                id: recipe.id,
                duration,
                amplifier,
                splash,
            })
        })
        .collect()
}

/// The damage values of the potions in the creative inventory, in vanilla
/// order: the water bottle (damage 0), then one per distinct effect list.
///
/// ```
/// let damages = umicraft_game::items::potion_creative_damages();
/// assert_eq!(damages[0], 0);
/// assert!(damages.contains(&8194)); // Potion of Swiftness
/// ```
#[must_use]
pub fn potion_creative_damages() -> Vec<i16> {
    let mut distinct: Vec<(Vec<Effect>, i32)> = Vec::new();
    for base in (0..=15).flat_map(|bits| [bits | DRINKABLE_BIT, bits | SPLASH_BIT]) {
        for upgrade in UPGRADE_BITS {
            let damage = base | upgrade;
            let list = effects(damage);
            if list.is_empty() {
                continue;
            }
            match distinct.iter_mut().find(|(known, _)| *known == list) {
                Some((_, stored)) => *stored = damage,
                None => distinct.push((list, damage)),
            }
        }
    }
    std::iter::once(0)
        .chain(distinct.into_iter().map(|(_, damage)| damage as i16))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_water_bottle_comes_first() {
        assert_eq!(potion_creative_damages()[0], 0);
    }

    #[test]
    fn distinct_effect_lists_are_unique_and_non_empty() {
        let damages = potion_creative_damages();
        // 62 distinct effect lists plus the water bottle.
        assert_eq!(damages.len(), 63);
        let mut lists: Vec<_> = damages[1..]
            .iter()
            .map(|&d| effects(i32::from(d)))
            .collect();
        assert!(lists.iter().all(|list| !list.is_empty()));
        let total = lists.len();
        lists.dedup();
        assert_eq!(lists.len(), total);
    }

    #[test]
    fn swiftness_and_its_upgrades_are_listed() {
        let damages = potion_creative_damages();
        for expected in [8194, 8226, 8258] {
            assert!(damages.contains(&expected), "{expected}");
        }
    }

    #[test]
    fn durations_follow_the_reference_formula() {
        // Swiftness (speed, requirement value 1, no upgrade): 1200 * 3 ticks.
        assert_eq!(effects(8194)[0].duration, 3600);
        // Splash: round(3600 * 0.75 + 0.5), the half rounding up.
        assert_eq!(effects(16386)[0].duration, 2701);
        // Instant health lasts 1 tick whatever the variant.
        assert_eq!(effects(8197)[0].duration, 1);
    }
}

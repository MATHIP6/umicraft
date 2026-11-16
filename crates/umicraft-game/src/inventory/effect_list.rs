// SPDX-License-Identifier: MIT OR Apache-2.0

//! The lines of the potion effect list beside the inventory (MCP
//! `InventoryEffectRenderer.drawActivePotionEffects`).
//!
//! Each active effect shows its status icon, its name with the level for
//! levels II to IV, and its remaining time. The lines come in the order of
//! [`Effects::listed`]; the window thread translates the keys and draws them.

use crate::inventory::tooltip::{effect_name_key, elapsed_time};
use crate::server::effects::{ActiveEffect, Effects};

/// The duration text of an effect sent with the "maximum" marker
/// (`Potion.getDurationString`).
const MAX_DURATION_TEXT: &str = "**:**";

/// The status icon index of each potion (`Potion.setIconIndex(x, y)`, the
/// index `x + 8 * y` in the icon grid of `container/inventory.png`). Instant
/// health, instant damage and saturation have none.
const STATUS_ICONS: [(u8, u8); 20] = [
    (1, 0),
    (2, 1),
    (3, 2),
    (4, 3),
    (5, 4),
    (8, 10),
    (9, 11),
    (10, 7),
    (11, 14),
    (12, 15),
    (13, 16),
    (14, 8),
    (15, 13),
    (16, 12),
    (17, 9),
    (18, 5),
    (19, 6),
    (20, 17),
    (21, 18),
    (22, 18),
];

/// One line of the effect list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectLine {
    /// The language key of the effect name (`potion.moveSpeed`).
    pub name_key: &'static str,
    /// The language key of the level appended to the name
    /// (`enchantment.level.2` to `.4`), for amplifiers 1 to 3 only.
    pub level_key: Option<&'static str>,
    /// The status icon index, `None` for an effect without one.
    pub icon: Option<u8>,
    /// The remaining time, `m:ss`, or `**:**` for a "maximum" duration.
    pub duration: String,
}

/// The lines of the effects of the local player, in the reference order.
///
/// An effect whose id is not a 1.8 potion is left out: the reference would
/// fail on it (`Potion.potionTypes[id]` is `null`).
#[must_use]
pub fn effect_lines(effects: &Effects) -> Vec<EffectLine> {
    effects.listed().into_iter().filter_map(line).collect()
}

fn line(effect: &ActiveEffect) -> Option<EffectLine> {
    Some(EffectLine {
        name_key: effect_name_key(effect.effect_id)?,
        level_key: level_key(effect.amplifier),
        icon: status_icon(effect.effect_id),
        duration: if effect.max_duration {
            MAX_DURATION_TEXT.to_owned()
        } else {
            elapsed_time(effect.duration)
        },
    })
}

/// The level shown after the name: II to IV only, nothing for level I or
/// past IV.
fn level_key(amplifier: i8) -> Option<&'static str> {
    match amplifier {
        1 => Some("enchantment.level.2"),
        2 => Some("enchantment.level.3"),
        3 => Some("enchantment.level.4"),
        _ => None,
    }
}

/// `Potion.getStatusIconIndex` for the potions that have an icon.
fn status_icon(effect_id: u8) -> Option<u8> {
    STATUS_ICONS
        .iter()
        .find(|&&(id, _)| id == effect_id)
        .map(|&(_, icon)| icon)
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_protocol::packets::play::clientbound::entity_effect::EntityEffect;

    fn effects(list: &[(i8, i8, i32)]) -> Effects {
        let mut effects = Effects::default();
        for &(effect_id, amplifier, duration) in list {
            effects.apply(&EntityEffect {
                entity_id: 1,
                effect_id,
                amplifier,
                duration,
                hide_particles: 1,
            });
        }
        effects
    }

    #[test]
    fn a_line_has_the_name_the_level_the_icon_and_the_time() {
        let lines = effect_lines(&effects(&[(1, 1, 3 * 60 * 20 + 5 * 20 + 19)]));
        assert_eq!(
            lines,
            [EffectLine {
                name_key: "potion.moveSpeed",
                level_key: Some("enchantment.level.2"),
                icon: Some(0),
                duration: "3:05".to_owned(),
            }]
        );
    }

    #[test]
    fn only_levels_two_to_four_are_written() {
        let level = |amplifier| effect_lines(&effects(&[(3, amplifier, 100)]))[0].level_key;
        assert_eq!(level(0), None);
        assert_eq!(level(3), Some("enchantment.level.4"));
        assert_eq!(level(4), None);
        assert_eq!(level(-1), None);
    }

    #[test]
    fn a_maximum_duration_shows_stars_and_unknown_ids_are_skipped() {
        let lines = effect_lines(&effects(&[(16, 0, 32767), (40, 0, 100)]));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].duration, "**:**");
        assert_eq!(lines[0].icon, Some(12));
    }

    #[test]
    fn the_icons_follow_the_reference_grid() {
        // `setIconIndex(x, y)` is `x + 8 * y`: regeneration (7, 0), water
        // breathing (0, 2), absorption shares health boost's (2, 2).
        assert_eq!(status_icon(10), Some(7));
        assert_eq!(status_icon(13), Some(16));
        assert_eq!(status_icon(22), status_icon(21));
        assert_eq!(status_icon(6), None);
        assert_eq!(status_icon(23), None);
    }

    #[test]
    fn the_lines_come_in_the_order_of_the_reference_map() {
        let lines = effect_lines(&effects(&[(1, 0, 100), (16, 0, 100)]));
        let names: Vec<_> = lines.iter().map(|line| line.name_key).collect();
        assert_eq!(names, ["potion.nightVision", "potion.moveSpeed"]);
    }
}

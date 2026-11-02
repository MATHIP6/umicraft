// SPDX-License-Identifier: MIT OR Apache-2.0

//! The bar above the hotbar: experience, or the jump bar on a horse (MCP
//! `GuiIngame.renderExpBar`, `renderHorseJumpBar`).

/// Width of the bar texture in pixels.
const BAR_WIDTH: f32 = 182.0;
/// The colour of the level number (`renderExpBar`: `8453920`), RGB.
pub const LEVEL_COLOR: u32 = 8_453_920;

/// What the bar above the hotbar shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BottomBar {
    /// Nothing: creative, spectator and the other modes without a bar.
    None,
    /// The experience bar and, above a level of zero, the level number.
    Experience {
        /// Pixels of the filled part, `(int)(experience * 183)`.
        fill: u32,
        /// The level, when it is above zero.
        level: Option<i32>,
    },
    /// The jump bar of a saddled horse.
    HorseJump {
        /// Pixels of the filled part, `(int)(power * 183)`.
        fill: u32,
    },
}

/// `EntityPlayer.xpBarCap`: the points of a level.
#[must_use]
pub fn xp_bar_cap(level: i32) -> i32 {
    if level >= 30 {
        112 + (level - 30) * 9
    } else if level >= 15 {
        37 + (level - 15) * 5
    } else {
        7 + level * 2
    }
}

/// Picks the bar like `renderGameOverlay`: the jump bar while riding a
/// saddled horse, else the experience bar in survival and adventure.
///
/// The fill is `(int)(fraction * 183)`, one more than the 182-pixel texture,
/// so a full bar draws one pixel past it; the renderer clamps that.
///
/// ```
/// use umicraft_game::hud::{bottom_bar, BottomBar};
/// let bar = bottom_bar(true, false, 0.0, 0.5, 7);
/// assert_eq!(bar, BottomBar::Experience { fill: 91, level: Some(7) });
/// ```
#[must_use]
pub fn bottom_bar(
    survival_or_adventure: bool,
    riding_horse: bool,
    jump_power: f32,
    experience: f32,
    level: i32,
) -> BottomBar {
    if riding_horse {
        BottomBar::HorseJump {
            fill: fill_pixels(jump_power),
        }
    } else if survival_or_adventure {
        BottomBar::Experience {
            fill: fill_pixels(experience),
            level: (level > 0).then_some(level),
        }
    } else {
        BottomBar::None
    }
}

fn fill_pixels(fraction: f32) -> u32 {
    (fraction * (BAR_WIDTH + 1.0)).max(0.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_level_colour_is_the_vanilla_green() {
        assert_eq!(LEVEL_COLOR, 0x80FF20);
    }

    #[test]
    fn the_cap_follows_the_three_level_ranges() {
        assert_eq!(xp_bar_cap(0), 7);
        assert_eq!(xp_bar_cap(14), 35);
        assert_eq!(xp_bar_cap(15), 37);
        assert_eq!(xp_bar_cap(29), 107);
        assert_eq!(xp_bar_cap(30), 112);
        assert_eq!(xp_bar_cap(31), 121);
    }

    #[test]
    fn the_fill_is_the_fraction_of_183_pixels() {
        let fill = |x| match bottom_bar(true, false, 0.0, x, 0) {
            BottomBar::Experience { fill, .. } => fill,
            other => panic!("{other:?}"),
        };
        assert_eq!(fill(0.0), 0);
        assert_eq!(fill(0.5), 91);
        assert_eq!(fill(1.0), 183);
    }

    #[test]
    fn a_level_of_zero_has_no_number() {
        assert_eq!(
            bottom_bar(true, false, 0.0, 0.0, 0),
            BottomBar::Experience {
                fill: 0,
                level: None
            }
        );
    }

    #[test]
    fn creative_has_no_bar_and_a_horse_replaces_it() {
        assert_eq!(bottom_bar(false, false, 0.0, 0.5, 3), BottomBar::None);
        assert_eq!(
            bottom_bar(true, true, 0.5, 0.9, 3),
            BottomBar::HorseJump { fill: 91 }
        );
        // The jump bar shows even where the experience bar would not.
        assert_eq!(
            bottom_bar(false, true, 1.0, 0.0, 0),
            BottomBar::HorseJump { fill: 183 }
        );
    }
}

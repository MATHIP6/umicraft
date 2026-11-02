// SPDX-License-Identifier: MIT OR Apache-2.0

//! The hearts, armour and food rows (MCP `GuiIngame.renderPlayerStats`).
//!
//! [`vitals_view`] reproduces the reference loop by loop, including the order
//! in which it draws from the shared `java.util.Random`: the low-health
//! shake of the hearts and the saturation shake of the food icons consume one
//! generator seeded with `updateCounter * 312871`, so a given tick always
//! shakes the same icons.
//!
//! Offsets are in GUI pixels relative to the row baseline `height - 39`
//! (negative is up); columns count icons from the left edge of the hotbar
//! for hearts and armour and from its right edge for food.

use umicraft_core::java_random::JavaRandom;

use super::air::{air_view, AirView};
use super::flash::FlashFrame;
use super::mount::{mount_hearts, MountHeartView, Riding};

/// Icons per row.
const ROW: i32 = 10;
/// `updateCounter * 312871`: the seed multiplier of the shake.
const SEED_MULTIPLIER: i32 = 312_871;
/// Health at or below which the hearts shake (`i <= 4`).
const LOW_HEALTH: i32 = 4;
/// Pixels an armour or air row sits above the top heart row's baseline.
const UPPER_ROW_GAP: i32 = 10;
/// Fewest pixels between two heart rows (`Math.max(10 - (rows - 2), 3)`).
const MIN_ROW_SPACING: i32 = 3;

/// How much of an icon is filled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    /// Only the container.
    Empty,
    /// One point of two.
    Half,
    /// Both points.
    Full,
}

/// The texture variant of the hearts, from the active effects (poison wins
/// over wither).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartStyle {
    /// Red hearts.
    Normal,
    /// Green hearts (`Potion.poison`).
    Poison,
    /// Black hearts (`Potion.wither`).
    Wither,
}

/// What a heart shows on top of its container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartBody {
    /// Health.
    Health(Fill),
    /// Absorption (golden) hearts; `half` for the odd last one.
    Absorption {
        /// Whether only half of the heart is golden.
        half: bool,
    },
}

/// One heart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeartView {
    /// The column, `0..10`, from the left.
    pub col: u8,
    /// Pixels above the baseline, including row spacing, shake and wiggle.
    pub dy: i32,
    /// The texture variant.
    pub style: HeartStyle,
    /// Whether the hardcore container row of the texture is used.
    pub hardcore: bool,
    /// Whether the highlighted container and the ghost are drawn (damage).
    pub blink: bool,
    /// The ghost of the old health, drawn under the body while blinking.
    pub ghost: Fill,
    /// The health or absorption drawn over the container.
    pub body: HeartBody,
}

/// The armour row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArmorView {
    /// The ten icons; [`Fill::Empty`] is the outline.
    pub icons: [Fill; ROW as usize],
}

/// One food icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FoodIconView {
    /// The column, `0..10`, from the right.
    pub col: u8,
    /// Pixels above the baseline: the saturation shake, `-1..=1`.
    pub dy: i32,
    /// Whether the hunger effect textures are used.
    pub hunger: bool,
    /// How much of the drumstick is filled.
    pub fill: Fill,
}

/// Everything `renderPlayerStats` needs about the player.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VitalsInput {
    /// `getHealth`.
    pub health: f32,
    /// The `generic.maxHealth` attribute.
    pub max_health: f32,
    /// `getAbsorptionAmount`.
    pub absorption: f32,
    /// `getTotalArmorValue`.
    pub armor: i32,
    /// `FoodStats.getFoodLevel`.
    pub food: i32,
    /// `FoodStats.getSaturationLevel`.
    pub saturation: f32,
    /// The world is hardcore.
    pub hardcore: bool,
    /// Poison is active.
    pub poison: bool,
    /// Wither is active.
    pub wither: bool,
    /// Regeneration is active: one heart wiggles.
    pub regeneration: bool,
    /// Hunger is active.
    pub hunger: bool,
    /// `GuiIngame.updateCounter`, the tick count of the GUI.
    pub update_counter: u32,
    /// The damage blink of this frame.
    pub flash: FlashFrame,
    /// What the player rides.
    pub riding: Riding,
    /// The air in ticks while the eyes are in water, else `None`.
    pub air: Option<i32>,
}

/// The status rows of one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct VitalsView {
    /// The hearts in drawing order (top row, last column first).
    pub hearts: Vec<HeartView>,
    /// The armour row, hidden at zero armour.
    pub armor: Option<ArmorView>,
    /// Pixels above the baseline of the armour row and the air bubbles.
    pub upper_dy: i32,
    /// The food icons; empty while riding.
    pub food: Vec<FoodIconView>,
    /// The mount's hearts; empty on foot.
    pub mount: Vec<MountHeartView>,
    /// The air bubbles.
    pub air: Option<AirView>,
}

/// Builds the rows for one frame.
///
/// ```
/// use umicraft_game::hud::{vitals_view, FlashFrame, Riding, VitalsInput};
/// let input = VitalsInput {
///     health: 20.0, max_health: 20.0, absorption: 0.0, armor: 0, food: 20,
///     saturation: 5.0, hardcore: false, poison: false, wither: false,
///     regeneration: false, hunger: false, update_counter: 0,
///     flash: FlashFrame { blink: false, last_health: 20 },
///     riding: Riding::OnFoot, air: None,
/// };
/// assert_eq!(vitals_view(&input).hearts.len(), 10);
/// ```
#[must_use]
pub fn vitals_view(input: &VitalsInput) -> VitalsView {
    let mut rng = JavaRandom::new(i64::from(
        (input.update_counter as i32).wrapping_mul(SEED_MULTIPLIER),
    ) as u64);
    let layout = RowLayout::new(input);
    let hearts = hearts(input, &layout, &mut rng);
    let (food, mount) = match input.riding {
        Riding::OnFoot => (food_icons(input, &mut rng), Vec::new()),
        Riding::Vehicle => (Vec::new(), Vec::new()),
        Riding::Mob { health, max_health } => (Vec::new(), mount_hearts(health, max_health)),
    };
    VitalsView {
        hearts,
        armor: (input.armor > 0).then(|| armor_row(input.armor)),
        upper_dy: layout.upper_dy,
        food,
        mount,
        air: input.air.map(air_view),
    }
}

/// How the heart rows stack: more health and absorption mean more rows, packed
/// closer together.
struct RowLayout {
    spacing: i32,
    upper_dy: i32,
    wiggle: Option<i32>,
}

impl RowLayout {
    fn new(input: &VitalsInput) -> Self {
        let rows = ((input.max_health + input.absorption) / 2.0 / 10.0).ceil() as i32;
        let spacing = (10 - (rows - 2)).max(MIN_ROW_SPACING);
        Self {
            spacing,
            upper_dy: -((rows - 1) * spacing + UPPER_ROW_GAP),
            // `updateCounter % ceil(maxHealth + 5)` picks the heart that hops.
            wiggle: input.regeneration.then(|| {
                (input.update_counter % (input.max_health + 5.0).ceil().max(1.0) as u32) as i32
            }),
        }
    }
}

fn hearts(input: &VitalsInput, layout: &RowLayout, rng: &mut JavaRandom) -> Vec<HeartView> {
    let health = input.health.ceil() as i32;
    let style = if input.poison {
        HeartStyle::Poison
    } else if input.wither {
        HeartStyle::Wither
    } else {
        HeartStyle::Normal
    };
    let count = ((input.max_health + input.absorption) / 2.0).ceil() as i32;
    let mut absorption_left = input.absorption;
    let mut hearts = Vec::with_capacity(count.max(0) as usize);
    for index in (0..count).rev() {
        let row = (index + 1 + ROW - 1) / ROW - 1;
        let mut dy = -(row * layout.spacing);
        if health <= LOW_HEALTH {
            dy += rng.next_int(2) as i32;
        }
        if layout.wiggle == Some(index) {
            dy -= 2;
        }
        let body = if absorption_left > 0.0 {
            let half = absorption_left == input.absorption && input.absorption % 2.0 == 1.0;
            absorption_left -= 2.0;
            HeartBody::Absorption { half }
        } else {
            HeartBody::Health(fill_of(index * 2 + 1, health))
        };
        hearts.push(HeartView {
            col: (index % ROW) as u8,
            dy,
            style,
            hardcore: input.hardcore,
            blink: input.flash.blink,
            ghost: fill_of(index * 2 + 1, input.flash.last_health),
            body,
        });
    }
    hearts
}

fn armor_row(armor: i32) -> ArmorView {
    let mut icons = [Fill::Empty; ROW as usize];
    for (index, icon) in icons.iter_mut().enumerate() {
        let point = index as i32 * 2 + 1;
        *icon = match point.cmp(&armor) {
            std::cmp::Ordering::Less => Fill::Full,
            std::cmp::Ordering::Equal => Fill::Half,
            std::cmp::Ordering::Greater => Fill::Empty,
        };
    }
    ArmorView { icons }
}

fn food_icons(input: &VitalsInput, rng: &mut JavaRandom) -> Vec<FoodIconView> {
    // `updateCounter % (food * 3 + 1) == 0` with no saturation shakes the
    // icons by one pixel either way.
    let shaking =
        input.saturation <= 0.0 && input.update_counter % (input.food.max(0) as u32 * 3 + 1) == 0;
    (0..ROW)
        .map(|index| FoodIconView {
            col: index as u8,
            dy: if shaking {
                rng.next_int(3) as i32 - 1
            } else {
                0
            },
            hunger: input.hunger,
            fill: fill_of(index * 2 + 1, input.food),
        })
        .collect()
}

/// Full below the value, half at it, empty above (the `i * 2 + 1 < value`
/// and `== value` pairs of the reference).
fn fill_of(point: i32, value: i32) -> Fill {
    match point.cmp(&value) {
        std::cmp::Ordering::Less => Fill::Full,
        std::cmp::Ordering::Equal => Fill::Half,
        std::cmp::Ordering::Greater => Fill::Empty,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> VitalsInput {
        VitalsInput {
            health: 20.0,
            max_health: 20.0,
            absorption: 0.0,
            armor: 0,
            food: 20,
            saturation: 5.0,
            hardcore: false,
            poison: false,
            wither: false,
            regeneration: false,
            hunger: false,
            update_counter: 7,
            flash: FlashFrame {
                blink: false,
                last_health: 20,
            },
            riding: Riding::OnFoot,
            air: None,
        }
    }

    fn body(view: &VitalsView, col: u8) -> HeartBody {
        view.hearts
            .iter()
            .find(|h| h.col == col)
            .map(|h| h.body)
            .expect("heart")
    }

    #[test]
    fn full_health_draws_ten_full_hearts_from_the_right() {
        let view = vitals_view(&input());
        assert_eq!(view.hearts.len(), 10);
        assert_eq!(view.hearts[0].col, 9, "drawn top row, last column first");
        assert!(view
            .hearts
            .iter()
            .all(|h| h.body == HeartBody::Health(Fill::Full) && h.dy == 0));
    }

    #[test]
    fn odd_health_ends_in_a_half_heart() {
        let view = vitals_view(&VitalsInput {
            health: 7.0,
            ..input()
        });
        assert_eq!(body(&view, 2), HeartBody::Health(Fill::Full));
        assert_eq!(body(&view, 3), HeartBody::Health(Fill::Half));
        assert_eq!(body(&view, 4), HeartBody::Health(Fill::Empty));
    }

    #[test]
    fn a_fractional_health_is_rounded_up() {
        let view = vitals_view(&VitalsInput {
            health: 6.2,
            ..input()
        });
        assert_eq!(body(&view, 3), HeartBody::Health(Fill::Half));
    }

    #[test]
    fn poison_wins_over_wither() {
        let both = vitals_view(&VitalsInput {
            poison: true,
            wither: true,
            ..input()
        });
        assert_eq!(both.hearts[0].style, HeartStyle::Poison);
        let wither = vitals_view(&VitalsInput {
            wither: true,
            ..input()
        });
        assert_eq!(wither.hearts[0].style, HeartStyle::Wither);
    }

    #[test]
    fn regeneration_lifts_one_heart_two_pixels() {
        // counter 7 % ceil(20 + 5) = 7: heart index 7.
        let view = vitals_view(&VitalsInput {
            regeneration: true,
            ..input()
        });
        let lifted: Vec<_> = view.hearts.iter().filter(|h| h.dy != 0).collect();
        assert_eq!(lifted.len(), 1);
        assert_eq!((lifted[0].col, lifted[0].dy), (7, -2));
    }

    #[test]
    fn a_counter_past_the_last_heart_lifts_nothing() {
        // Beyond the last heart the counter lifts nothing (24 > 9).
        let view = vitals_view(&VitalsInput {
            regeneration: true,
            update_counter: 24,
            ..input()
        });
        assert!(view.hearts.iter().all(|h| h.dy == 0));
    }

    #[test]
    fn low_health_shakes_hearts_by_the_seeded_generator() {
        let low = VitalsInput {
            health: 4.0,
            ..input()
        };
        let view = vitals_view(&low);
        // The reference draws one nextInt(2) per heart, top index first.
        let mut rng = JavaRandom::new(i64::from(7_i32 * 312_871) as u64);
        for heart in &view.hearts {
            assert_eq!(heart.dy, rng.next_int(2) as i32);
        }
        assert!(view.hearts.iter().any(|h| h.dy == 1));
        assert_eq!(
            vitals_view(&low),
            vitals_view(&low),
            "same tick, same shake"
        );
    }

    #[test]
    fn five_health_does_not_shake() {
        let view = vitals_view(&VitalsInput {
            health: 5.0,
            ..input()
        });
        assert!(view.hearts.iter().all(|h| h.dy == 0));
    }

    #[test]
    fn absorption_adds_a_row_and_draws_golden_hearts_first() {
        // 20 health + 4 absorption: 12 hearts in two rows, spacing 10, the
        // two top hearts golden; armour and air move up by 20.
        let view = vitals_view(&VitalsInput {
            absorption: 4.0,
            armor: 5,
            ..input()
        });
        assert_eq!(view.hearts.len(), 12);
        assert_eq!(view.hearts[0].body, HeartBody::Absorption { half: false });
        assert_eq!(view.hearts[1].body, HeartBody::Absorption { half: false });
        assert_eq!(view.hearts[2].body, HeartBody::Health(Fill::Full));
        assert_eq!(view.hearts[0].dy, -10, "the eleventh heart is in row two");
        assert_eq!(view.upper_dy, -20);
    }

    #[test]
    fn an_odd_absorption_ends_in_a_half_golden_heart_at_the_top() {
        let view = vitals_view(&VitalsInput {
            absorption: 1.0,
            ..input()
        });
        assert_eq!(view.hearts[0].body, HeartBody::Absorption { half: true });
    }

    #[test]
    fn many_rows_pack_closer_but_never_below_three_pixels() {
        let view = vitals_view(&VitalsInput {
            max_health: 100.0,
            health: 100.0,
            ..input()
        });
        // 5 rows: spacing 10 - 3 = 7.
        assert_eq!(view.hearts.len(), 50);
        assert_eq!(view.hearts[0].dy, -28);
        assert_eq!(view.upper_dy, -(4 * 7 + 10));
        let huge = vitals_view(&VitalsInput {
            max_health: 400.0,
            health: 400.0,
            ..input()
        });
        assert_eq!(huge.upper_dy, -(19 * 3 + 10));
    }

    #[test]
    fn the_hardcore_flag_and_the_blink_reach_every_heart() {
        let view = vitals_view(&VitalsInput {
            hardcore: true,
            health: 10.0,
            flash: FlashFrame {
                blink: true,
                last_health: 14,
            },
            ..input()
        });
        assert!(view.hearts.iter().all(|h| h.hardcore && h.blink));
        let ghost = |col| view.hearts.iter().find(|h| h.col == col).map(|h| h.ghost);
        assert_eq!(ghost(6), Some(Fill::Full));
        assert_eq!(ghost(7), Some(Fill::Empty));
    }

    #[test]
    fn armour_is_hidden_at_zero_and_half_filled_when_odd() {
        assert!(vitals_view(&input()).armor.is_none());
        let armor = vitals_view(&VitalsInput {
            armor: 7,
            ..input()
        })
        .armor
        .expect("row");
        assert_eq!(armor.icons[2], Fill::Full);
        assert_eq!(armor.icons[3], Fill::Half);
        assert_eq!(armor.icons[4], Fill::Empty);
        assert_eq!(vitals_view(&input()).upper_dy, -10);
    }

    #[test]
    fn food_fills_from_the_right_edge_and_hunger_changes_the_texture() {
        let view = vitals_view(&VitalsInput {
            food: 5,
            hunger: true,
            ..input()
        });
        assert_eq!(view.food.len(), 10);
        assert_eq!(view.food[1].fill, Fill::Full);
        assert_eq!(view.food[2].fill, Fill::Half);
        assert_eq!(view.food[3].fill, Fill::Empty);
        assert!(view.food.iter().all(|f| f.hunger));
    }

    #[test]
    fn empty_saturation_shakes_food_on_the_matching_ticks_only() {
        let mut shaky = VitalsInput {
            saturation: 0.0,
            food: 10,
            ..input()
        };
        // 10 * 3 + 1 = 31: tick 31 shakes, tick 30 does not.
        shaky.update_counter = 30;
        assert!(vitals_view(&shaky).food.iter().all(|f| f.dy == 0));
        shaky.update_counter = 31;
        let view = vitals_view(&shaky);
        let mut rng = JavaRandom::new(i64::from(31_i32 * 312_871) as u64);
        for icon in &view.food {
            assert_eq!(icon.dy, rng.next_int(3) as i32 - 1);
        }
        shaky.saturation = 0.5;
        assert!(vitals_view(&shaky).food.iter().all(|f| f.dy == 0));
    }

    #[test]
    fn the_food_shake_continues_the_sequence_of_the_low_health_shake() {
        let view = vitals_view(&VitalsInput {
            health: 2.0,
            saturation: 0.0,
            food: 10,
            update_counter: 31,
            ..input()
        });
        let mut rng = JavaRandom::new(i64::from(31_i32 * 312_871) as u64);
        for _ in 0..view.hearts.len() {
            rng.next_int(2);
        }
        for icon in &view.food {
            assert_eq!(icon.dy, rng.next_int(3) as i32 - 1);
        }
    }

    #[test]
    fn riding_a_mob_replaces_food_by_its_hearts() {
        let view = vitals_view(&VitalsInput {
            riding: Riding::Mob {
                health: 10.0,
                max_health: 20.0,
            },
            ..input()
        });
        assert!(view.food.is_empty());
        assert_eq!(view.mount.len(), 10);
    }

    #[test]
    fn riding_a_boat_shows_neither_food_nor_hearts() {
        let view = vitals_view(&VitalsInput {
            riding: Riding::Vehicle,
            ..input()
        });
        assert!(view.food.is_empty() && view.mount.is_empty());
    }

    #[test]
    fn air_shows_only_when_given() {
        assert!(vitals_view(&input()).air.is_none());
        let view = vitals_view(&VitalsInput {
            air: Some(150),
            ..input()
        });
        assert_eq!(view.air.map(|a| a.full), Some(5));
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The liquid colour of a potion item, ported from MCP `PotionHelper`.
//!
//! A potion's damage value is a bit field. `PotionHelper` keeps one small
//! expression per potion (`"!0 & 1 & !2 & !3 & 1+6"`) that is evaluated
//! against the damage to decide whether the potion carries that effect, and a
//! second one that gives the amplifier. The bottle colour is the average of
//! the effect colours, each counted `amplifier + 1` times.

/// Colour of a bottle without effects (`PotionHelper.calcPotionLiquidColor`).
pub(super) const WATER_COLOR: u32 = 0x38_5DC6;

/// One potion that `PotionHelper.getPotionEffects` can derive from a damage value.
pub(super) struct Recipe {
    /// `Potion.getId()`.
    pub id: u8,
    /// `PotionHelper.potionRequirements`: the effect is present when this is positive.
    pub requirement: &'static str,
    /// `PotionHelper.potionAmplifiers`: the amplifier expression, if the potion has one.
    pub amplifier: Option<&'static str>,
}

/// `PotionHelper.potionRequirements` and `potionAmplifiers`, in potion id order.
pub(super) static RECIPES: &[Recipe] = &[
    Recipe {
        id: 1,
        requirement: "!0 & 1 & !2 & !3 & 1+6",
        amplifier: Some("5"),
    },
    Recipe {
        id: 2,
        requirement: "!0 & 1 & !2 & 3 & 3+6",
        amplifier: None,
    },
    Recipe {
        id: 5,
        requirement: "0 & !1 & !2 & 3 & 3+6",
        amplifier: Some("5"),
    },
    Recipe {
        id: 6,
        requirement: "0 & !1 & 2 & !3",
        amplifier: Some("5"),
    },
    Recipe {
        id: 7,
        requirement: "!0 & !1 & 2 & 3",
        amplifier: Some("5"),
    },
    Recipe {
        id: 8,
        requirement: "0 & 1 & !2 & 3 & 3+6",
        amplifier: Some("5"),
    },
    Recipe {
        id: 10,
        requirement: "0 & !1 & !2 & !3 & 0+6",
        amplifier: Some("5"),
    },
    Recipe {
        id: 12,
        requirement: "0 & 1 & !2 & !3 & 0+6",
        amplifier: None,
    },
    Recipe {
        id: 13,
        requirement: "0 & !1 & 2 & 3 & 2+6",
        amplifier: None,
    },
    Recipe {
        id: 14,
        requirement: "!0 & 1 & 2 & 3 & 2+6",
        amplifier: None,
    },
    Recipe {
        id: 16,
        requirement: "!0 & 1 & 2 & !3 & 2+6",
        amplifier: None,
    },
    Recipe {
        id: 18,
        requirement: "!0 & !1 & !2 & 3 & 3+6",
        amplifier: None,
    },
    Recipe {
        id: 19,
        requirement: "!0 & !1 & 2 & !3 & 2+6",
        amplifier: Some("5"),
    },
];

/// `Potion.getLiquidColor()` of the potions in [`RECIPES`] (`Potion.java` constructors).
fn potion_color(id: u8) -> u32 {
    match id {
        1 => 8_171_462,
        2 => 5_926_017,
        5 => 9_643_043,
        6 => 16_262_179,
        7 => 4_393_481,
        8 => 2_293_580,
        10 => 13_458_603,
        12 => 14_981_690,
        13 => 3_035_801,
        14 => 8_356_754,
        16 => 2_039_713,
        18 => 4_738_376,
        19 => 5_149_489,
        _ => 0,
    }
}

/// The colour of the bottle for a potion damage value (`PotionHelper.getLiquidColor`).
///
/// The splash bit (16384) does not change it: the reference uses the same
/// effect list for the drinkable and the thrown potion.
pub(super) fn liquid_color(damage: i32) -> u32 {
    let effects = potion_effects(RECIPES, damage);
    if effects.is_empty() {
        WATER_COLOR
    } else {
        blend(&effects)
    }
}

/// The `(potion id, amplifier)` pairs a damage value carries
/// (`PotionHelper.getPotionEffects`, minus durations that do not affect colour).
pub(super) fn potion_effects(recipes: &[Recipe], damage: i32) -> Vec<(u8, i32)> {
    recipes
        .iter()
        .filter(|recipe| evaluate(recipe.requirement, damage) > 0)
        .map(|recipe| {
            let amplifier = recipe
                .amplifier
                .map_or(0, |expression| evaluate(expression, damage).max(0));
            (recipe.id, amplifier)
        })
        .collect()
}

/// `PotionHelper.calcPotionLiquidColor`: the mean of the effect colours, each
/// repeated `amplifier + 1` times. Single precision and truncation as in the reference.
fn blend(effects: &[(u8, i32)]) -> u32 {
    let (mut red, mut green, mut blue, mut count) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
    for &(id, amplifier) in effects {
        let color = potion_color(id);
        for _ in 0..=amplifier {
            red += channel(color >> 16);
            green += channel(color >> 8);
            blue += channel(color);
            count += 1.0;
        }
    }
    let scale = |sum: f32| (sum / count * 255.0) as u32;
    scale(red) << 16 | scale(green) << 8 | scale(blue)
}

/// One colour channel as a fraction of 255.
fn channel(shifted: u32) -> f32 {
    (shifted & 255) as f32 / 255.0
}

/// Evaluates a requirement or amplifier expression for a damage value
/// (`PotionHelper.parsePotionEffects` over the whole string).
pub(super) fn evaluate(expression: &str, damage: i32) -> i32 {
    parse(expression.as_bytes(), 0, expression.len() as isize, damage)
}

/// `parsePotionEffects(s, start, end, damage)`: `|` is the loosest operator
/// (first positive alternative), then `&` (both sides positive, the larger
/// wins), then a sum of terms.
fn parse(text: &[u8], start: isize, end: isize, damage: i32) -> i32 {
    if start >= text.len() as isize || end < 0 || start >= end {
        return 0;
    }
    if let Some(bar) = find(text, b'|', start, end) {
        let left = parse(text, start, bar - 1, damage);
        return if left > 0 {
            left
        } else {
            parse(text, bar + 1, end, damage).max(0)
        };
    }
    if let Some(amp) = find(text, b'&', start, end) {
        let left = parse(text, start, amp - 1, damage);
        if left <= 0 {
            return 0;
        }
        let right = parse(text, amp + 1, end, damage);
        return if right <= 0 { 0 } else { left.max(right) };
    }
    sum_terms(&text[start as usize..end as usize], damage)
}

/// The position of the first `needle` in `start..end`, like `indexOf` bounded by `end`.
fn find(text: &[u8], needle: u8, start: isize, end: isize) -> Option<isize> {
    let slice = text.get(start as usize..)?;
    let position = start + slice.iter().position(|&byte| byte == needle)? as isize;
    (position < end).then_some(position)
}

/// A comparison operator in front of a bit count (`=`, `>`, `<`).
#[derive(Clone, Copy, PartialEq)]
enum Compare {
    Equal,
    Greater,
    Less,
}

/// One term under construction: `[!|-] number [* digit]` with an optional comparison.
#[derive(Default)]
struct Term {
    /// `!`: the flag must be unset.
    unset: bool,
    /// `-`: the result is negated.
    negate: bool,
    compare: Option<Compare>,
    /// After `*`: the next digit multiplies the term.
    expect_factor: bool,
    factor: Option<i32>,
    number: Option<i32>,
}

impl Term {
    /// `func_77904_a`: the value of the finished term, then reset for the next one.
    fn finish(&mut self, damage: i32) -> i32 {
        let term = std::mem::take(self);
        let number = term.number.unwrap_or(0);
        let mut value = match (term.unset, term.compare) {
            (true, _) => i32::from(!flag(damage, number)),
            (false, Some(compare)) => i32::from(match compare {
                Compare::Equal => set_flags(damage) == number,
                Compare::Greater => set_flags(damage) > number,
                Compare::Less => set_flags(damage) < number,
            }),
            (false, None) => i32::from(flag(damage, number)),
        };
        if let Some(factor) = term.factor {
            value *= factor;
        }
        if term.negate {
            value = -value;
        }
        value
    }
}

/// The sum of the terms of an expression without `|` and `&`.
fn sum_terms(text: &[u8], damage: i32) -> i32 {
    let mut term = Term::default();
    let mut sum = 0;
    for &byte in text {
        match byte {
            b'0'..=b'9' => {
                let digit = i32::from(byte - b'0');
                if term.expect_factor {
                    term.factor = Some(digit);
                } else {
                    term.number = Some(term.number.unwrap_or(0) * 10 + digit);
                }
            }
            b'*' => term.expect_factor = true,
            b'!' | b'-' | b'+' | b'=' | b'<' | b'>' => {
                if term.number.is_some() {
                    sum += term.finish(damage);
                }
                match byte {
                    b'!' => term.unset = true,
                    b'-' => term.negate = true,
                    b'=' => term.compare = Some(Compare::Equal),
                    b'<' => term.compare = Some(Compare::Less),
                    b'>' => term.compare = Some(Compare::Greater),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    if term.number.is_some() {
        sum += term.finish(damage);
    }
    sum
}

/// `PotionHelper.checkFlag`: bit `bit` of `value` (the shift wraps like Java's).
fn flag(value: i32, bit: i32) -> bool {
    value & 1_i32.wrapping_shl(bit as u32) != 0
}

/// `PotionHelper.countSetFlags`; a negative value counts as 0 like the reference loop.
fn set_flags(value: i32) -> i32 {
    value.max(0).count_ones() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY: &[Recipe] = &[
        Recipe {
            id: 1,
            requirement: "0 & !1",
            amplifier: Some("2"),
        },
        Recipe {
            id: 2,
            requirement: "1 | 2",
            amplifier: None,
        },
        Recipe {
            id: 3,
            requirement: ">2",
            amplifier: None,
        },
    ];

    #[test]
    fn requirements_combine_with_and_or() {
        assert_eq!(potion_effects(TINY, 0b001), vec![(1, 0)]);
        assert_eq!(potion_effects(TINY, 0b011), vec![(2, 0)]);
        assert_eq!(potion_effects(TINY, 0b100), vec![(2, 0)]);
        assert!(potion_effects(TINY, 0).is_empty());
    }

    #[test]
    fn amplifier_expression_counts_flags() {
        assert_eq!(potion_effects(TINY, 0b101), vec![(1, 1), (2, 0)]);
        assert_eq!(evaluate("2", 0b100), 1);
        assert_eq!(evaluate("2", 0), 0);
    }

    #[test]
    fn comparison_and_factor_and_negation() {
        assert_eq!(evaluate(">1", 0b11), 1);
        assert_eq!(evaluate(">1", 0b01), 0);
        assert_eq!(evaluate("=2", 0b11), 1);
        assert_eq!(evaluate("0+1", 0b11), 2);
        assert_eq!(evaluate("0*3", 1), 3);
        assert_eq!(evaluate("0-1", 0b01), 1);
        assert_eq!(evaluate("0-1", 0b11), 0);
        assert_eq!(evaluate("<2", 0b10), 1);
    }

    #[test]
    fn water_bottle_has_the_reference_colour() {
        assert_eq!(liquid_color(0), 0x38_5DC6);
        assert_eq!(WATER_COLOR, 3_694_022);
    }

    #[test]
    fn single_effect_potions_use_the_potion_colour() {
        // 8194 = speed (id 1), 8197 = instant health (id 6), 8201 = strength (id 5).
        assert_eq!(liquid_color(8194), 8_171_462);
        assert_eq!(liquid_color(8197), 16_262_179);
        assert_eq!(liquid_color(8201), 9_643_043);
    }

    #[test]
    fn splash_and_drinkable_share_the_colour() {
        assert_eq!(liquid_color(8194 | 16384), liquid_color(8194));
        assert_eq!(liquid_color(16421), liquid_color(8197));
    }

    #[test]
    fn amplifier_weights_a_single_effect_without_changing_it() {
        // 8226 = speed II: the same colour repeated twice averages to itself.
        assert_eq!(potion_effects(RECIPES, 8226), vec![(1, 1)]);
        assert_eq!(liquid_color(8226), 8_171_462);
    }
}

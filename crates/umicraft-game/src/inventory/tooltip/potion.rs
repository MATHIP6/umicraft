// SPDX-License-Identifier: MIT OR Apache-2.0

//! The effect lines of a potion tooltip (MCP `ItemPotion.addInformation`).
//!
//! Each effect gives one line `Name [level] [(m:ss)]`, red for a bad effect
//! and gray otherwise. Effects that change an attribute then add a blank line,
//! "When Applied:" and one `+N% Speed` style line per modifier. A potion with
//! no effect says "No Effects"; the water bottle (damage 0) adds nothing.
//!
//! The effects come from the stack's `CustomPotionEffects` list when it has
//! one, otherwise from the damage value (`ItemPotion.getEffects`).

use super::{format_amount, TooltipColor, TooltipLine, TooltipText};
use crate::items::{potion_effects, PotionEffect};
use crate::nbt::Nbt;

/// `Items.potionitem` metadata of the water bottle, which gets no lines.
const WATER_BOTTLE: i16 = 0;
/// Longest NBT effect id: `Potion.potionTypes` has 32 slots.
const POTION_SLOTS: i32 = 32;
/// Ticks per second, for the `m:ss` duration.
const TICKS_PER_SECOND: i32 = 20;
/// Effects of 20 ticks or less show no duration (`ItemPotion.addInformation`).
const MIN_SHOWN_DURATION: i32 = TICKS_PER_SECOND;

/// `AttributeModifier` operations that scale the base value, shown as percents.
const OPERATION_MULTIPLY_BASE: u8 = 1;
const OPERATION_MULTIPLY_TOTAL: u8 = 2;

/// What a potion does to an attribute, per level.
struct Modifier {
    /// `IAttribute.getAttributeUnlocalizedName()`.
    attribute: &'static str,
    /// `Potion.getAttributeModifierAmount(0, ...)`: the amount of level I,
    /// which grows linearly with the level.
    amount: f64,
    /// `AttributeModifier.getOperation()`.
    operation: u8,
}

/// The static description of one `Potion`.
struct PotionType {
    /// `Potion.getId()`.
    id: u8,
    /// `Potion.getName()`, the language key of the effect.
    name: &'static str,
    /// `Potion.isBadEffect()`.
    bad: bool,
    modifier: Option<Modifier>,
}

const fn plain(id: u8, name: &'static str, bad: bool) -> PotionType {
    PotionType {
        id,
        name,
        bad,
        modifier: None,
    }
}

const fn modifying(
    id: u8,
    name: &'static str,
    bad: bool,
    attribute: &'static str,
    amount: f64,
    operation: u8,
) -> PotionType {
    PotionType {
        id,
        name,
        bad,
        modifier: Some(Modifier {
            attribute,
            amount,
            operation,
        }),
    }
}

/// `Potion.potionTypes` of 1.8. The amounts are the ones the tooltip uses: the
/// `float` constants of `Potion`, and the overrides of `PotionAttackDamage`
/// (strength 1.3 per level, weakness -0.5 per level).
static POTIONS: [PotionType; 23] = [
    modifying(
        1,
        "potion.moveSpeed",
        false,
        "generic.movementSpeed",
        0.200_000_002_980_232_24,
        OPERATION_MULTIPLY_TOTAL,
    ),
    modifying(
        2,
        "potion.moveSlowdown",
        true,
        "generic.movementSpeed",
        -0.150_000_005_960_464_48,
        OPERATION_MULTIPLY_TOTAL,
    ),
    plain(3, "potion.digSpeed", false),
    plain(4, "potion.digSlowDown", true),
    modifying(
        5,
        "potion.damageBoost",
        false,
        "generic.attackDamage",
        1.3,
        OPERATION_MULTIPLY_TOTAL,
    ),
    plain(6, "potion.heal", false),
    plain(7, "potion.harm", true),
    plain(8, "potion.jump", false),
    plain(9, "potion.confusion", true),
    plain(10, "potion.regeneration", false),
    plain(11, "potion.resistance", false),
    plain(12, "potion.fireResistance", false),
    plain(13, "potion.waterBreathing", false),
    plain(14, "potion.invisibility", false),
    plain(15, "potion.blindness", true),
    plain(16, "potion.nightVision", false),
    plain(17, "potion.hunger", true),
    modifying(18, "potion.weakness", true, "generic.attackDamage", -0.5, 0),
    plain(19, "potion.poison", true),
    plain(20, "potion.wither", true),
    modifying(21, "potion.healthBoost", false, "generic.maxHealth", 4.0, 0),
    plain(22, "potion.absorption", false),
    plain(23, "potion.saturation", false),
];

fn potion_type(id: u8) -> Option<&'static PotionType> {
    POTIONS.iter().find(|potion| potion.id == id)
}

/// The lines a potion with this `damage` and tag adds under its name.
pub(super) fn lines(damage: i16, root: Option<&Nbt>) -> Vec<TooltipLine> {
    if damage == WATER_BOTTLE {
        return Vec::new();
    }
    let effects = effects(damage, root);
    let mut lines: Vec<TooltipLine> = effects.iter().filter_map(effect_line).collect();
    if lines.is_empty() {
        return vec![TooltipLine::plain(
            TooltipText::key("potion.empty"),
            TooltipColor::Gray,
        )];
    }
    let modifiers: Vec<TooltipLine> = effects.iter().flat_map(modifier_line).collect();
    if !modifiers.is_empty() {
        lines.push(TooltipLine::plain(
            TooltipText::literal(""),
            TooltipColor::Gray,
        ));
        lines.push(TooltipLine::plain(
            TooltipText::key("potion.effects.whenDrank"),
            TooltipColor::DarkPurple,
        ));
        lines.extend(modifiers);
    }
    lines
}

/// `ItemPotion.hasEffect`: whether a potion stack shows the enchantment glint,
/// that is whether it carries any effect. `nbt` is the raw tag of the stack.
#[must_use]
pub fn has_effects(damage: i16, nbt: Option<&[u8]>) -> bool {
    let root = nbt.and_then(|bytes| Nbt::parse(bytes).ok());
    !effects(damage, root.as_ref()).is_empty()
}

/// `ItemPotion.getEffects`.
fn effects(damage: i16, root: Option<&Nbt>) -> Vec<PotionEffect> {
    match root
        .and_then(|r| r.get("CustomPotionEffects"))
        .and_then(Nbt::as_list)
    {
        Some(custom) => custom.iter().filter_map(custom_effect).collect(),
        None => potion_effects(i32::from(damage)),
    }
}

/// `PotionEffect.readCustomPotionEffectFromNBT`: `None` for an unknown id.
fn custom_effect(entry: &Nbt) -> Option<PotionEffect> {
    let read = |key| entry.get(key).and_then(Nbt::as_int).unwrap_or(0);
    // `getByte` is signed.
    let id = i32::from(read("Id") as i8);
    if !(0..POTION_SLOTS).contains(&id) {
        return None;
    }
    let id = u8::try_from(id).ok()?;
    potion_type(id)?;
    Some(PotionEffect::new(
        id,
        read("Duration"),
        i32::from(read("Amplifier") as i8),
    ))
}

fn effect_line(effect: &PotionEffect) -> Option<TooltipLine> {
    let potion = potion_type(effect.id)?;
    let mut parts = vec![TooltipText::key(potion.name)];
    if effect.amplifier > 0 {
        parts.push(TooltipText::literal(" "));
        parts.push(TooltipText::key(format!(
            "potion.potency.{}",
            effect.amplifier
        )));
    }
    if effect.duration > MIN_SHOWN_DURATION {
        parts.push(TooltipText::literal(format!(
            " ({})",
            elapsed_time(effect.duration)
        )));
    }
    let colour = if potion.bad {
        TooltipColor::Red
    } else {
        TooltipColor::Gray
    };
    Some(TooltipLine::plain(TooltipText::Concat(parts), colour))
}

/// `StringUtils.ticksToElapsedTime`: `m:ss`.
fn elapsed_time(ticks: i32) -> String {
    let seconds = ticks / TICKS_PER_SECOND;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// The "When Applied" line of the effect, if it changes an attribute.
fn modifier_line(effect: &PotionEffect) -> Option<TooltipLine> {
    let modifier = potion_type(effect.id)?.modifier.as_ref()?;
    let amount = modifier.amount * f64::from(effect.amplifier + 1);
    let shown = if matches!(
        modifier.operation,
        OPERATION_MULTIPLY_BASE | OPERATION_MULTIPLY_TOTAL
    ) {
        amount * 100.0
    } else {
        amount
    };
    // A zero amount adds no line, as in the reference.
    let (kind, colour) = if amount > 0.0 {
        ("plus", TooltipColor::Blue)
    } else if amount < 0.0 {
        ("take", TooltipColor::Red)
    } else {
        return None;
    };
    let text = TooltipText::Translation {
        key: format!("attribute.modifier.{kind}.{}", modifier.operation),
        args: vec![
            TooltipText::literal(format_amount(shown.abs())),
            TooltipText::key(format!("attribute.name.{}", modifier.attribute)),
        ],
    };
    Some(TooltipLine::plain(text, colour))
}

#[cfg(test)]
mod tests {
    use umicraft_protocol::packets::play::ItemStack;

    use super::super::tooltip;
    use super::*;

    const POTION_ID: i16 = 373;

    fn potion(damage: i16, nbt: Option<Vec<u8>>) -> ItemStack {
        ItemStack {
            item_id: POTION_ID,
            stack_size: 1,
            damage,
            nbt,
        }
    }

    /// `{CustomPotionEffects: [{Id, Amplifier, Duration}, ...]}`.
    fn custom(effects: &[(i8, i8, i32)]) -> Vec<u8> {
        let name = |out: &mut Vec<u8>, tag: u8, name: &str| {
            out.push(tag);
            out.extend((name.len() as u16).to_be_bytes());
            out.extend(name.as_bytes());
        };
        let mut nbt = vec![10, 0, 0];
        name(&mut nbt, 9, "CustomPotionEffects");
        nbt.push(10);
        nbt.extend((effects.len() as i32).to_be_bytes());
        for &(id, amplifier, duration) in effects {
            name(&mut nbt, 1, "Id");
            nbt.push(id as u8);
            name(&mut nbt, 1, "Amplifier");
            nbt.push(amplifier as u8);
            name(&mut nbt, 3, "Duration");
            nbt.extend(duration.to_be_bytes());
            nbt.push(0);
        }
        nbt.push(0);
        nbt
    }

    fn texts(lines: &[TooltipLine]) -> Vec<&TooltipText> {
        lines.iter().map(|line| &line.text).collect()
    }

    fn name_with(parts: Vec<TooltipText>) -> TooltipText {
        TooltipText::Concat(parts)
    }

    #[test]
    fn the_water_bottle_adds_no_line() {
        assert_eq!(tooltip(&potion(0, None)).len(), 1);
    }

    #[test]
    fn a_potion_without_effects_says_so() {
        // Damage 16 is a brewing bit with no recipe: no effect.
        let lines = tooltip(&potion(16, None));
        assert_eq!(lines[1].text, TooltipText::key("potion.empty"));
        assert_eq!(lines[1].color, TooltipColor::Gray);
    }

    #[test]
    fn swiftness_shows_its_duration_and_the_speed_modifier() {
        // 8194: Potion of Swiftness, 3:00.
        let lines = tooltip(&potion(8194, None));
        assert_eq!(
            lines[1].text,
            name_with(vec![
                TooltipText::key("potion.moveSpeed"),
                TooltipText::literal(" (3:00)")
            ])
        );
        assert_eq!(lines[1].color, TooltipColor::Gray);
        assert_eq!(lines[2].text, TooltipText::literal(""));
        assert_eq!(lines[3].text, TooltipText::key("potion.effects.whenDrank"));
        assert_eq!(lines[3].color, TooltipColor::DarkPurple);
        assert_eq!(
            lines[4].text,
            TooltipText::Translation {
                key: "attribute.modifier.plus.2".into(),
                args: vec![
                    TooltipText::literal("20"),
                    TooltipText::key("attribute.name.generic.movementSpeed")
                ],
            }
        );
        assert_eq!(lines[4].color, TooltipColor::Blue);
    }

    #[test]
    fn bad_effects_are_red_and_instant_ones_have_no_duration() {
        // 16460: splash Harming (instant damage).
        let harming = tooltip(&potion(16460, None));
        assert_eq!(
            harming[1].text,
            name_with(vec![TooltipText::key("potion.harm")])
        );
        assert_eq!(harming[1].color, TooltipColor::Red);
        assert_eq!(harming.len(), 2);
    }

    #[test]
    fn a_higher_level_adds_the_potency_and_scales_the_modifier() {
        let lines = tooltip(&potion(0, Some(custom(&[(5, 1, 1800)]))));
        assert_eq!(lines.len(), 1, "damage 0 stays a water bottle");
        let lines = tooltip(&potion(1, Some(custom(&[(5, 1, 1800)]))));
        assert_eq!(
            lines[1].text,
            name_with(vec![
                TooltipText::key("potion.damageBoost"),
                TooltipText::literal(" "),
                TooltipText::key("potion.potency.1"),
                TooltipText::literal(" (1:30)"),
            ])
        );
        assert_eq!(
            lines[4].text,
            TooltipText::Translation {
                key: "attribute.modifier.plus.2".into(),
                args: vec![
                    TooltipText::literal("260"),
                    TooltipText::key("attribute.name.generic.attackDamage")
                ],
            }
        );
    }

    #[test]
    fn a_negative_modifier_is_red_and_positive_in_the_text() {
        let weakness = tooltip(&potion(1, Some(custom(&[(18, 0, 1200)]))));
        assert_eq!(
            weakness[4].text,
            TooltipText::Translation {
                key: "attribute.modifier.take.0".into(),
                args: vec![
                    TooltipText::literal("0.5"),
                    TooltipText::key("attribute.name.generic.attackDamage")
                ],
            }
        );
        assert_eq!(weakness[4].color, TooltipColor::Red);
        let slowness = tooltip(&potion(1, Some(custom(&[(2, 0, 1200)]))));
        assert!(matches!(
            &slowness[4].text,
            TooltipText::Translation { args, .. } if args[0] == TooltipText::literal("15")
        ));
    }

    #[test]
    fn short_effects_show_no_duration() {
        let lines = tooltip(&potion(1, Some(custom(&[(10, 0, 20)]))));
        assert_eq!(
            lines[1].text,
            name_with(vec![TooltipText::key("potion.regeneration")])
        );
    }

    #[test]
    fn unknown_custom_ids_are_skipped_and_an_empty_list_says_no_effects() {
        let lines = tooltip(&potion(
            1,
            Some(custom(&[(40, 0, 600), (0, 0, 600), (-1, 0, 600)])),
        ));
        assert_eq!(texts(&lines[1..]), [&TooltipText::key("potion.empty")]);
        let empty = tooltip(&potion(8194, Some(custom(&[]))));
        assert_eq!(texts(&empty[1..]), [&TooltipText::key("potion.empty")]);
    }

    #[test]
    fn the_hide_additional_flag_hides_the_effects() {
        let mut nbt = vec![10, 0, 0, 3, 0, 9];
        nbt.extend(b"HideFlags");
        nbt.extend(32_i32.to_be_bytes());
        nbt.push(0);
        assert_eq!(tooltip(&potion(8194, Some(nbt))).len(), 1);
    }

    #[test]
    fn durations_format_as_minutes_and_padded_seconds() {
        assert_eq!(elapsed_time(3600), "3:00");
        assert_eq!(elapsed_time(21 * 20), "0:21");
        assert_eq!(elapsed_time(9 * 20 + 19), "0:09");
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The attribute block of a tooltip (MCP `ItemStack.getTooltip` over
//! `getAttributeModifiers`): a blank line, then one blue `+N` or red `-N` line
//! per modifier.
//!
//! The modifiers are the stack's own `AttributeModifiers` NBT list when it has
//! one, and the item's otherwise (the attack damage of swords and tools). Both
//! sit in a Guava `HashMultimap`, so the lines come in the iteration order of
//! Java 8 hash tables ([`hash_order`]), not in list order.

use super::{TooltipColor, TooltipLine, TooltipText};
use crate::item_nbt::ItemDisplay;
use crate::items::{self, ItemKind, Tier, ToolKind};
use crate::nbt::Nbt;

/// Enchantment id of sharpness (`Enchantment.sharpness`).
const SHARPNESS_ID: i16 = 16;
/// Attack damage per sharpness level (`EnchantmentDamage.calcDamageByCreature`
/// for `EnumCreatureAttribute.UNDEFINED`).
const SHARPNESS_PER_LEVEL: f64 = 1.25;
/// `Item.itemModifierUUID`, CB3F55D3-645C-4F38-A497-9C13A33DB5CF: the
/// modifier of an item's own attack damage, which gets the sharpness bonus.
const ITEM_MODIFIER: (i64, i64) = (
    0xCB3F_55D3_645C_4F38_u64 as i64,
    0xA497_9C13_A33D_B5CF_u64 as i64,
);
/// The attack damage attribute.
const ATTACK_DAMAGE: &str = "generic.attackDamage";
/// Initial table sizes of the multimap: `new HashMap()` for the keys and
/// `Sets.newHashSetWithExpectedSize(2)` (a capacity of 3, so 4) per key.
const KEY_TABLE: usize = 16;
const VALUE_TABLE: usize = 4;

/// One `AttributeModifier` with the attribute it modifies.
#[derive(Debug, Clone, PartialEq)]
struct Modifier {
    attribute: String,
    uuid: (i64, i64),
    amount: f64,
    operation: i32,
}

/// The attribute lines of a stack, nothing when it has no modifier.
pub(super) fn lines(
    def: Option<&items::ItemDef>,
    display: &ItemDisplay,
    root: Option<&Nbt>,
) -> Vec<TooltipLine> {
    let modifiers = match root.and_then(|tag| tag.get("AttributeModifiers")) {
        Some(Nbt::List(list)) => list.iter().filter_map(read_modifier).collect(),
        _ => item_modifiers(def),
    };
    if modifiers.is_empty() {
        return Vec::new();
    }
    let sharpness: f64 = display
        .enchantments
        .iter()
        .filter(|e| e.id == SHARPNESS_ID)
        .map(|e| f64::from(e.level) * SHARPNESS_PER_LEVEL)
        .sum();
    let mut lines = vec![TooltipLine::plain(
        TooltipText::literal(""),
        TooltipColor::Gray,
    )];
    lines.extend(hash_order(modifiers).iter().filter_map(|modifier| {
        let bonus = if modifier.uuid == ITEM_MODIFIER {
            sharpness
        } else {
            0.0
        };
        modifier_line(modifier, modifier.amount + bonus)
    }));
    lines
}

/// The `+N` or `-N` line of a modifier; percentages for operations 1 and 2.
fn modifier_line(modifier: &Modifier, amount: f64) -> Option<TooltipLine> {
    let shown = if matches!(modifier.operation, 1 | 2) {
        amount * 100.0
    } else {
        amount
    };
    let (sign, color, value) = if amount > 0.0 {
        ("plus", TooltipColor::Blue, shown)
    } else if amount < 0.0 {
        ("take", TooltipColor::Red, -shown)
    } else {
        return None;
    };
    let text = TooltipText::Translation {
        key: format!("attribute.modifier.{sign}.{}", modifier.operation),
        args: vec![
            TooltipText::Literal(format_amount(value)),
            TooltipText::key(format!("attribute.name.{}", modifier.attribute)),
        ],
    };
    Some(TooltipLine::plain(text, color))
}

/// `SharedMonsterAttributes.readAttributeModifierFromNBT` and the checks of
/// `getAttributeModifiers`: a name, an operation from 0 to 2, and a UUID
/// with neither half zero.
fn read_modifier(entry: &Nbt) -> Option<Modifier> {
    let string = |key| entry.get(key).and_then(Nbt::as_str).unwrap_or_default();
    let long = |key| entry.get(key).and_then(as_i64).unwrap_or(0);
    let operation = entry.get("Operation").and_then(as_i64).unwrap_or(0);
    let uuid = (long("UUIDMost"), long("UUIDLeast"));
    if string("Name").is_empty() || !(0..=2).contains(&operation) || uuid.0 == 0 || uuid.1 == 0 {
        return None;
    }
    Some(Modifier {
        attribute: string("AttributeName").to_string(),
        uuid,
        amount: entry.get("Amount").and_then(as_f64).unwrap_or(0.0),
        operation: operation as i32,
    })
}

/// The item's own modifiers (`Item.getItemAttributeModifiers`): the attack
/// damage of swords and tools.
fn item_modifiers(def: Option<&items::ItemDef>) -> Vec<Modifier> {
    def.and_then(|def| base_attack_damage(def.kind))
        .map(|amount| Modifier {
            attribute: ATTACK_DAMAGE.to_string(),
            uuid: ITEM_MODIFIER,
            amount,
            operation: 0,
        })
        .into_iter()
        .collect()
}

/// The attack damage an item adds to the player's own (`ItemSword.attackDamage`
/// = 4 + material bonus, `ItemTool.damageVsEntity` = class base + bonus).
fn base_attack_damage(kind: ItemKind) -> Option<f64> {
    let ItemKind::Tool(tool, tier) = kind else {
        return None;
    };
    let class_base = match tool {
        ToolKind::Sword => 4.0,
        ToolKind::Axe => 3.0,
        ToolKind::Pickaxe => 2.0,
        ToolKind::Shovel => 1.0,
        ToolKind::Hoe | ToolKind::Shears => return None,
    };
    Some(class_base + material_bonus(tier))
}

/// `Item.ToolMaterial.getDamageVsEntity`.
fn material_bonus(tier: Tier) -> f64 {
    match tier {
        Tier::Wood | Tier::Gold => 0.0,
        Tier::Stone => 1.0,
        Tier::Iron => 2.0,
        Tier::Diamond => 3.0,
    }
}

/// Formats like `DecimalFormat("#.###")`: up to three decimals, no trailing
/// zeros and no trailing point.
pub(super) fn format_amount(amount: f64) -> String {
    let fixed = format!("{amount:.3}");
    fixed.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// The entries of a `HashMultimap` built from `modifiers` in order: keys in
/// the order of a Java 8 `HashMap`, each key's modifiers in the order of its
/// `HashSet`, and a modifier equal to one already there (same UUID, same
/// attribute) dropped.
fn hash_order(modifiers: Vec<Modifier>) -> Vec<Modifier> {
    let mut keys: Vec<(String, Vec<Modifier>)> = Vec::new();
    for modifier in modifiers {
        match keys.iter_mut().find(|(key, _)| *key == modifier.attribute) {
            Some((_, values)) if values.iter().any(|v| v.uuid == modifier.uuid) => {}
            Some((_, values)) => values.push(modifier),
            None => keys.push((modifier.attribute.clone(), vec![modifier])),
        }
    }
    let keys = java_order(keys, KEY_TABLE, |(key, _)| string_hash(key));
    keys.into_iter()
        .flat_map(|(_, values)| java_order(values, VALUE_TABLE, |m| uuid_hash(m.uuid)))
        .collect()
}

/// Iteration order of a Java 8 `HashMap` that received `entries` in order:
/// by bucket of the final table, insertion order within a bucket (a resize
/// keeps the relative order of the entries it splits).
fn java_order<T>(entries: Vec<T>, initial_table: usize, hash: impl Fn(&T) -> i32) -> Vec<T> {
    let mut table = initial_table;
    // `resize` doubles the table once the size passes three quarters of it.
    while entries.len() > table * 3 / 4 {
        table *= 2;
    }
    let mut keyed: Vec<(usize, usize, T)> = entries
        .into_iter()
        .enumerate()
        .map(|(order, entry)| (spread(hash(&entry)) & (table - 1), order, entry))
        .collect();
    keyed.sort_by_key(|&(bucket, order, _)| (bucket, order));
    keyed.into_iter().map(|(_, _, entry)| entry).collect()
}

/// `HashMap.hash`: the high half folded into the low one.
fn spread(hash: i32) -> usize {
    let hash = hash as u32;
    (hash ^ (hash >> 16)) as usize
}

/// `String.hashCode` over UTF-16 code units.
fn string_hash(text: &str) -> i32 {
    text.encode_utf16().fold(0_i32, |hash, unit| {
        hash.wrapping_mul(31).wrapping_add(i32::from(unit))
    })
}

/// `UUID.hashCode`.
fn uuid_hash((most, least): (i64, i64)) -> i32 {
    let hilo = most ^ least;
    ((hilo >> 32) as i32) ^ (hilo as i32)
}

/// Any integer tag as a long (`NBTTagCompound.getLong`).
fn as_i64(tag: &Nbt) -> Option<i64> {
    match *tag {
        Nbt::Long(value) => Some(value),
        _ => tag.as_int().map(i64::from),
    }
}

/// Any number tag as a double (`NBTTagCompound.getDouble`).
fn as_f64(tag: &Nbt) -> Option<f64> {
    match *tag {
        Nbt::Float(value) => Some(f64::from(value)),
        Nbt::Double(value) => Some(value),
        _ => as_i64(tag).map(|value| value as f64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modifier(attribute: &str, uuid: (i64, i64), amount: f64, operation: i32) -> Nbt {
        Nbt::Compound(vec![
            ("AttributeName".into(), Nbt::String(attribute.into())),
            ("Name".into(), Nbt::String("x".into())),
            ("Amount".into(), Nbt::Double(amount)),
            ("Operation".into(), Nbt::Int(operation)),
            ("UUIDMost".into(), Nbt::Long(uuid.0)),
            ("UUIDLeast".into(), Nbt::Long(uuid.1)),
        ])
    }

    fn root(list: Vec<Nbt>) -> Nbt {
        Nbt::Compound(vec![("AttributeModifiers".into(), Nbt::List(list))])
    }

    fn texts(lines: &[TooltipLine]) -> Vec<(String, TooltipColor)> {
        lines
            .iter()
            .map(|line| match &line.text {
                TooltipText::Translation { key, args } => (format!("{key} {:?}", args), line.color),
                other => (format!("{other:?}"), line.color),
            })
            .collect()
    }

    #[test]
    fn the_java_hashes_match_the_reference() {
        assert_eq!(string_hash("hello"), 99_162_322);
        assert_eq!(string_hash("ab"), 3105);
        assert_eq!(uuid_hash((1, 0)), 1);
        assert_eq!(uuid_hash((1 << 32, 0)), 1);
    }

    #[test]
    fn nbt_modifiers_replace_the_item_ones() {
        let sword = items::lookup(276);
        let tag = root(vec![
            modifier("generic.movementSpeed", (1, 1), 0.1, 1),
            modifier("generic.maxHealth", (2, 2), -4.0, 0),
        ]);
        let lines = lines(sword, &ItemDisplay::default(), Some(&tag));
        let shown = texts(&lines[1..]);
        assert_eq!(shown.len(), 2);
        assert!(shown
            .iter()
            .any(|(text, color)| text.contains("attribute.modifier.plus.1")
                && text.contains("\"10\"")
                && *color == TooltipColor::Blue));
        assert!(shown
            .iter()
            .any(|(text, color)| text.contains("attribute.modifier.take.0")
                && text.contains("\"4\"")
                && *color == TooltipColor::Red));
    }

    #[test]
    fn keys_come_in_java_hash_order() {
        // "a" hashes to 97 (bucket 1) and "b" to 98 (bucket 2): "a" comes
        // first whatever the order of the list.
        let order = |names: &[&str]| -> Vec<String> {
            let mods = names
                .iter()
                .enumerate()
                .map(|(i, n)| Modifier {
                    attribute: (*n).to_string(),
                    uuid: (i as i64 + 1, 1),
                    amount: 1.0,
                    operation: 0,
                })
                .collect();
            hash_order(mods).into_iter().map(|m| m.attribute).collect()
        };
        assert_eq!(order(&["b", "a"]), ["a", "b"]);
        assert_eq!(order(&["a", "b"]), ["a", "b"]);
    }

    #[test]
    fn invalid_modifiers_are_dropped() {
        let tag = root(vec![
            modifier("generic.maxHealth", (0, 5), 4.0, 0),
            modifier("generic.maxHealth", (3, 3), 4.0, 3),
        ]);
        assert!(lines(None, &ItemDisplay::default(), Some(&tag)).is_empty());
    }

    #[test]
    fn a_sword_shows_its_attack_damage_with_sharpness() {
        let display = ItemDisplay {
            enchantments: vec![crate::item_nbt::Enchantment { id: 16, level: 2 }],
            ..ItemDisplay::default()
        };
        let lines = lines(items::lookup(276), &display, None);
        let shown = texts(&lines);
        assert!(shown[1].0.contains("\"9.5\""), "{shown:?}");
    }
}

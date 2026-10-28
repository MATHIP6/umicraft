// SPDX-License-Identifier: MIT OR Apache-2.0

//! The item tooltip text (MCP `ItemStack.getTooltip`, non-advanced mode).
//!
//! [`tooltip`] is a pure function from a wire [`ItemStack`] to coloured
//! [`TooltipLine`]s. Text is returned as [`TooltipText`] translation keys, not
//! as localised strings, so the function needs no language files and the
//! renderer resolves the keys against whichever language resource it loads.
//!
//! ## What is ported
//!
//! Line 0 is the display name coloured by rarity (`GuiScreen.renderToolTip`
//! colours it with `ItemStack.getRarity().rarityColor`; every other line is
//! gray unless it carries its own colour). Then, in the vanilla order: the
//! `addInformation` lines (record title, enchanted-book stored enchantments),
//! the `ench` list, the "Dyed" line, the lore, the attack-damage attribute
//! block (a blank line plus `+N Attack Damage`, which 1.8 shows in normal
//! mode), and `Unbreakable`. The `HideFlags` bits 1, 2, 4 and 32 are honoured.
//! Advanced-only lines (id, durability, NBT size) are never produced.
//!
//! ## Limitations
//!
//! * Item names use the key `item.<registry name>.name`, or
//!   `tile.<registry name>.name` for [`ItemKind::Block`]. Vanilla 1.8 uses
//!   legacy unlocalised names (`item.swordDiamond.name`) and per-metadata
//!   sub-names (`tile.wool.red.name`); the language resource must be keyed by
//!   this scheme and metadata variants are not distinguished.
//! * Only sword, pickaxe, axe and shovel attack damage is produced (hoes and
//!   shears have no attribute modifier in 1.8). A stack-level
//!   `AttributeModifiers` NBT list that replaces the item's own modifiers is
//!   ignored.
//! * `CanDestroy` / `CanPlaceOn`, potion effects, books, maps (apart from the
//!   ` #<damage>` name suffix), fireworks and banners add no lines.
//! * The stack NBT is parsed twice (once here, once by [`ItemDisplay`]); a
//!   tooltip is built per hover, not per frame of bulk data.

use umicraft_protocol::packets::play::ItemStack;

use crate::item_nbt::{Enchantment, ItemDisplay};
use crate::items::{self, ItemKind, Tier, ToolKind};
use crate::nbt::Nbt;

/// Registry name of the golden apple (`ItemAppleGold`).
const GOLDEN_APPLE: &str = "golden_apple";
/// Registry name of the filled map, whose name gets a ` #<damage>` suffix.
const FILLED_MAP: &str = "filled_map";
/// Registry name of the enchanted book (`ItemEnchantedBook`).
const ENCHANTED_BOOK: &str = "enchanted_book";
/// Enchantment id of sharpness (`Enchantment.sharpness`).
const SHARPNESS_ID: i16 = 16;
/// Extra attack damage per sharpness level (`EnchantmentDamage`, type 0).
const SHARPNESS_PER_LEVEL: f64 = 1.25;
/// Highest level that has an `enchantment.level.N` translation.
const MAX_NAMED_LEVEL: u16 = 10;

const HIDE_ENCHANTMENTS: i32 = 1;
const HIDE_ATTRIBUTES: i32 = 2;
const HIDE_UNBREAKABLE: i32 = 4;
const HIDE_ADDITIONAL: i32 = 32;

/// A chat colour (MCP `EnumChatFormatting`, the 16 colour entries).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TooltipColor {
    /// `§0`.
    Black,
    /// `§1`.
    DarkBlue,
    /// `§2`.
    DarkGreen,
    /// `§3`.
    DarkAqua,
    /// `§4`.
    DarkRed,
    /// `§5`.
    DarkPurple,
    /// `§6`.
    Gold,
    /// `§7`, the default tooltip colour.
    Gray,
    /// `§8`.
    DarkGray,
    /// `§9`.
    Blue,
    /// `§a`.
    Green,
    /// `§b`.
    Aqua,
    /// `§c`.
    Red,
    /// `§d`.
    LightPurple,
    /// `§e`.
    Yellow,
    /// `§f`.
    White,
}

impl TooltipColor {
    /// The character after the section sign in a formatting code.
    pub fn code(self) -> char {
        match self {
            Self::Black => '0',
            Self::DarkBlue => '1',
            Self::DarkGreen => '2',
            Self::DarkAqua => '3',
            Self::DarkRed => '4',
            Self::DarkPurple => '5',
            Self::Gold => '6',
            Self::Gray => '7',
            Self::DarkGray => '8',
            Self::Blue => '9',
            Self::Green => 'a',
            Self::Aqua => 'b',
            Self::Red => 'c',
            Self::LightPurple => 'd',
            Self::Yellow => 'e',
            Self::White => 'f',
        }
    }

    /// The `0xRRGGBB` value the font renderer draws the colour with
    /// (`FontRenderer.colorCode` without the shadow darkening).
    pub fn rgb(self) -> u32 {
        match self {
            Self::Black => 0x00_00_00,
            Self::DarkBlue => 0x00_00_AA,
            Self::DarkGreen => 0x00_AA_00,
            Self::DarkAqua => 0x00_AA_AA,
            Self::DarkRed => 0xAA_00_00,
            Self::DarkPurple => 0xAA_00_AA,
            Self::Gold => 0xFF_AA_00,
            Self::Gray => 0xAA_AA_AA,
            Self::DarkGray => 0x55_55_55,
            Self::Blue => 0x55_55_FF,
            Self::Green => 0x55_FF_55,
            Self::Aqua => 0x55_FF_FF,
            Self::Red => 0xFF_55_55,
            Self::LightPurple => 0xFF_55_FF,
            Self::Yellow => 0xFF_FF_55,
            Self::White => 0xFF_FF_FF,
        }
    }
}

/// An item rarity (MCP `EnumRarity`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rarity {
    /// White.
    Common,
    /// Yellow.
    Uncommon,
    /// Aqua.
    Rare,
    /// Light purple.
    Epic,
}

impl Rarity {
    /// The colour of the name line (`EnumRarity.rarityColor`).
    pub fn color(self) -> TooltipColor {
        match self {
            Self::Common => TooltipColor::White,
            Self::Uncommon => TooltipColor::Yellow,
            Self::Rare => TooltipColor::Aqua,
            Self::Epic => TooltipColor::LightPurple,
        }
    }
}

/// The text of a tooltip line, unresolved against any language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TooltipText {
    /// Text shown as is (custom names, lore, numbers, fallbacks).
    Literal(String),
    /// A language key with positional `%s` arguments.
    Translation {
        /// The language key, e.g. `item.diamond_sword.name`.
        key: String,
        /// The arguments, in `%1$s`, `%2$s` order.
        args: Vec<TooltipText>,
    },
    /// Several texts shown back to back (e.g. enchantment name, space, level).
    Concat(Vec<TooltipText>),
}

impl TooltipText {
    fn key(key: impl Into<String>) -> Self {
        Self::Translation {
            key: key.into(),
            args: Vec::new(),
        }
    }

    fn literal(text: impl Into<String>) -> Self {
        Self::Literal(text.into())
    }
}

/// One line of a tooltip with its colour and style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TooltipLine {
    /// What the line says.
    pub text: TooltipText,
    /// The line colour.
    pub color: TooltipColor,
    /// Whether the line is drawn italic (custom names, lore, "Dyed").
    pub italic: bool,
}

impl TooltipLine {
    fn plain(text: TooltipText, color: TooltipColor) -> Self {
        Self {
            text,
            color,
            italic: false,
        }
    }

    fn italic(text: TooltipText, color: TooltipColor) -> Self {
        Self {
            text,
            color,
            italic: true,
        }
    }
}

/// Builds the normal-mode (F3+H off) tooltip of `stack`.
///
/// The first line is the name; an unknown item id yields a literal fallback
/// name and common rarity. An empty stack (`item_id < 0`) still returns the
/// fallback line, the caller decides whether to show a tooltip at all.
///
/// ```
/// use umicraft_game::inventory::tooltip::{tooltip, TooltipText};
/// use umicraft_protocol::packets::play::ItemStack;
///
/// let stack = ItemStack { item_id: 276, stack_size: 1, damage: 0, nbt: None };
/// let lines = tooltip(&stack);
/// assert_eq!(lines[0].text, TooltipText::Translation {
///     key: "item.diamond_sword.name".into(),
///     args: vec![],
/// });
/// ```
pub fn tooltip(stack: &ItemStack) -> Vec<TooltipLine> {
    let bytes = stack.nbt.as_deref();
    let root = bytes.and_then(|b| Nbt::parse(b).ok());
    let display = bytes.map(ItemDisplay::from_nbt).unwrap_or_default();
    let hide = root
        .as_ref()
        .and_then(|r| r.get("HideFlags"))
        .and_then(Nbt::as_int)
        .unwrap_or(0);
    let def = items::lookup(i32::from(stack.item_id));

    let mut lines = vec![name_line(
        stack,
        def,
        &display,
        rarity(stack, def, root.as_ref()),
    )];
    if hide & HIDE_ADDITIONAL == 0 {
        lines.extend(additional_lines(def, root.as_ref()));
    }
    if hide & HIDE_ENCHANTMENTS == 0 {
        lines.extend(display.enchantments.iter().filter_map(enchantment_line));
    }
    if has_dyed_colour(root.as_ref()) {
        lines.push(TooltipLine::italic(
            TooltipText::key("item.dyed"),
            TooltipColor::Gray,
        ));
    }
    lines.extend(display.lore.iter().map(|lore| {
        TooltipLine::italic(
            TooltipText::literal(lore.as_str()),
            TooltipColor::DarkPurple,
        )
    }));
    if hide & HIDE_ATTRIBUTES == 0 {
        lines.extend(attack_damage_lines(def, &display));
    }
    if hide & HIDE_UNBREAKABLE == 0 && is_unbreakable(root.as_ref()) {
        lines.push(TooltipLine::plain(
            TooltipText::key("item.unbreakable"),
            TooltipColor::Blue,
        ));
    }
    lines
}

/// The rarity of `stack` (`ItemStack.getRarity` -> `Item.getRarity`).
///
/// Overrides ported: golden apple (rare, epic for damage > 0), records (rare),
/// enchanted book (uncommon with stored enchantments). Everything else is
/// rare when it has an `ench` list and common otherwise.
fn rarity(stack: &ItemStack, def: Option<&items::ItemDef>, root: Option<&Nbt>) -> Rarity {
    let name = def.map(|d| d.name).unwrap_or_default();
    let enchanted = root
        .and_then(|r| r.get("ench"))
        .and_then(Nbt::as_list)
        .is_some();
    if name == GOLDEN_APPLE {
        return if stack.damage == 0 {
            Rarity::Rare
        } else {
            Rarity::Epic
        };
    }
    if name.starts_with("record_") {
        return Rarity::Rare;
    }
    if name == ENCHANTED_BOOK && !stored_enchantments(root).is_empty() {
        return Rarity::Uncommon;
    }
    if enchanted {
        Rarity::Rare
    } else {
        Rarity::Common
    }
}

/// Line 0: the custom name in italic, else the translated item name
/// (`ItemStack.getDisplayName` and the filled-map ` #damage` suffix).
fn name_line(
    stack: &ItemStack,
    def: Option<&items::ItemDef>,
    display: &ItemDisplay,
    rarity: Rarity,
) -> TooltipLine {
    let colour = rarity.color();
    if let Some(custom) = &display.custom_name {
        return TooltipLine::italic(TooltipText::literal(custom.as_str()), colour);
    }
    let Some(def) = def else {
        let fallback = format!("Unknown item {}", stack.item_id);
        return TooltipLine::plain(TooltipText::Literal(fallback), colour);
    };
    let prefix = if def.kind == ItemKind::Block {
        "tile"
    } else {
        "item"
    };
    let name = TooltipText::key(format!("{prefix}.{}.name", def.name));
    let text = if def.name == FILLED_MAP {
        let suffix = TooltipText::literal(format!(" #{}", stack.damage));
        TooltipText::Concat(vec![name, suffix])
    } else {
        name
    };
    TooltipLine::plain(text, colour)
}

/// The lines `Item.addInformation` adds in 1.8: the record title and the
/// stored enchantments of an enchanted book.
fn additional_lines(def: Option<&items::ItemDef>, root: Option<&Nbt>) -> Vec<TooltipLine> {
    let Some(def) = def else { return Vec::new() };
    if let Some(record) = def.name.strip_prefix("record_") {
        let key = format!("item.record.{record}.desc");
        return vec![TooltipLine::plain(
            TooltipText::key(key),
            TooltipColor::Gray,
        )];
    }
    if def.name == ENCHANTED_BOOK {
        return stored_enchantments(root)
            .iter()
            .filter_map(enchantment_line)
            .collect();
    }
    Vec::new()
}

/// The `StoredEnchantments` list of an enchanted book
/// (`ItemEnchantedBook.getEnchantments`).
fn stored_enchantments(root: Option<&Nbt>) -> Vec<Enchantment> {
    let list = root
        .and_then(|r| r.get("StoredEnchantments"))
        .and_then(Nbt::as_list)
        .unwrap_or_default();
    list.iter()
        .map(|entry| {
            let short = |key| entry.get(key).and_then(Nbt::as_int).unwrap_or(0);
            Enchantment {
                id: short("id") as i16,
                level: short("lvl").clamp(0, i32::from(u16::MAX)) as u16,
            }
        })
        .collect()
}

/// One enchantment line (`Enchantment.getTranslatedName`), `None` for an id
/// the 1.8 table does not know (vanilla skips those).
fn enchantment_line(enchantment: &Enchantment) -> Option<TooltipLine> {
    let key = enchantment_key(enchantment.id)?;
    let level = if (1..=MAX_NAMED_LEVEL).contains(&enchantment.level) {
        TooltipText::key(format!("enchantment.level.{}", enchantment.level))
    } else {
        TooltipText::literal(enchantment.level.to_string())
    };
    let text = TooltipText::Concat(vec![
        TooltipText::key(key),
        TooltipText::literal(" "),
        level,
    ]);
    Some(TooltipLine::plain(text, TooltipColor::Gray))
}

/// The language key of an enchantment id (`Enchantment.getName`).
///
/// Protection and damage enchantments share a family key per type
/// (`EnchantmentProtection`, `EnchantmentDamage`), the others use their
/// `setName` string.
fn enchantment_key(id: i16) -> Option<&'static str> {
    Some(match id {
        0 => "enchantment.protect.all",
        1 => "enchantment.protect.fire",
        2 => "enchantment.protect.fall",
        3 => "enchantment.protect.explosion",
        4 => "enchantment.protect.projectile",
        5 => "enchantment.oxygen",
        6 => "enchantment.waterWorker",
        7 => "enchantment.thorns",
        8 => "enchantment.waterWalker",
        16 => "enchantment.damage.all",
        17 => "enchantment.damage.undead",
        18 => "enchantment.damage.arthropods",
        19 => "enchantment.knockback",
        20 => "enchantment.fire",
        21 => "enchantment.lootBonus",
        32 => "enchantment.digging",
        33 => "enchantment.untouching",
        34 => "enchantment.durability",
        35 => "enchantment.lootBonusDigger",
        48 => "enchantment.arrowDamage",
        49 => "enchantment.arrowKnockback",
        50 => "enchantment.arrowFire",
        51 => "enchantment.arrowInfinite",
        61 => "enchantment.lootBonusFishing",
        62 => "enchantment.fishingSpeed",
        _ => return None,
    })
}

/// `display.color` is present as a `TAG_Int` (`hasKey("color", 3)`).
fn has_dyed_colour(root: Option<&Nbt>) -> bool {
    matches!(
        root.and_then(|r| r.get("display"))
            .and_then(|d| d.get("color")),
        Some(Nbt::Int(_))
    )
}

/// `Unbreakable` is a non-zero `TAG_Byte` (`NBTTagCompound.getBoolean`).
fn is_unbreakable(root: Option<&Nbt>) -> bool {
    matches!(
        root.and_then(|r| r.get("Unbreakable")),
        Some(Nbt::Byte(value)) if *value != 0
    )
}

/// The attribute block of a weapon or tool: a blank line, then
/// `+N Attack Damage` in blue. The number is the base damage plus the
/// sharpness bonus (`EnchantmentHelper.func_152377_a`), printed with
/// `DecimalFormat("#.###")`.
fn attack_damage_lines(def: Option<&items::ItemDef>, display: &ItemDisplay) -> Vec<TooltipLine> {
    let Some(base) = def.and_then(|d| base_attack_damage(d.kind)) else {
        return Vec::new();
    };
    let sharpness: f64 = display
        .enchantments
        .iter()
        .filter(|e| e.id == SHARPNESS_ID)
        .map(|e| f64::from(e.level) * SHARPNESS_PER_LEVEL)
        .sum();
    let amount = base + sharpness;
    if amount <= 0.0 {
        return Vec::new();
    }
    let text = TooltipText::Translation {
        key: "attribute.modifier.plus.0".into(),
        args: vec![
            TooltipText::Literal(format_amount(amount)),
            TooltipText::key("attribute.name.generic.attackDamage"),
        ],
    };
    vec![
        TooltipLine::plain(TooltipText::literal(""), TooltipColor::Gray),
        TooltipLine::plain(text, TooltipColor::Blue),
    ]
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
fn format_amount(amount: f64) -> String {
    let fixed = format!("{amount:.3}");
    fixed.trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes a minimal root compound from already-encoded payload bytes.
    fn compound(entries: &[Vec<u8>]) -> Vec<u8> {
        let mut out = vec![10, 0, 0];
        for entry in entries {
            out.extend_from_slice(entry);
        }
        out.push(0);
        out
    }

    fn named(tag: u8, name: &str, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![tag];
        out.extend_from_slice(&(name.len() as u16).to_be_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(payload);
        out
    }

    fn string_payload(text: &str) -> Vec<u8> {
        let mut out = (text.len() as u16).to_be_bytes().to_vec();
        out.extend_from_slice(text.as_bytes());
        out
    }

    fn string_list(name: &str, items: &[&str]) -> Vec<u8> {
        let mut payload = vec![8];
        payload.extend_from_slice(&(items.len() as i32).to_be_bytes());
        for item in items {
            payload.extend(string_payload(item));
        }
        named(9, name, &payload)
    }

    fn display(inner: Vec<Vec<u8>>) -> Vec<u8> {
        let mut payload = Vec::new();
        for entry in inner {
            payload.extend(entry);
        }
        payload.push(0);
        named(10, "display", &payload)
    }

    fn ench_list(name: &str, entries: &[(i16, i16)]) -> Vec<u8> {
        let mut payload = vec![10];
        payload.extend_from_slice(&(entries.len() as i32).to_be_bytes());
        for (id, lvl) in entries {
            payload.extend(named(2, "id", &id.to_be_bytes()));
            payload.extend(named(2, "lvl", &lvl.to_be_bytes()));
            payload.push(0);
        }
        named(9, name, &payload)
    }

    fn stack(item_id: i16, damage: i16, nbt: Option<Vec<u8>>) -> ItemStack {
        ItemStack {
            item_id,
            stack_size: 1,
            damage,
            nbt,
        }
    }

    fn key(text: &str) -> TooltipText {
        TooltipText::key(text)
    }

    #[test]
    fn plain_item_has_a_single_white_name_line() {
        let lines = tooltip(&stack(276, 0, None));
        assert_eq!(lines.len(), 3, "name, blank line, attack damage");
        assert_eq!(lines[0].text, key("item.diamond_sword.name"));
        assert_eq!(lines[0].color, TooltipColor::White);
        assert!(!lines[0].italic);
        let apple = tooltip(&stack(260, 0, None));
        assert_eq!(apple.len(), 1);
    }

    #[test]
    fn blocks_use_the_tile_prefix() {
        let lines = tooltip(&stack(1, 0, None));
        assert_eq!(lines[0].text, key("tile.stone.name"));
    }

    #[test]
    fn unknown_item_has_a_literal_common_name() {
        let lines = tooltip(&stack(30_000, 0, None));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, TooltipText::literal("Unknown item 30000"));
        assert_eq!(lines[0].color, TooltipColor::White);
        assert_eq!(tooltip(&stack(-1, 0, None)).len(), 1);
    }

    #[test]
    fn custom_name_is_italic_literal() {
        let nbt = compound(&[display(vec![named(
            8,
            "Name",
            &string_payload("Excalibur"),
        )])]);
        let lines = tooltip(&stack(260, 0, Some(nbt)));
        assert_eq!(lines[0].text, TooltipText::literal("Excalibur"));
        assert!(lines[0].italic);
    }

    #[test]
    fn lore_is_dark_purple_italic_after_the_name() {
        let nbt = compound(&[display(vec![string_list("Lore", &["one", "two"])])]);
        let lines = tooltip(&stack(260, 0, Some(nbt)));
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1].text, TooltipText::literal("one"));
        assert_eq!(lines[2].text, TooltipText::literal("two"));
        assert!(lines[1..]
            .iter()
            .all(|l| l.italic && l.color == TooltipColor::DarkPurple));
    }

    #[test]
    fn enchantments_list_names_and_levels() {
        let nbt = compound(&[ench_list("ench", &[(34, 3), (21, 11), (99, 1)])]);
        let lines = tooltip(&stack(257, 0, Some(nbt)));
        assert_eq!(lines[0].color, TooltipColor::Aqua, "enchanted is rare");
        // iron pickaxe: name, unbreaking, looting, blank, attack damage
        assert_eq!(
            lines[1].text,
            TooltipText::Concat(vec![
                key("enchantment.durability"),
                TooltipText::literal(" "),
                key("enchantment.level.3"),
            ])
        );
        assert_eq!(
            lines[2].text,
            TooltipText::Concat(vec![
                key("enchantment.lootBonus"),
                TooltipText::literal(" "),
                TooltipText::literal("11"),
            ]),
            "levels above 10 print the number"
        );
        assert!(lines[1..3].iter().all(|l| l.color == TooltipColor::Gray));
        assert_eq!(lines.len(), 5, "unknown enchantment 99 is skipped");
    }

    #[test]
    fn protection_and_damage_use_family_keys() {
        assert_eq!(enchantment_key(0), Some("enchantment.protect.all"));
        assert_eq!(enchantment_key(17), Some("enchantment.damage.undead"));
        assert_eq!(enchantment_key(62), Some("enchantment.fishingSpeed"));
        assert_eq!(enchantment_key(9), None);
    }

    #[test]
    fn rarity_colours_follow_the_item() {
        let colour = |s| tooltip(&s)[0].color;
        assert_eq!(colour(stack(322, 0, None)), TooltipColor::Aqua);
        assert_eq!(colour(stack(322, 1, None)), TooltipColor::LightPurple);
        assert_eq!(colour(stack(2256, 0, None)), TooltipColor::Aqua, "record");
        let book = compound(&[ench_list("StoredEnchantments", &[(16, 1)])]);
        assert_eq!(colour(stack(403, 0, Some(book))), TooltipColor::Yellow);
        assert_eq!(colour(stack(403, 0, None)), TooltipColor::White);
        assert_eq!(Rarity::Epic.color().code(), 'd');
    }

    #[test]
    fn enchanted_book_lists_stored_enchantments() {
        let book = compound(&[ench_list("StoredEnchantments", &[(16, 2)])]);
        let lines = tooltip(&stack(403, 0, Some(book)));
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[1].text,
            TooltipText::Concat(vec![
                key("enchantment.damage.all"),
                TooltipText::literal(" "),
                key("enchantment.level.2"),
            ])
        );
    }

    #[test]
    fn record_shows_its_title() {
        let lines = tooltip(&stack(2256, 0, None));
        assert_eq!(lines[1].text, key("item.record.13.desc"));
    }

    #[test]
    fn attack_damage_lines_match_vanilla_numbers() {
        let damage = |id| {
            let lines = tooltip(&stack(id, 0, None));
            assert_eq!(lines[1].text, TooltipText::literal(""));
            assert_eq!(lines[2].color, TooltipColor::Blue);
            match &lines[2].text {
                TooltipText::Translation { key, args } => {
                    assert_eq!(key, "attribute.modifier.plus.0");
                    assert_eq!(
                        args[1],
                        TooltipText::key("attribute.name.generic.attackDamage")
                    );
                    args[0].clone()
                }
                other => panic!("unexpected {other:?}"),
            }
        };
        assert_eq!(damage(276), TooltipText::literal("7")); // diamond sword
        assert_eq!(damage(268), TooltipText::literal("4")); // wooden sword
        assert_eq!(damage(283), TooltipText::literal("4")); // golden sword
        assert_eq!(damage(257), TooltipText::literal("4")); // iron pickaxe
        assert_eq!(damage(279), TooltipText::literal("6")); // diamond axe
        assert_eq!(damage(256), TooltipText::literal("3")); // iron shovel
    }

    #[test]
    fn sharpness_adds_to_the_attack_damage() {
        let nbt = compound(&[ench_list("ench", &[(16, 5)])]);
        let lines = tooltip(&stack(276, 0, Some(nbt)));
        let last = lines.last().expect("line");
        assert_eq!(
            last.text,
            TooltipText::Translation {
                key: "attribute.modifier.plus.0".into(),
                args: vec![
                    TooltipText::literal("13.25"),
                    key("attribute.name.generic.attackDamage"),
                ],
            }
        );
    }

    #[test]
    fn hoes_shears_and_food_have_no_attribute_block() {
        for id in [290, 359, 260] {
            assert_eq!(tooltip(&stack(id, 0, None)).len(), 1, "item {id}");
        }
    }

    #[test]
    fn unbreakable_requires_a_nonzero_byte() {
        let flagged = compound(&[named(1, "Unbreakable", &[1])]);
        let lines = tooltip(&stack(260, 0, Some(flagged)));
        assert_eq!(lines[1].text, key("item.unbreakable"));
        assert_eq!(lines[1].color, TooltipColor::Blue);
        let off = compound(&[named(1, "Unbreakable", &[0])]);
        assert_eq!(tooltip(&stack(260, 0, Some(off))).len(), 1);
        let wrong_type = compound(&[named(3, "Unbreakable", &1_i32.to_be_bytes())]);
        assert_eq!(tooltip(&stack(260, 0, Some(wrong_type))).len(), 1);
    }

    #[test]
    fn hide_flags_suppress_sections() {
        let all = compound(&[
            ench_list("ench", &[(34, 1)]),
            named(1, "Unbreakable", &[1]),
            named(3, "HideFlags", &(1_i32 | 4).to_be_bytes()),
        ]);
        let lines = tooltip(&stack(276, 0, Some(all)));
        // name plus the attribute block only
        assert_eq!(lines.len(), 3);
        let no_attr = compound(&[named(3, "HideFlags", &2_i32.to_be_bytes())]);
        assert_eq!(tooltip(&stack(276, 0, Some(no_attr))).len(), 1);
    }

    #[test]
    fn dyed_leather_shows_the_italic_dyed_line() {
        let nbt = compound(&[display(vec![named(
            3,
            "color",
            &0xFF0000_i32.to_be_bytes(),
        )])]);
        let lines = tooltip(&stack(298, 0, Some(nbt)));
        assert_eq!(lines[1].text, key("item.dyed"));
        assert!(lines[1].italic);
    }

    #[test]
    fn filled_map_name_gets_the_damage_suffix() {
        let lines = tooltip(&stack(358, 7, None));
        assert_eq!(
            lines[0].text,
            TooltipText::Concat(vec![
                key("item.filled_map.name"),
                TooltipText::literal(" #7")
            ])
        );
    }

    #[test]
    fn malformed_nbt_degrades_to_the_plain_tooltip() {
        let lines = tooltip(&stack(260, 0, Some(vec![1, 2, 3])));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, key("item.apple.name"));
    }

    #[test]
    fn colour_codes_and_rgb_cover_the_sixteen_colours() {
        assert_eq!(TooltipColor::Gray.code(), '7');
        assert_eq!(TooltipColor::Gold.rgb(), 0xFF_AA_00);
        assert_eq!(TooltipColor::Black.code(), '0');
        assert_eq!(TooltipColor::White.rgb(), 0xFF_FF_FF);
    }

    #[test]
    fn amounts_are_formatted_like_decimal_format() {
        assert_eq!(format_amount(7.0), "7");
        assert_eq!(format_amount(13.25), "13.25");
        assert_eq!(format_amount(0.5), "0.5");
    }
}

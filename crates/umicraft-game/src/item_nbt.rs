// SPDX-License-Identifier: MIT OR Apache-2.0

//! The display data an item stack carries in its NBT.
//!
//! Custom name, lore, enchantments, leather colour and skull owner are read
//! by MCP `ItemStack.getDisplayName` / `getTooltip`, `ItemArmor.getColor`,
//! `ItemStack.getEnchantmentTagList` and `ItemSkull.getItemStackDisplayName`.
//! The NBT comes from the server, so a malformed compound yields the default
//! (empty) display instead of an error: a bad tooltip must never stop the
//! client.

use crate::nbt::Nbt;

/// One entry of the `ench` list (MCP `Enchantment` id and level).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Enchantment {
    /// The enchantment id (MCP `Enchantment.effectId`).
    pub id: i16,
    /// The level, `0` when the tag is missing.
    pub level: u16,
}

/// What an item stack shows beyond its id: names, lore, glint and colour.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemDisplay {
    /// `display.Name`, replacing the registry name when present.
    pub custom_name: Option<String>,
    /// `display.Lore`, one tooltip line each.
    pub lore: Vec<String>,
    /// The `ench` list, in stored order.
    pub enchantments: Vec<Enchantment>,
    /// `display.color`, the dyed colour of leather armour (`0xRRGGBB`).
    pub leather_color: Option<u32>,
    /// `SkullOwner` (a name, or the `Name` of the owner compound).
    pub skull_owner: Option<String>,
}

impl ItemDisplay {
    /// Reads the display data from raw stack NBT.
    ///
    /// Returns the default display for malformed NBT.
    pub fn from_nbt(bytes: &[u8]) -> Self {
        Nbt::parse(bytes)
            .map(|root| Self::from_tree(&root))
            .unwrap_or_default()
    }

    fn from_tree(root: &Nbt) -> Self {
        let display = root.get("display");
        Self {
            custom_name: display
                .and_then(|d| d.get("Name"))
                .and_then(Nbt::as_str)
                .map(str::to_owned),
            lore: display
                .and_then(|d| d.get("Lore"))
                .and_then(Nbt::as_list)
                .map(|lines| {
                    lines
                        .iter()
                        .filter_map(Nbt::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            enchantments: root
                .get("ench")
                .and_then(Nbt::as_list)
                .map(|list| list.iter().map(enchantment).collect())
                .unwrap_or_default(),
            leather_color: display
                .and_then(|d| d.get("color"))
                .and_then(Nbt::as_int)
                // A colour is 24 bits; the sign bit of a malformed int is dropped.
                .map(|color| color as u32 & 0x00FF_FFFF),
            skull_owner: root.get("SkullOwner").and_then(skull_owner),
        }
    }

    /// Whether the stack shows the enchantment glint (`ItemStack.hasEffect`
    /// for plain items; items with an intrinsic glint are the caller's case).
    pub fn has_glint(&self) -> bool {
        !self.enchantments.is_empty()
    }
}

fn enchantment(entry: &Nbt) -> Enchantment {
    let short = |key| entry.get(key).and_then(Nbt::as_int).unwrap_or(0);
    Enchantment {
        id: short("id") as i16,
        level: short("lvl").clamp(0, i32::from(u16::MAX)) as u16,
    }
}

fn skull_owner(tag: &Nbt) -> Option<String> {
    let name = tag
        .as_str()
        .or_else(|| tag.get("Name").and_then(Nbt::as_str))?;
    Some(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string_tag(name: &str, value: &str) -> Vec<u8> {
        let mut out = vec![8];
        out.extend((name.len() as u16).to_be_bytes());
        out.extend(name.as_bytes());
        out.extend((value.len() as u16).to_be_bytes());
        out.extend(value.as_bytes());
        out
    }

    /// `{display: {Name, Lore: [..], color}, ench: [{id, lvl}], SkullOwner}`.
    fn full_stack() -> Vec<u8> {
        let mut nbt = vec![10, 0, 0, 10, 0, 7];
        nbt.extend(b"display");
        nbt.extend(string_tag("Name", "Sword"));
        nbt.extend([9, 0, 4]);
        nbt.extend(b"Lore");
        nbt.extend([8, 0, 0, 0, 2, 0, 1, b'a', 0, 1, b'b']);
        nbt.extend([3, 0, 5]);
        nbt.extend(b"color");
        nbt.extend(0xFF12_3456_u32.to_be_bytes());
        nbt.push(0);
        nbt.extend([9, 0, 4]);
        nbt.extend(b"ench");
        nbt.extend([10, 0, 0, 0, 1, 2, 0, 2]);
        nbt.extend(b"id");
        nbt.extend(16_i16.to_be_bytes());
        nbt.extend([2, 0, 3]);
        nbt.extend(b"lvl");
        nbt.extend(5_i16.to_be_bytes());
        nbt.push(0);
        nbt.extend(string_tag("SkullOwner", "Notch"));
        nbt.push(0);
        nbt
    }

    #[test]
    fn reads_every_display_field() {
        let display = ItemDisplay::from_nbt(&full_stack());
        assert_eq!(display.custom_name.as_deref(), Some("Sword"));
        assert_eq!(display.lore, ["a", "b"]);
        assert_eq!(display.enchantments, [Enchantment { id: 16, level: 5 }]);
        assert_eq!(display.leather_color, Some(0x12_3456));
        assert_eq!(display.skull_owner.as_deref(), Some("Notch"));
        assert!(display.has_glint());
    }

    #[test]
    fn malformed_nbt_gives_the_default_display() {
        let bytes = full_stack();
        assert_eq!(
            ItemDisplay::from_nbt(&bytes[..bytes.len() - 7]),
            ItemDisplay::default()
        );
        assert!(!ItemDisplay::default().has_glint());
    }
}

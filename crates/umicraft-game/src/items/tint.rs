// SPDX-License-Identifier: MIT OR Apache-2.0

//! Per-layer tint colours of the items whose colour is not a block property.
//!
//! The client multiplies each model layer by `Item.getColorFromItemStack`.
//! This module covers the potion bottle (`ItemPotion`), the spawn egg
//! (`ItemMonsterPlacer`) and the firework charge (`ItemFireworkCharge`, which
//! needs the stack NBT and so has its own function).

use super::egg_table::EGGS;
use super::potion_color::liquid_color;
use crate::nbt::Nbt;

const POTION: i16 = 373;
const SPAWN_EGG: i16 = 383;
const WHITE: u32 = 0xFF_FFFF;
/// `ItemFireworkCharge`: colour of a charge without a usable `Colors` array.
const FIREWORK_DEFAULT: u32 = 0x8A_8A8A;

/// The tint of layer 0 and layer 1 as `0x00RRGGBB`, `None` for an item this
/// table does not tint (the caller then applies its own rules).
///
/// * Potion (373): layer 0 is the liquid colour of the damage value, layer 1
///   (the bottle) is white (`ItemPotion.getColorFromItemStack`).
/// * Spawn egg (383): layer 0 and 1 are the primary and secondary colour of
///   the entity id in the damage; an unknown id is white on both layers
///   (`ItemMonsterPlacer.getColorFromItemStack`).
///
/// ```
/// use umicraft_game::items::tint::layer_tints;
///
/// assert_eq!(layer_tints(373, 0), Some([0x385DC6, 0xFFFFFF]));
/// assert_eq!(layer_tints(50, 0), None);
/// ```
pub fn layer_tints(item_id: i16, damage: i16) -> Option<[u32; 2]> {
    match item_id {
        POTION => Some([liquid_color(i32::from(damage)), WHITE]),
        SPAWN_EGG => Some(egg_colors(damage)),
        _ => None,
    }
}

/// The two colours of the spawn egg for entity id `damage`.
fn egg_colors(damage: i16) -> [u32; 2] {
    u8::try_from(damage)
        .ok()
        .and_then(|entity| EGGS.binary_search_by_key(&entity, |&(id, ..)| id).ok())
        .map_or([WHITE, WHITE], |index| [EGGS[index].1, EGGS[index].2])
}

/// The tint of **layer 1** of a firework charge (item 402); layer 0 is white.
///
/// `ItemFireworkCharge.getColorFromItemStack` reads `Explosion.Colors` (an int
/// array) of the stack tag: one colour is used as is, several are averaged per
/// channel (the reference averages, it does not take the first), and a missing
/// or non-int-array `Colors` (also no tag at all) gives `0x8A8A8A`. An empty
/// array divides by zero in the reference; here it gives the default too.
///
/// `nbt` is the stack's tag in the wire format; `None` means the stack has no
/// tag. Returns `None` only when the bytes are not valid NBT.
pub fn firework_charge_tint(nbt: Option<&[u8]>) -> Option<u32> {
    let Some(bytes) = nbt else {
        return Some(FIREWORK_DEFAULT);
    };
    let tag = Nbt::parse(bytes).ok()?;
    let colors = match tag
        .get("Explosion")
        .and_then(|explosion| explosion.get("Colors"))
    {
        Some(Nbt::IntArray(colors)) if !colors.is_empty() => colors,
        _ => return Some(FIREWORK_DEFAULT),
    };
    Some(average(colors))
}

/// The per-channel integer mean of `0x00RRGGBB` values; a single value is returned unchanged.
fn average(colors: &[i32]) -> u32 {
    let count = colors.len() as u32;
    let mean = |shift: u32| {
        let sum: u32 = colors.iter().map(|&c| (c as u32 >> shift) & 255).sum();
        sum / count
    };
    mean(16) << 16 | mean(8) << 8 | mean(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Root compound { Explosion: compound { Colors: int array } }.
    fn charge_nbt(colors: &[i32]) -> Vec<u8> {
        let mut bytes = vec![10, 0, 0, 10, 0, 9];
        bytes.extend_from_slice(b"Explosion");
        bytes.extend_from_slice(&[11, 0, 6]);
        bytes.extend_from_slice(b"Colors");
        bytes.extend_from_slice(&(colors.len() as i32).to_be_bytes());
        for color in colors {
            bytes.extend_from_slice(&color.to_be_bytes());
        }
        bytes.extend_from_slice(&[0, 0]);
        bytes
    }

    #[test]
    fn water_bottle_is_the_reference_blue() {
        assert_eq!(layer_tints(373, 0), Some([0x385DC6, 0xFFFFFF]));
    }

    #[test]
    fn potions_tint_layer_zero_only() {
        assert_eq!(layer_tints(373, 8194), Some([8_171_462, 0xFFFFFF]));
        assert_eq!(layer_tints(373, 8197).map(|t| t[0]), Some(16_262_179));
        assert_eq!(layer_tints(373, 8194 | 16384), layer_tints(373, 8194));
    }

    #[test]
    fn creeper_egg_uses_its_two_colours() {
        assert_eq!(layer_tints(383, 50), Some([894_731, 0]));
    }

    #[test]
    fn unknown_egg_is_white() {
        assert_eq!(layer_tints(383, 0), Some([0xFFFFFF; 2]));
        assert_eq!(layer_tints(383, -1), Some([0xFFFFFF; 2]));
    }

    #[test]
    fn other_items_are_not_tinted_here() {
        assert_eq!(layer_tints(402, 0), None);
        assert_eq!(layer_tints(1, 0), None);
    }

    #[test]
    fn egg_table_is_sorted_for_binary_search() {
        assert!(EGGS.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }

    #[test]
    fn firework_single_colour_is_used_as_is() {
        assert_eq!(
            firework_charge_tint(Some(&charge_nbt(&[0xFF0000]))),
            Some(0xFF0000)
        );
    }

    #[test]
    fn firework_colours_are_averaged() {
        let bytes = charge_nbt(&[0xFF0000, 0x0000FF]);
        assert_eq!(firework_charge_tint(Some(&bytes)), Some(0x7F007F));
    }

    #[test]
    fn firework_without_colours_is_grey() {
        assert_eq!(firework_charge_tint(None), Some(0x8A8A8A));
        assert_eq!(firework_charge_tint(Some(&[10, 0, 0, 0])), Some(0x8A8A8A));
        assert_eq!(firework_charge_tint(Some(&charge_nbt(&[]))), Some(0x8A8A8A));
    }

    #[test]
    fn firework_malformed_nbt_is_none() {
        assert_eq!(firework_charge_tint(Some(&[10, 0])), None);
    }
}

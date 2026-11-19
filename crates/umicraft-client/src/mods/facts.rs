// SPDX-License-Identifier: MIT OR Apache-2.0

//! Turns what the session knows (the inventory, the potion effects) into the
//! plain facts the built-in mods read from their [`ModContext`]
//! (`umicraft_builtin_mods`). Pure functions: the window passes the data in.
//!
//! [`ModContext`]: umicraft_builtin_mods::ModContext

use umicraft_builtin_mods::{ArmorPiece, EffectInfo, MAX_EFFECTS};
use umicraft_game::inventory::HOTBAR_START;
use umicraft_game::items;
use umicraft_game::server::effects::ActiveEffect;
use umicraft_protocol::packets::play::ItemStack;

/// The window slots of the worn armor in the player window, helmet first
/// (MCP `ContainerPlayer`: slots 5 to 8 hold armor piece 3 to 0).
const ARMOR_SLOTS: std::ops::Range<usize> = 5..9;

/// The duration marker of an endless effect, as the mods expect it.
const ENDLESS_TICKS: i32 = 32_767;

/// The armor shown for `stack`: its name and durability, `None` for an empty
/// slot or an id the item table does not know.
fn piece(stack: &ItemStack) -> Option<ArmorPiece> {
    let def = items::lookup(i32::from(stack.item_id))?;
    Some(ArmorPiece {
        name: def.name,
        damage: stack.damage,
        max_damage: def.max_damage,
    })
}

/// The four armor pieces worn (helmet first) and the item in the hand, from
/// the 45 slots of the player window and the selected hotbar slot.
#[must_use]
pub fn equipment(
    slots: &[Option<ItemStack>],
    selected: u8,
) -> ([Option<ArmorPiece>; 4], Option<ArmorPiece>) {
    let worn = |slot: usize| slots.get(slot).and_then(Option::as_ref).and_then(piece);
    let armor = std::array::from_fn(|index| worn(ARMOR_SLOTS.start + index));
    let held = worn(HOTBAR_START + usize::from(selected.min(8)));
    (armor, held)
}

/// The active effects as the mods read them, in id order; effects past
/// [`MAX_EFFECTS`] are left out (the game has 23 kinds, so none is).
#[must_use]
pub fn effects(active: &[ActiveEffect]) -> [Option<EffectInfo>; MAX_EFFECTS] {
    let mut out = [None; MAX_EFFECTS];
    for (slot, effect) in out.iter_mut().zip(active) {
        *slot = Some(EffectInfo {
            id: effect.effect_id,
            amplifier: effect.amplifier,
            ticks: if effect.max_duration {
                ENDLESS_TICKS
            } else {
                effect.duration
            },
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(item_id: i16, damage: i16) -> ItemStack {
        ItemStack {
            item_id,
            stack_size: 1,
            damage,
            nbt: None,
        }
    }

    #[test]
    fn the_armor_is_read_helmet_first_and_the_held_item_from_the_hotbar() {
        let mut slots: Vec<Option<ItemStack>> = vec![None; 45];
        slots[5] = Some(stack(310, 13)); // diamond helmet
        slots[8] = Some(stack(309, 0)); // iron boots
        slots[HOTBAR_START + 2] = Some(stack(276, 61)); // diamond sword
        let (armor, held) = equipment(&slots, 2);
        assert_eq!(
            armor[0],
            Some(ArmorPiece {
                name: "diamond_helmet",
                damage: 13,
                max_damage: 363
            })
        );
        assert_eq!(armor[1], None);
        assert_eq!(armor[3].map(|p| p.name), Some("iron_boots"));
        assert_eq!(
            held,
            Some(ArmorPiece {
                name: "diamond_sword",
                damage: 61,
                max_damage: 1561
            })
        );
    }

    #[test]
    fn an_unknown_item_or_a_short_slot_list_shows_nothing() {
        let slots = vec![Some(stack(9999, 0))];
        assert_eq!(equipment(&slots, 0), ([None; 4], None));
    }

    #[test]
    fn effects_keep_their_ticks_and_mark_the_endless_ones() {
        let active = [
            ActiveEffect {
                effect_id: 1,
                amplifier: 1,
                duration: 1660,
                show_particles: true,
                max_duration: false,
            },
            ActiveEffect {
                effect_id: 12,
                amplifier: 0,
                duration: 5,
                show_particles: true,
                max_duration: true,
            },
        ];
        let out = effects(&active);
        assert_eq!(
            out[0],
            Some(EffectInfo {
                id: 1,
                amplifier: 1,
                ticks: 1660
            })
        );
        assert_eq!(out[1].map(|e| e.ticks), Some(32_767));
        assert_eq!(out[2], None);
    }
}

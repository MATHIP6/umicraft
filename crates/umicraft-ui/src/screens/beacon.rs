// SPDX-License-Identifier: MIT OR Apache-2.0

//! The beacon, after `ContainerBeacon` / `GuiBeacon`.

use super::decoration::Decoration;
use super::interaction::{Dynamic, Hotspot, HotspotSpec, ScreenState};
use super::layout::{grid_slots, STRIDE};
use super::{ScreenSpec, SlotSpec};

/// Left edge of the backpack, wider than the usual 176 pixel window.
const BACKPACK_X: u32 = 36;
/// Top edge of the backpack.
const BACKPACK_Y: u32 = 137;
/// Edge of a button.
const BUTTON: u32 = 22;
/// Row of the texture where the button sprites start.
const BUTTON_V: u32 = 219;
/// The potion id and the status icon index of each power, by tier
/// (`TileEntityBeacon.effectsList`, `Potion.setIconIndex`).
const TIERS: [&[(i32, u32)]; 4] = [
    &[(1, 0), (3, 2)],
    &[(11, 14), (8, 10)],
    &[(5, 4)],
    &[(10, 7)],
];
/// The tier whose second button repeats the primary power as a secondary.
const SECONDARY_TIER: u8 = 3;

/// Builds the beacon screen (230x219): the payment slot, backpack, hotbar.
pub(super) fn spec() -> ScreenSpec {
    let mut slots = vec![SlotSpec {
        index: 0,
        x: 136,
        y: 110,
    }];
    slots.extend(grid_slots(1, 9, 3, BACKPACK_X, BACKPACK_Y));
    slots.extend(grid_slots(
        28,
        9,
        1,
        BACKPACK_X,
        BACKPACK_Y + 3 * STRIDE + 4,
    ));
    ScreenSpec::single("container/beacon", (230, 219), slots)
}

/// The potion id of a power button. The second button of the last tier
/// repeats the primary power (`GuiBeacon.updateScreen`), so it needs
/// `primary`; it does not exist without one.
#[must_use]
pub fn effect_of(tier: u8, slot: u8, primary: i32) -> Option<i32> {
    let row = TIERS.get(usize::from(tier))?;
    match row.get(usize::from(slot)) {
        Some(&(id, _)) => Some(id),
        None if tier == SECONDARY_TIER && usize::from(slot) == row.len() && primary > 0 => {
            Some(primary)
        }
        None => None,
    }
}

/// The status icon index of a potion id.
fn icon_index(effect: i32) -> Option<u32> {
    TIERS
        .iter()
        .flat_map(|row| row.iter())
        .find(|&&(id, _)| id == effect)
        .map(|&(_, icon)| icon)
}

/// Every button of the screen, power buttons included whether or not the
/// repeat button is shown.
pub(super) fn hotspots() -> Vec<HotspotSpec> {
    let mut specs = Vec::new();
    for (tier, row) in TIERS.iter().enumerate() {
        let tier = tier as u8;
        // The last tier has one more button than effects: the repeated primary.
        let count = row.len() as u32 + u32::from(tier == SECONDARY_TIER);
        let width = count * BUTTON + (count - 1) * 2;
        let (centre, y) = if tier == SECONDARY_TIER {
            (167, 47)
        } else {
            (76, 22 + u32::from(tier) * 25)
        };
        for slot in 0..count {
            specs.push(HotspotSpec {
                hotspot: Hotspot::BeaconPower {
                    tier,
                    slot: slot as u8,
                },
                x: centre + slot * 24 - width / 2,
                y,
                width: BUTTON,
                height: BUTTON,
            });
        }
    }
    for (hotspot, x) in [(Hotspot::BeaconConfirm, 164), (Hotspot::BeaconCancel, 190)] {
        specs.push(HotspotSpec {
            hotspot,
            x,
            y: 107,
            width: BUTTON,
            height: BUTTON,
        });
    }
    specs
}

/// The buttons for the window properties `[levels, primary, secondary]` and
/// whether the payment slot holds an item.
pub(super) fn dynamic(state: &ScreenState<'_>) -> Dynamic {
    let property = |id: usize| state.properties.get(id).map(|&v| i32::from(v));
    let Some(levels) = property(0) else {
        // The server has not sent the pyramid height yet: no power buttons.
        return Dynamic::default();
    };
    let (primary, secondary) = (property(1).unwrap_or(0), property(2).unwrap_or(0));
    let mut decorations = Vec::new();
    for spec in hotspots() {
        let (v, icon) = match spec.hotspot {
            Hotspot::BeaconPower { tier, slot } => {
                let Some(effect) = effect_of(tier, slot, primary) else {
                    continue;
                };
                let selected = if tier == SECONDARY_TIER {
                    effect == secondary
                } else {
                    effect == primary
                };
                let state = button_state(
                    i32::from(tier) < levels,
                    selected,
                    state.hover == Some(spec.hotspot),
                );
                let icon = icon_index(effect).map(|i| {
                    Decoration::full(
                        (i % 8 * 18, 198 + i / 8 * 18),
                        (18, 18),
                        (spec.x + 2, spec.y + 2),
                    )
                    .on_inventory_sheet()
                });
                (state, icon)
            }
            Hotspot::BeaconConfirm => (
                button_state(
                    state.filled[0] && primary > 0,
                    false,
                    state.hover == Some(spec.hotspot),
                ),
                Some(Decoration::full(
                    (90, 220),
                    (18, 18),
                    (spec.x + 2, spec.y + 2),
                )),
            ),
            Hotspot::BeaconCancel => (
                button_state(true, false, state.hover == Some(spec.hotspot)),
                Some(Decoration::full(
                    (112, 220),
                    (18, 18),
                    (spec.x + 2, spec.y + 2),
                )),
            ),
            _ => continue,
        };
        decorations.push(Decoration::full(
            (v, BUTTON_V),
            (BUTTON, BUTTON),
            (spec.x, spec.y),
        ));
        decorations.extend(icon);
    }
    Dynamic::sprites(decorations)
}

/// The column of the button sprite: normal, selected, disabled or hovered.
fn button_state(enabled: bool, selected: bool, hovered: bool) -> u32 {
    let column = if !enabled {
        2
    } else if selected {
        1
    } else if hovered {
        3
    } else {
        0
    };
    column * BUTTON
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_container_beacon_coordinates() {
        let slots = spec().slots;
        assert_eq!(slots.len(), 37);
        assert_eq!((slots[0].x, slots[0].y), (136, 110));
        assert_eq!((slots[1].x, slots[1].y), (36, 137));
        assert_eq!((slots[27].x, slots[27].y), (180, 173));
        assert_eq!((slots[28].x, slots[28].y), (36, 195));
        assert_eq!((slots[36].x, slots[36].y), (180, 195));
    }

    #[test]
    fn power_buttons_follow_gui_beacon_positions() {
        let at = |tier, slot| {
            let spec = hotspots()
                .into_iter()
                .find(|s| s.hotspot == Hotspot::BeaconPower { tier, slot })
                .unwrap();
            (spec.x, spec.y)
        };
        assert_eq!(at(0, 0), (53, 22));
        assert_eq!(at(0, 1), (77, 22));
        assert_eq!(at(1, 0), (53, 47));
        assert_eq!(at(2, 0), (65, 72));
        assert_eq!(at(3, 0), (144, 47));
        assert_eq!(at(3, 1), (168, 47));
    }

    #[test]
    fn the_repeat_button_needs_a_primary_power() {
        assert_eq!(effect_of(3, 0, 0), Some(10));
        assert_eq!(effect_of(3, 1, 0), None);
        assert_eq!(effect_of(3, 1, 8), Some(8));
        assert_eq!(effect_of(0, 1, 0), Some(3));
        assert_eq!(effect_of(2, 1, 5), None);
        assert_eq!(effect_of(4, 0, 0), None);
    }

    #[test]
    fn no_buttons_before_the_levels_arrive() {
        assert!(dynamic(&ScreenState::default()).decorations.is_empty());
    }

    #[test]
    fn tiers_above_the_pyramid_are_disabled_and_the_selection_shows() {
        let state = ScreenState {
            properties: &[1, 1, 0],
            filled: [true, false, false],
            ..ScreenState::default()
        };
        let sprites = dynamic(&state).decorations;
        let column = |x: u32, y: u32| {
            sprites
                .iter()
                .find(|d| d.dst == (x, y) && d.size == (22, 22))
                .map(|d| d.src.0 / 22)
        };
        // Speed (tier 0) is the selected primary power, haste is plain.
        assert_eq!(column(53, 22), Some(1));
        assert_eq!(column(77, 22), Some(0));
        // Tier 1 is out of reach for a one-level pyramid.
        assert_eq!(column(53, 47), Some(2));
        // The done button works with a payment and a primary power.
        assert_eq!(column(164, 107), Some(0));
    }

    #[test]
    fn done_is_disabled_without_a_payment() {
        let state = ScreenState {
            properties: &[1, 1, 0],
            ..ScreenState::default()
        };
        let done = dynamic(&state)
            .decorations
            .into_iter()
            .find(|d| d.dst == (164, 107) && d.size == (22, 22))
            .unwrap();
        assert_eq!(done.src.0 / 22, 2);
    }

    #[test]
    fn power_icons_come_from_the_inventory_sheet() {
        let state = ScreenState {
            properties: &[4, 0, 0],
            ..ScreenState::default()
        };
        let decorations = dynamic(&state).decorations;
        let icon = decorations
            .iter()
            .find(|d| d.dst == (55, 24))
            .expect("speed icon");
        assert_eq!(icon.src, (0, 198));
        assert_eq!(icon.sheet, super::super::decoration::Sheet::Inventory);
        // Resistance: icon index 14 -> column 6, row 1.
        let resistance = decorations.iter().find(|d| d.dst == (55, 49)).unwrap();
        assert_eq!(resistance.src, (6 * 18, 198 + 18));
    }
}

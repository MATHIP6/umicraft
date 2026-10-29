// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fish food values, which depend on the damage value of the stack.
//!
//! MCP `ItemFishFood.FishType` stores the values per subtype; the generated
//! table only has the placeholder `0` for the two fish items because
//! `ItemFishFood`'s constructor passes `(0, 0.0F)` to `ItemFood`.

use super::kind::FoodValues;

/// The item id of raw fish (`Items.fish`).
pub(super) const RAW_FISH: u16 = 349;

/// The item id of cooked fish (`Items.cooked_fish`).
pub(super) const COOKED_FISH: u16 = 350;

/// One `FishType`: uncooked and cooked `(hunger, saturation)`; `None` when the
/// type cannot be cooked (`FishType.canCook`).
struct FishType {
    raw: (u8, f32),
    cooked: Option<(u8, f32)>,
}

/// `FishType` by metadata: cod, salmon, clownfish, pufferfish.
const FISH: [FishType; 4] = [
    FishType {
        raw: (2, 0.1),
        cooked: Some((5, 0.6)),
    },
    FishType {
        raw: (2, 0.1),
        cooked: Some((6, 0.8)),
    },
    FishType {
        raw: (1, 0.1),
        cooked: None,
    },
    FishType {
        raw: (1, 0.1),
        cooked: None,
    },
];

/// The food values of a fish item with the given damage value, `None` when
/// `id` is not a fish. An unknown damage value is a cod
/// (`FishType.byMetadata`).
pub(super) fn fish_food(id: u16, meta: i16) -> Option<FoodValues> {
    let cooked_item = match id {
        RAW_FISH => false,
        COOKED_FISH => true,
        _ => return None,
    };
    let fish = usize::try_from(meta)
        .ok()
        .and_then(|meta| FISH.get(meta))
        .unwrap_or(&FISH[0]);
    // A cooked item of a type that cannot be cooked eats like the raw one.
    let (hunger, saturation) = match fish.cooked {
        Some(cooked) if cooked_item => cooked,
        _ => fish.raw,
    };
    Some(FoodValues {
        hunger,
        saturation,
        always_edible: false,
    })
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! When the crosshair is drawn for a spectator (MCP
//! `GuiIngame.showCrosshair`): only on an entity or on a block whose tile
//! entity is an `IInventory`.

/// Block ids whose tile entity implements `IInventory` in 1.8.8: dispenser,
/// furnace, lit furnace, chest, brewing stand, beacon, hopper, trapped chest
/// and dropper (`TileEntityDispenser`, `TileEntityFurnace`, `TileEntityChest`,
/// `TileEntityBrewingStand`, `TileEntityBeacon`, `TileEntityHopper`).
const CONTAINER_BLOCKS: [u16; 9] = [23, 61, 62, 54, 117, 138, 154, 146, 158];

/// Whether a spectator sees the crosshair: when an entity is pointed at, or
/// the pointed block (`block_id`) holds an inventory. The ender chest does
/// not: its tile entity is not an `IInventory`.
///
/// ```
/// use umicraft_game::hud::crosshair::spectator_crosshair;
/// assert!(spectator_crosshair(false, Some(54)));
/// assert!(!spectator_crosshair(false, Some(1)));
/// ```
#[must_use]
pub fn spectator_crosshair(entity_pointed: bool, block_id: Option<u16>) -> bool {
    entity_pointed || block_id.is_some_and(|id| CONTAINER_BLOCKS.contains(&id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entity_or_a_container_shows_the_crosshair() {
        assert!(spectator_crosshair(true, None));
        for id in CONTAINER_BLOCKS {
            assert!(spectator_crosshair(false, Some(id)), "block {id}");
        }
    }

    #[test]
    fn plain_blocks_ender_chests_and_the_sky_hide_it() {
        assert!(!spectator_crosshair(false, Some(1)));
        assert!(!spectator_crosshair(false, Some(130)));
        assert!(!spectator_crosshair(false, None));
    }
}

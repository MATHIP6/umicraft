// SPDX-License-Identifier: MIT OR Apache-2.0

//! OptiFine "Better Snow": a plant, fence or similar thin block that stands
//! next to a snow layer gets a snow layer drawn in its own cell, so the snow
//! does not stop at a hole around it (`optifine.BetterSnow`, ADR-0088/0089).

use umicraft_world::BlockState;

use crate::better_grass::SNOW_LAYER;
use crate::BlockSource;

/// The state of the layer that is drawn: `Blocks.snow_layer`'s default.
pub(crate) const LAYER_STATE: BlockState = BlockState::new(SNOW_LAYER, 0);

/// Whether `state` at `position` gets a snow layer: it is a block
/// `BetterSnow.checkBlock` accepts, a snow layer lies north, south, west or
/// east of it, and the block below is a full opaque cube.
pub(crate) fn needs_layer<S: BlockSource + ?Sized>(
    source: &S,
    [x, y, z]: [i32; 3],
    state: BlockState,
) -> bool {
    accepts(state)
        && [[0, -1], [0, 1], [-1, 0], [1, 0]]
            .iter()
            .any(|[dx, dz]| source.block(x + dx, y, z + dz).id() == SNOW_LAYER)
        && source.block(x, y - 1, z).is_opaque()
}

/// `BetterSnow.checkBlock`: saplings, flowers, mushrooms, tall grass and
/// double plants, fences, fence gates, flower pots, panes, sugar cane, walls,
/// an upright redstone torch and a floor lever.
fn accepts(state: BlockState) -> bool {
    let meta = state.metadata();
    match state.id() {
        6 | 31 | 37..=40 | 175 => true,
        83 | 85 | 101 | 102 | 107 | 113 | 139 | 140 | 160 | 183..=192 => true,
        // `BlockTorch.FACING == UP` is metadata 5 on a redstone torch.
        75 | 76 => meta == 5,
        // The lever orientations UP_X and UP_Z are metadata 5 and 6.
        69 => matches!(meta & 7, 5 | 6),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Blocks(Vec<([i32; 3], BlockState)>);

    impl BlockSource for Blocks {
        fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
            self.0
                .iter()
                .find(|(at, _)| *at == [x, y, z])
                .map_or(BlockState::AIR, |(_, state)| *state)
        }
    }

    fn scene(plant: BlockState, snow: bool) -> Blocks {
        let mut blocks = vec![([0, 0, 0], plant), ([0, -1, 0], BlockState::new(1, 0))];
        if snow {
            blocks.push(([1, 0, 0], LAYER_STATE));
        }
        Blocks(blocks)
    }

    #[test]
    fn a_plant_beside_a_snow_layer_on_solid_ground_gets_a_layer() {
        let tall_grass = BlockState::new(31, 1);
        assert!(needs_layer(&scene(tall_grass, true), [0, 0, 0], tall_grass));
    }

    #[test]
    fn without_snow_neighbour_or_solid_floor_it_does_not() {
        let tall_grass = BlockState::new(31, 1);
        assert!(!needs_layer(
            &scene(tall_grass, false),
            [0, 0, 0],
            tall_grass
        ));
        let mut floating = scene(tall_grass, true);
        floating.0.retain(|(at, _)| at[1] != -1);
        assert!(!needs_layer(&floating, [0, 0, 0], tall_grass));
    }

    #[test]
    fn full_blocks_and_wall_torches_are_not_accepted() {
        let stone = BlockState::new(1, 0);
        assert!(!needs_layer(&scene(stone, true), [0, 0, 0], stone));
        let wall_torch = BlockState::new(76, 1);
        assert!(!needs_layer(
            &scene(wall_torch, true),
            [0, 0, 0],
            wall_torch
        ));
        let upright = BlockState::new(76, 5);
        assert!(needs_layer(&scene(upright, true), [0, 0, 0], upright));
    }
}

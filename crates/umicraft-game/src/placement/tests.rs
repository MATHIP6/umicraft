// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use super::*;
use crate::raycast::Face;

/// A block source backed by a hash map.
#[derive(Default)]
struct Blocks(HashMap<(i32, i32, i32), BlockState>);

impl Blocks {
    fn with(blocks: &[(i32, i32, i32, u16, u8)]) -> Self {
        Self(
            blocks
                .iter()
                .map(|&(x, y, z, id, meta)| ((x, y, z), BlockState::new(id, meta)))
                .collect(),
        )
    }
}

impl BlockSource for Blocks {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        self.0.get(&(x, y, z)).copied().unwrap_or(BlockState::AIR)
    }
}

/// A floor of stone at y = 0 around the origin.
fn floor() -> Vec<(i32, i32, i32, u16, u8)> {
    (-3..=3)
        .flat_map(|x| (-3..=3).map(move |z| (x, 0, z, 1, 0)))
        .collect()
}

fn floor_with(extra: &[(i32, i32, i32, u16, u8)]) -> Blocks {
    let mut blocks = floor();
    blocks.extend_from_slice(extra);
    Blocks::with(&blocks)
}

/// A click on `face` of `block` at the face point `(u, v)`-ish `cursor`.
fn hit(block: (i32, i32, i32), face: Face, cursor: [f64; 3]) -> RayHit {
    RayHit {
        block: BlockPos::new(block.0, block.1, block.2),
        face,
        position: DVec3::new(
            f64::from(block.0) + cursor[0],
            f64::from(block.1) + cursor[1],
            f64::from(block.2) + cursor[2],
        ),
        distance: 3.0,
    }
}

/// The top face of the floor block at the origin.
fn top_of_origin() -> RayHit {
    hit((0, 0, 0), Face::Up, [0.5, 1.0, 0.5])
}

fn stack(id: i16, damage: i16) -> ItemStack {
    ItemStack {
        item_id: id,
        stack_size: 5,
        damage,
        nbt: None,
    }
}

/// A player standing two blocks south of the origin, looking with `yaw`.
fn player(yaw: f32) -> Placer<'static> {
    let mut placer = Placer::alone(Aabb::new(
        DVec3::new(0.2, 1.0, 2.2),
        DVec3::new(0.8, 2.8, 2.8),
    ));
    placer.yaw = yaw;
    placer
}

fn placed(
    world: &Blocks,
    hit: &RayHit,
    item: &ItemStack,
    placer: &Placer<'_>,
) -> Vec<(BlockPos, BlockState)> {
    predict(&PlaceClick {
        world,
        hit,
        item,
        placer: Some(placer),
    })
    .map(|placement| placement.changes().collect())
    .unwrap_or_default()
}

fn at(x: i32, y: i32, z: i32, id: u16, meta: u8) -> (BlockPos, BlockState) {
    (BlockPos::new(x, y, z), BlockState::new(id, meta))
}

#[test]
fn a_block_goes_next_to_the_clicked_face() {
    let world = floor_with(&[]);
    let cobble = stack(4, 0);
    assert_eq!(
        placed(&world, &top_of_origin(), &cobble, &player(0.0)),
        vec![at(0, 1, 0, 4, 0)]
    );
}

#[test]
fn tall_grass_and_a_single_snow_layer_are_replaced_in_place() {
    let cobble = stack(4, 0);
    for (id, meta) in [(31, 1), (78, 0), (106, 0)] {
        let world = floor_with(&[(0, 1, 0, id, meta)]);
        let click = hit((0, 1, 0), Face::North, [0.5, 0.5, 0.0]);
        assert_eq!(
            placed(&world, &click, &cobble, &player(0.0)),
            vec![at(0, 1, 0, 4, 0)],
            "block {id}"
        );
    }
    // Two snow layers are not replaceable: the block goes beside them.
    let world = floor_with(&[(0, 1, 0, 78, 1)]);
    let click = hit((0, 1, 0), Face::North, [0.5, 0.25, 0.0]);
    assert_eq!(
        placed(&world, &click, &cobble, &player(0.0)),
        vec![at(0, 1, -1, 4, 0)]
    );
}

/// Regression test: the client never placed a block itself, so at a high
/// ping the block appeared a round trip after the click, and the next
/// block could not be placed against it until then.
#[test]
fn a_block_inside_an_entity_is_neither_sent_nor_placed_but_a_torch_is() {
    let world = floor_with(&[]);
    let inside = Placer::alone(Aabb::new(
        DVec3::new(0.2, 1.0, 0.2),
        DVec3::new(0.8, 2.8, 0.8),
    ));
    let cobble = stack(4, 0);
    let click = PlaceClick {
        world: &world,
        hit: &top_of_origin(),
        item: &cobble,
        placer: Some(&inside),
    };
    assert!(!can_place_on_side(&click));
    assert_eq!(predict(&click), None);
    // Another entity in the cell refuses it too.
    let mob = [Aabb::new(
        DVec3::new(0.1, 1.0, 0.1),
        DVec3::new(0.9, 2.9, 0.9),
    )];
    let watched = Placer {
        others: &mob,
        ..player(0.0)
    };
    assert!(!can_place_on_side(&PlaceClick {
        placer: Some(&watched),
        ..click
    }));
    // A torch has no collision box.
    let torch = stack(50, 0);
    let click = PlaceClick {
        item: &torch,
        ..click
    };
    assert!(can_place_on_side(&click));
    assert_eq!(
        predict(&click).map(|p| p.changes().collect::<Vec<_>>()),
        Some(vec![at(0, 1, 0, 50, 5)])
    );
}

#[test]
fn stairs_face_the_player_and_turn_over_on_a_high_click() {
    let world = floor_with(&[(0, 1, -1, 1, 0)]);
    let oak = stack(53, 0);
    // Yaw 180 looks north: the stair faces north (meta 3).
    assert_eq!(
        placed(&world, &top_of_origin(), &oak, &player(180.0)),
        vec![at(0, 1, 0, 53, 3)]
    );
    // Yaw -90 looks east (meta 0); a click on the upper half of a wall turns
    // the stair upside down.
    let high = hit((0, 1, -1), Face::South, [0.5, 0.75, 1.0]);
    assert_eq!(
        placed(&world, &high, &oak, &player(-90.0)),
        vec![at(0, 1, 0, 53, 4)]
    );
    // A click under a block is always a top stair (yaw 90 looks west: 1).
    let ceiling = floor_with(&[(0, 3, 0, 1, 0)]);
    let under = hit((0, 3, 0), Face::Down, [0.5, 0.0, 0.5]);
    assert_eq!(
        placed(&ceiling, &under, &oak, &player(90.0)),
        vec![at(0, 2, 0, 53, 1 | 4)]
    );
}

#[test]
fn slabs_take_the_half_of_the_click_and_merge_into_double_slabs() {
    let world = floor_with(&[(0, 1, -1, 1, 0)]);
    let birch = stack(126, 2);
    assert_eq!(
        placed(&world, &top_of_origin(), &birch, &player(0.0)),
        vec![at(0, 1, 0, 126, 2)]
    );
    let high = hit((0, 1, -1), Face::South, [0.5, 0.6, 1.0]);
    assert_eq!(
        placed(&world, &high, &birch, &player(0.0)),
        vec![at(0, 1, 0, 126, 2 | 8)]
    );
    // The top of a bottom slab of the same wood becomes a double slab.
    let slab = floor_with(&[(0, 1, 0, 126, 2)]);
    let on_slab = hit((0, 1, 0), Face::Up, [0.5, 0.5, 0.5]);
    assert_eq!(
        placed(&slab, &on_slab, &birch, &player(0.0)),
        vec![at(0, 1, 0, 125, 2)]
    );
    // Another wood is placed above instead.
    let oak = stack(126, 0);
    assert_eq!(
        placed(&slab, &on_slab, &oak, &player(0.0)),
        vec![at(0, 2, 0, 126, 0)]
    );
    // A slab beside the clicked face completes too.
    let beside = hit((0, 1, -1), Face::South, [0.5, 0.25, 1.0]);
    let world = floor_with(&[(0, 1, -1, 1, 0), (0, 1, 0, 126, 2 | 8)]);
    assert_eq!(
        placed(&world, &beside, &birch, &player(0.0)),
        vec![at(0, 1, 0, 125, 2)]
    );
}

#[test]
fn logs_hay_and_quartz_pillars_follow_the_clicked_axis() {
    let world = floor_with(&[(0, 1, -1, 1, 0)]);
    let side = hit((0, 1, -1), Face::South, [0.5, 0.5, 1.0]);
    assert_eq!(
        placed(&world, &side, &stack(17, 2), &player(0.0)),
        vec![at(0, 1, 0, 17, 2 | 8)]
    );
    assert_eq!(
        placed(&world, &top_of_origin(), &stack(162, 1), &player(0.0)),
        vec![at(0, 1, 0, 162, 1)]
    );
    assert_eq!(
        placed(&world, &side, &stack(170, 0), &player(0.0)),
        vec![at(0, 1, 0, 170, 8)]
    );
    assert_eq!(
        placed(&world, &side, &stack(155, 2), &player(0.0)),
        vec![at(0, 1, 0, 155, 4)]
    );
    assert_eq!(
        placed(&world, &side, &stack(155, 1), &player(0.0)),
        vec![at(0, 1, 0, 155, 1)]
    );
}

#[test]
fn subtypes_and_leaves_keep_their_variant() {
    let world = floor_with(&[]);
    let click = top_of_origin();
    assert_eq!(
        placed(&world, &click, &stack(35, 14), &player(0.0)),
        vec![at(0, 1, 0, 35, 14)]
    );
    // A damage past the variants falls back to the first one.
    assert_eq!(
        placed(&world, &click, &stack(3, 9), &player(0.0)),
        vec![at(0, 1, 0, 3, 0)]
    );
    // Placed leaves never decay (`ItemLeaves`: `damage | 4`).
    assert_eq!(
        placed(&world, &click, &stack(18, 1), &player(0.0)),
        vec![at(0, 1, 0, 18, 5)]
    );
    // An item block whose damage is not its metadata places metadata 0.
    assert_eq!(
        placed(&world, &click, &stack(4, 3), &player(0.0)),
        vec![at(0, 1, 0, 4, 0)]
    );
}

#[test]
fn furnaces_pumpkins_and_gates_turn_to_the_player() {
    let world = floor_with(&[]);
    let click = top_of_origin();
    // Looking north (yaw 180): the front faces south.
    assert_eq!(
        placed(&world, &click, &stack(61, 0), &player(180.0)),
        vec![at(0, 1, 0, 61, 3)]
    );
    assert_eq!(
        placed(&world, &click, &stack(130, 0), &player(180.0)),
        vec![at(0, 1, 0, 130, 3)]
    );
    assert_eq!(
        placed(&world, &click, &stack(86, 0), &player(180.0)),
        vec![at(0, 1, 0, 86, 0)]
    );
    assert_eq!(
        placed(&world, &click, &stack(120, 0), &player(180.0)),
        vec![at(0, 1, 0, 120, 0)]
    );
    // A gate lies along the view: north is horizontal index 2.
    assert_eq!(
        placed(&world, &click, &stack(107, 0), &player(180.0)),
        vec![at(0, 1, 0, 107, 2)]
    );
    // An anvil lies across it: north turned clockwise is east (3).
    assert_eq!(
        placed(&world, &click, &stack(145, 1), &player(180.0)),
        vec![at(0, 1, 0, 145, 3 | 4)]
    );
}

#[test]
fn pistons_and_dispensers_look_up_or_down_from_close_by() {
    let world = floor_with(&[]);
    let click = top_of_origin();
    let near = Placer::alone(Aabb::new(
        DVec3::new(0.7, 1.0, 1.2),
        DVec3::new(1.3, 2.8, 1.8),
    ));
    // The eyes (2.62) are not two blocks above the cell (1): horizontal,
    // towards the player who looks south.
    assert_eq!(
        placed(&world, &click, &stack(33, 0), &near),
        vec![at(0, 1, 0, 33, 2)]
    );
    // From a block higher the piston faces up.
    let high = Placer::alone(near.bounds.offset(DVec3::new(0.0, 1.0, 0.0)));
    assert_eq!(
        placed(&world, &click, &stack(23, 0), &high),
        vec![at(0, 1, 0, 23, 1)]
    );
    // Under a ceiling, from below, it faces down.
    let ceiling = floor_with(&[(0, 4, 0, 1, 0)]);
    let under = hit((0, 4, 0), Face::Down, [0.5, 0.0, 0.5]);
    assert_eq!(
        placed(&ceiling, &under, &stack(29, 0), &near),
        vec![at(0, 3, 0, 29, 0)]
    );
}

#[test]
fn a_hopper_points_into_the_clicked_block_but_never_up() {
    let world = floor_with(&[(0, 1, -1, 1, 0)]);
    let side = hit((0, 1, -1), Face::South, [0.5, 0.5, 1.0]);
    assert_eq!(
        placed(&world, &side, &stack(154, 0), &player(0.0)),
        vec![at(0, 1, 0, 154, 2)]
    );
    assert_eq!(
        placed(&world, &top_of_origin(), &stack(154, 0), &player(0.0)),
        vec![at(0, 1, 0, 154, 0)]
    );
    let ceiling = floor_with(&[(0, 4, 0, 1, 0)]);
    let under = hit((0, 4, 0), Face::Down, [0.5, 0.0, 0.5]);
    assert_eq!(
        placed(&ceiling, &under, &stack(154, 0), &player(0.0)),
        vec![at(0, 3, 0, 154, 0)]
    );
}

#[test]
fn torches_and_ladders_hang_on_walls_and_need_one() {
    let world = floor_with(&[(0, 1, -1, 1, 0)]);
    let wall = hit((0, 1, -1), Face::South, [0.5, 0.5, 1.0]);
    assert_eq!(
        placed(&world, &wall, &stack(50, 0), &player(0.0)),
        vec![at(0, 1, 0, 50, 3)]
    );
    assert_eq!(
        placed(&world, &wall, &stack(65, 0), &player(0.0)),
        vec![at(0, 1, 0, 65, 3)]
    );
    // On the floor, the ladder finds the wall to the north.
    assert_eq!(
        placed(&world, &top_of_origin(), &stack(65, 0), &player(0.0)),
        vec![at(0, 1, 0, 65, 3)]
    );
    // A torch stands on glass but not on leaves, which hold nothing.
    let glass = floor_with(&[(0, 1, 0, 20, 0)]);
    let on_glass = hit((0, 1, 0), Face::Up, [0.5, 1.0, 0.5]);
    assert_eq!(
        placed(&glass, &on_glass, &stack(76, 0), &player(0.0)),
        vec![at(0, 2, 0, 76, 5)]
    );
    let leaves = floor_with(&[(0, 1, 0, 18, 0)]);
    let click = PlaceClick {
        world: &leaves,
        hit: &on_glass,
        item: &stack(50, 0),
        placer: None,
    };
    assert!(!can_place_on_side(&click));
    // A ladder in the open has nothing to hang on.
    let open = Blocks::with(&[(0, 0, 0, 1, 0)]);
    let click = PlaceClick {
        world: &open,
        hit: &top_of_origin(),
        item: &stack(65, 0),
        placer: None,
    };
    assert!(!can_place_on_side(&click));
}

#[test]
fn pumpkins_gates_carpets_and_trapdoors_check_their_support() {
    // A pumpkin needs a solid top: glass has none, a top slab has one.
    let glass = floor_with(&[(0, 1, 0, 20, 0)]);
    let on_glass = hit((0, 1, 0), Face::Up, [0.5, 1.0, 0.5]);
    let pumpkin = stack(86, 0);
    let click = |world| PlaceClick {
        world,
        hit: &on_glass,
        item: &pumpkin,
        placer: None,
    };
    assert!(!can_place_on_side(&click(&glass)));
    assert!(can_place_on_side(&click(&floor_with(&[(0, 1, 0, 44, 8)]))));
    // A trapdoor goes on the side of a block, never on top.
    let world = floor_with(&[(0, 1, -1, 1, 0)]);
    let trapdoor = stack(96, 0);
    let top = PlaceClick {
        world: &world,
        hit: &top_of_origin(),
        item: &trapdoor,
        placer: None,
    };
    assert!(!can_place_on_side(&top));
    let side = hit((0, 1, -1), Face::South, [0.5, 0.75, 1.0]);
    assert_eq!(
        placed(&world, &side, &trapdoor, &player(0.0)),
        vec![at(0, 1, 0, 96, 1 | 8)]
    );
}

#[test]
fn a_chest_beside_another_turns_both_along_the_pair() {
    // A chest at x = -1; the player looks north (yaw 180), so the new chest
    // faces south (3) and the pair runs along X: the old one turns too.
    let world = floor_with(&[(-1, 1, 0, 54, 2)]);
    assert_eq!(
        placed(&world, &top_of_origin(), &stack(54, 0), &player(180.0)),
        vec![at(-1, 1, 0, 54, 3), at(0, 1, 0, 54, 3)]
    );
    // Alone, it just faces the player.
    let world = floor_with(&[]);
    assert_eq!(
        placed(&world, &top_of_origin(), &stack(54, 0), &player(180.0)),
        vec![at(0, 1, 0, 54, 3)]
    );
    // Beside a double chest it does not fit.
    let world = floor_with(&[(-1, 1, 0, 54, 2), (-2, 1, 0, 54, 2)]);
    let click = PlaceClick {
        world: &world,
        hit: &top_of_origin(),
        item: &stack(54, 0),
        placer: None,
    };
    assert!(!can_place_on_side(&click));
}

#[test]
fn unmodelled_items_are_sent_but_not_predicted() {
    let world = floor_with(&[]);
    let poppy = stack(38, 0);
    let click = PlaceClick {
        world: &world,
        hit: &top_of_origin(),
        item: &poppy,
        placer: None,
    };
    assert!(can_place_on_side(&click));
    assert_eq!(predict(&click), None);
    // A plain item is no block item at all.
    let stick = stack(280, 0);
    assert_eq!(rule_of(stick.item_id), None);
}

#[test]
fn nothing_is_placed_from_an_empty_stack_or_above_the_world() {
    let world = floor_with(&[]);
    let mut empty = stack(4, 0);
    empty.stack_size = 0;
    assert!(placed(&world, &top_of_origin(), &empty, &player(0.0)).is_empty());
    let roof = Blocks::with(&[(0, 254, 0, 1, 0)]);
    let top = hit((0, 254, 0), Face::Up, [0.5, 1.0, 0.5]);
    let high = Placer::alone(Aabb::new(
        DVec3::new(5.0, 0.0, 5.0),
        DVec3::new(5.6, 1.8, 5.6),
    ));
    assert!(placed(&roof, &top, &stack(4, 0), &high).is_empty());
    // A torch is not solid: it may go in the top layer.
    assert_eq!(
        placed(&roof, &top, &stack(50, 0), &high),
        vec![at(0, 255, 0, 50, 5)]
    );
}

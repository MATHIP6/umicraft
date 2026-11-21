// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the connected-texture parser and the tile choice. The worlds are
//! generated in code.

use super::*;
use std::collections::HashSet;

/// A fake world: the offsets that connect, and the offsets of the same block.
struct World {
    position: [i32; 3],
    connected: HashSet<[i32; 3]>,
    same: HashSet<[i32; 3]>,
}

impl World {
    fn new(connected: &[[i32; 3]]) -> Self {
        Self {
            position: [0, 64, 0],
            connected: connected.iter().copied().collect(),
            same: HashSet::new(),
        }
    }
}

impl CtmNeighbours for World {
    fn position(&self) -> [i32; 3] {
        self.position
    }
    fn connects(&self, offset: [i32; 3]) -> bool {
        self.connected.contains(&offset)
    }
    fn same_block(&self, offset: [i32; 3]) -> bool {
        self.same.contains(&offset)
    }
}

const W: [i32; 3] = [-1, 0, 0];
const E: [i32; 3] = [1, 0, 0];
const D: [i32; 3] = [0, -1, 0];
const U: [i32; 3] = [0, 1, 0];
const N: [i32; 3] = [0, 0, -1];
const S: [i32; 3] = [0, 0, 1];

fn ids(name: &str) -> Option<u16> {
    match name {
        "minecraft:glass" => Some(20),
        "minecraft:stone" => Some(1),
        "minecraft:log" => Some(17),
        _ => None,
    }
}

fn one(text: &str) -> Result<CtmRule, CtmError> {
    parse::parse_rule("ctm/glass/glass.properties", text.as_bytes(), &ids)
}

fn rule(method: Method, tiles: usize) -> CtmRule {
    CtmRule {
        name: "t".into(),
        match_blocks: None,
        metadata: None,
        match_tiles: None,
        method,
        tiles: (0..tiles).map(|n| format!("blocks/ctm/t/{n}")).collect(),
        connect: Connect::Block,
        faces: FaceMask::ALL,
        biomes: None,
        heights: None,
        linked: false,
        weights: None,
        symmetry: Symmetry::None,
        metadata_max: -1,
    }
}

// ---- parsing ----

#[test]
fn a_glass_file_parses_with_every_key() {
    let rule = one(
        "matchBlocks=glass stone:1-2\nmatchTiles=glass blocks/stone.png\n\
         method=ctm\ntiles=0-46\nconnect=tile\nfaces=sides top\nbiomes=Plains\n\
         heights=60-70\nmetadata=0-3\nlinked=true\nsymmetry=all\n",
    )
    .expect("valid");
    assert_eq!(rule.name, "glass");
    let blocks = rule.match_blocks.as_ref().expect("blocks");
    assert_eq!(blocks[0].id, 20);
    assert_eq!(blocks[1].id, 1);
    assert!(blocks[1].metadata.as_ref().expect("meta").contains(2));
    assert_eq!(
        rule.match_tiles.as_deref().expect("tiles"),
        ["blocks/glass", "blocks/stone"]
    );
    assert_eq!(rule.method, Method::Ctm);
    assert_eq!(rule.tiles.len(), 47);
    assert_eq!(rule.tiles[0], "blocks/ctm/glass/0");
    assert_eq!(rule.tiles[46], "blocks/ctm/glass/46");
    assert_eq!(rule.connect, Connect::Tile);
    assert!(rule.faces.allows(Side::North, Axis::Y));
    assert!(rule.faces.allows(Side::Up, Axis::Y));
    assert!(!rule.faces.allows(Side::Down, Axis::Y));
    assert!(rule.biomes.is_some());
    assert!(rule.matches_height(65) && !rule.matches_height(71));
    assert!(rule.linked);
    assert_eq!(rule.symmetry, Symmetry::All);
    assert_eq!(rule.metadata_max, 3);
}

#[test]
fn defaults_are_the_ctm_method_and_block_connect() {
    let rule = one("matchBlocks=glass\ntiles=0-46\n").expect("valid");
    assert_eq!(rule.method, Method::Ctm);
    assert_eq!(rule.connect, Connect::Block);
    assert_eq!(rule.faces, FaceMask::ALL);
    let by_tile = one("matchTiles=glass\ntiles=0-46\n").expect("valid");
    assert_eq!(by_tile.connect, Connect::Tile);
}

#[test]
fn block_id_files_name_their_block() {
    let rule = parse::parse_rule("ctm/block20.properties", b"tiles=0-46", &ids).expect("valid");
    assert_eq!(rule.match_blocks.expect("blocks")[0].id, 20);
    assert_eq!(rule.connect, Connect::Block);
}

#[test]
fn a_file_with_neither_match_leaves_the_name_for_the_client() {
    let rule = parse::parse_rule("ctm/glass_red.properties", b"tiles=0-46", &ids).expect("valid");
    assert!(rule.match_blocks.is_none() && rule.match_tiles.is_none());
    assert_eq!(rule.name, "glass_red");
}

#[test]
fn numeric_block_ids_and_metadata_parse() {
    let rule = one("matchBlocks=20 35:0-3\ntiles=0-46").expect("valid");
    let blocks = rule.match_blocks.expect("blocks");
    assert_eq!((blocks[0].id, blocks[1].id), (20, 35));
    assert!(rule.metadata.is_none());
    assert!(blocks[1].metadata.as_ref().expect("meta").contains(3));
    assert!(blocks[0].metadata.is_none());
}

#[test]
fn tile_names_cover_ranges_names_and_paths() {
    let rule = one("method=fixed\ntiles=stone.png\n").expect("valid");
    assert_eq!(rule.tiles, ["blocks/ctm/glass/stone"]);
    let rule = one("method=fixed\ntiles=textures/blocks/stone\n").expect("valid");
    assert_eq!(rule.tiles, ["blocks/stone"]);
    let rule = one("method=fixed\ntiles=mcpatcher/ctm/other/3\n").expect("valid");
    assert_eq!(rule.tiles, ["blocks/ctm/other/3"]);
    let rule = one("method=random\ntiles=1-3,7 5-4\n").expect("valid");
    assert_eq!(rule.tiles.len(), 4, "the reversed interval is skipped");
    assert!(matches!(
        one("method=fixed\ntiles=../x\n"),
        Err(CtmError::InvalidTile(_))
    ));
    assert!(matches!(
        one("method=fixed\ntiles=textures/entity/x\n"),
        Err(CtmError::InvalidTile(_))
    ));
}

#[test]
fn tile_counts_are_checked_per_method() {
    let cases = [
        ("ctm", 46, false),
        ("ctm", 47, true),
        ("glass", 47, true),
        ("horizontal", 4, true),
        ("bookshelf", 3, false),
        ("vertical", 4, true),
        ("vertical", 5, false),
        ("h+v", 7, true),
        ("horizontal+vertical", 6, false),
        ("v+h", 7, true),
        ("vertical+horizontal", 8, false),
        ("top", 1, true),
        ("top", 2, false),
        ("fixed", 1, true),
        ("fixed", 2, false),
        ("random", 1, true),
        ("random", 9, true),
    ];
    for (method, count, ok) in cases {
        let text = format!(
            "matchBlocks=glass\nmethod={method}\ntiles=0-{}\n",
            count - 1
        );
        assert_eq!(one(&text).is_ok(), ok, "{method} with {count} tiles");
    }
}

#[test]
fn repeat_needs_a_size_that_matches_the_tiles() {
    let ok = one("matchBlocks=glass\nmethod=repeat\nwidth=2\nheight=3\ntiles=0-5\n").expect("ok");
    assert_eq!(
        ok.method,
        Method::Repeat {
            width: 2,
            height: 3
        }
    );
    for text in [
        "method=repeat\nwidth=0\nheight=1\ntiles=0\n",
        "method=repeat\nwidth=17\nheight=1\ntiles=0-16\n",
        "method=repeat\nwidth=2\nheight=0\ntiles=0-1\n",
    ] {
        let text = format!("matchBlocks=glass\n{text}");
        assert!(
            matches!(one(&text), Err(CtmError::InvalidRepeatSize { .. })),
            "{text}"
        );
    }
    assert!(matches!(
        one("matchBlocks=glass\nmethod=repeat\nwidth=2\nheight=2\ntiles=0-4\n"),
        Err(CtmError::TileCount { .. })
    ));
}

#[test]
fn bad_files_give_typed_errors() {
    assert!(matches!(
        one("matchBlocks=glass\ntiles=0-46\nmethod=wavy"),
        Err(CtmError::UnknownMethod(_))
    ));
    assert_eq!(one("matchBlocks=glass\nmethod=ctm"), Err(CtmError::NoTiles));
    assert!(matches!(
        one("matchBlocks=glass\ntiles=0-46\nconnect=x"),
        Err(CtmError::UnknownConnect(_))
    ));
    assert!(matches!(
        one("matchBlocks=glass\ntiles=0-46\nfaces=left"),
        Err(CtmError::UnknownFace(_))
    ));
    assert_eq!(
        one("matchBlocks=glass\ntiles=0-46\nrenderPass=1"),
        Err(CtmError::RenderPass(1))
    );
    assert!(matches!(
        one("matchBlocks=nope\ntiles=0-46"),
        Err(CtmError::BadBlockMatch(_))
    ));
    assert!(matches!(
        one("matchBlocks=glass:color=red\ntiles=0-46"),
        Err(CtmError::BadBlockMatch(_))
    ));
    assert!(matches!(
        one("matchBlocks=glass\ntiles=0-46\nmetadata=x"),
        Err(CtmError::InvalidList { .. })
    ));
    assert!(matches!(one("a=\\u12"), Err(CtmError::Properties(_))));
}

#[test]
fn a_bad_file_costs_only_its_rule() {
    let files: [(&str, &[u8]); 3] = [
        ("ctm/a.properties", b"matchBlocks=glass\ntiles=0-46"),
        ("ctm/b.properties", b"method=wavy"),
        ("ctm/readme.txt", b"hello"),
    ];
    let rules = parse_rules(files, &ids);
    assert_eq!(rules.rules.len(), 1);
    assert_eq!(rules.rules[0].name, "a");
}

#[test]
fn min_and_max_height_make_a_range() {
    let rule = one("matchBlocks=glass\ntiles=0-46\nminHeight=10\nmaxHeight=20").expect("ok");
    assert!(rule.matches_height(10) && rule.matches_height(20) && !rule.matches_height(21));
    let open = one("matchBlocks=glass\ntiles=0-46\nminHeight=10").expect("ok");
    assert!(open.matches_height(500));
}

#[test]
fn weights_are_trimmed_and_padded_with_the_average() {
    let rule = one("matchBlocks=glass\nmethod=random\ntiles=0-3\nweights=2 4").expect("ok");
    let weights = rule.weights.expect("weights");
    assert_eq!(weights.cumulative, [2, 6, 9, 12]);
    assert_eq!(weights.total, 12);
    let trimmed = one("matchBlocks=glass\nmethod=random\ntiles=0-1\nweights=1 1 1").expect("ok");
    assert_eq!(trimmed.weights.expect("weights").cumulative, [1, 2]);
    let zero = one("matchBlocks=glass\nmethod=random\ntiles=0-1\nweights=0 0").expect("ok");
    assert_eq!(zero.weights.expect("weights").total, 1);
}

#[test]
fn match_filters_combine_block_and_metadata() {
    let rule = one("matchBlocks=glass stone:1\nmetadata=0-1\ntiles=0-46").expect("ok");
    assert!(rule.matches_block(20, 0));
    assert!(
        !rule.matches_block(20, 2),
        "the global metadata list applies"
    );
    assert!(rule.matches_block(1, 1));
    assert!(!rule.matches_block(1, 0));
    assert!(!rule.matches_block(5, 0));
}

// ---- face mask and pillars ----

#[test]
fn the_face_mask_follows_the_pillar_axis() {
    let mask = parse::parse_faces("top").expect("mask");
    assert!(mask.allows(Side::Up, Axis::Y));
    assert!(
        mask.allows(Side::North, Axis::Z),
        "the north end of a Z log is its top"
    );
    assert!(!mask.allows(Side::South, Axis::Z));
    assert!(mask.allows(Side::West, Axis::X));
    assert!(!mask.allows(Side::East, Axis::X));
    assert!(!mask.allows(Side::Up, Axis::X));
}

#[test]
fn pillar_axes_and_metadata() {
    assert_eq!(pillar_axis_and_metadata(17, 4, 3), (Axis::X, 0));
    assert_eq!(pillar_axis_and_metadata(17, 9, 3), (Axis::Z, 1));
    assert_eq!(pillar_axis_and_metadata(17, 9, 15), (Axis::Z, 9));
    assert_eq!(pillar_axis_and_metadata(155, 4, 2), (Axis::Z, 2));
    assert_eq!(pillar_axis_and_metadata(155, 3, 4), (Axis::X, 3));
    assert_eq!(pillar_axis_and_metadata(20, 7, -1), (Axis::Y, 7));
}

// ---- ctm 47 ----

/// The edge offsets of `side`, as left, right, up, down.
fn edges(side: Side) -> [[i32; 3]; 4] {
    match side {
        Side::Down => [W, E, N, S],
        Side::Up => [W, E, S, N],
        Side::North => [E, W, D, U],
        Side::South => [W, E, D, U],
        Side::West => [N, S, D, U],
        Side::East => [S, N, D, U],
    }
}

#[test]
fn all_sixteen_edge_cases_pick_their_base_tile() {
    let expected = [0, 3, 1, 2, 12, 15, 13, 14, 36, 39, 37, 38, 24, 27, 25, 26];
    let ctm = rule(Method::Ctm, 47);
    for side in [
        Side::Down,
        Side::Up,
        Side::North,
        Side::South,
        Side::West,
        Side::East,
    ] {
        for (mask, tile) in expected.iter().enumerate() {
            let connected: Vec<[i32; 3]> = (0..4)
                .filter(|bit| mask & (1 << bit) != 0)
                .map(|bit| edges(side)[bit])
                .collect();
            let world = World::new(&connected);
            assert_eq!(
                ctm.select(&world, side, Axis::Y, false),
                Some(*tile),
                "{side:?} {mask:04b}"
            );
            // With no corner block connected the fancy refinement only
            // changes tiles that have corners to draw; a tile with all
            // edges absent stays 0.
            if mask == 0 {
                assert_eq!(ctm.select(&world, side, Axis::Y, true), Some(0));
            }
        }
    }
}

#[test]
fn fast_ignores_corners_and_fancy_draws_them() {
    // A glass wall seen from the south: neighbours on all four edges, and
    // the up-left corner (west+up) missing.
    let ctm = rule(Method::Ctm, 47);
    let mut connected = vec![W, E, D, U, add(E, D), add(W, D), add(E, U)];
    let world = World::new(&connected);
    assert_eq!(ctm.select(&world, Side::South, Axis::Y, false), Some(26));
    // Corners of the south face in order: E+D, W+D, E+U, W+U; the missing
    // one is the fourth, which is tile 45.
    assert_eq!(ctm.select(&world, Side::South, Axis::Y, true), Some(45));
    connected.push(add(W, U));
    let full = World::new(&connected);
    assert_eq!(ctm.select(&full, Side::South, Axis::Y, true), Some(26));
    let none = World::new(&[W, E, D, U]);
    assert_eq!(ctm.select(&none, Side::South, Axis::Y, true), Some(46));
}

const fn add(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[test]
fn corner_refinements_replace_partial_bases() {
    let ctm = rule(Method::Ctm, 47);
    // South face: left=W, right=E, up=D, down=U. Base 13 is right+up.
    let base13 = [E, D];
    let corners = |extra: &[[i32; 3]]| {
        let mut all = base13.to_vec();
        all.extend_from_slice(extra);
        World::new(&all)
    };
    // The first corner of the south face is E+D: connected keeps 13,
    // missing gives 4.
    assert_eq!(
        ctm.select(&corners(&[add(E, D)]), Side::South, Axis::Y, true),
        Some(13)
    );
    assert_eq!(
        ctm.select(&corners(&[]), Side::South, Axis::Y, true),
        Some(4)
    );
    assert_eq!(
        ctm.select(&corners(&[]), Side::South, Axis::Y, false),
        Some(13)
    );
}

#[test]
fn refine_covers_each_documented_replacement() {
    let cases = [
        (13, [true, false, false, false], 4),
        (15, [false, true, false, false], 5),
        (37, [false, false, true, false], 16),
        (39, [false, false, false, true], 17),
        (14, [true, true, false, false], 7),
        (25, [true, false, true, false], 6),
        (27, [false, true, false, true], 19),
        (38, [false, false, true, true], 18),
        (14, [false, true, false, false], 31),
        (25, [true, false, false, false], 30),
        (27, [false, true, false, false], 41),
        (38, [false, false, false, true], 40),
        (14, [true, false, false, false], 29),
        (25, [false, false, true, false], 28),
        (27, [false, false, false, true], 43),
        (38, [false, false, true, false], 42),
        (14, [false, false, false, false], 14),
        (3, [true, true, true, true], 3),
    ];
    for (base, open, tile) in cases {
        assert_eq!(select::refine_for_test(base, open), tile, "{base} {open:?}");
    }
    // All sixteen corner patterns of the full tile.
    let full = [
        ([true, true, true, true], 46),
        ([false, true, true, true], 9),
        ([true, false, true, true], 21),
        ([true, true, false, true], 8),
        ([true, true, true, false], 20),
        ([true, true, false, false], 11),
        ([false, false, true, true], 22),
        ([false, true, false, true], 23),
        ([true, false, true, false], 10),
        ([true, false, false, true], 34),
        ([false, true, true, false], 35),
        ([true, false, false, false], 32),
        ([false, true, false, false], 33),
        ([false, false, true, false], 44),
        ([false, false, false, true], 45),
        ([false, false, false, false], 26),
    ];
    for (open, tile) in full {
        assert_eq!(select::refine_for_test(26, open), tile, "{open:?}");
    }
}

// ---- horizontal, vertical and combinations ----

#[test]
fn horizontal_picks_by_the_two_side_neighbours() {
    let rule = rule(Method::Horizontal, 4);
    // North face: first neighbour east, second west.
    let at =
        |connected: &[[i32; 3]]| rule.select(&World::new(connected), Side::North, Axis::Y, false);
    assert_eq!(at(&[E, W]), Some(1));
    assert_eq!(at(&[E]), Some(2));
    assert_eq!(at(&[W]), Some(0));
    assert_eq!(at(&[]), Some(3));
    // South face mirrors it.
    let south =
        |connected: &[[i32; 3]]| rule.select(&World::new(connected), Side::South, Axis::Y, false);
    assert_eq!(south(&[W]), Some(2));
    assert_eq!(south(&[E]), Some(0));
    // Faces parallel to the axis take no tile.
    assert_eq!(
        rule.select(&World::new(&[E, W]), Side::Up, Axis::Y, false),
        None
    );
    assert_eq!(
        rule.select(&World::new(&[E, W]), Side::Down, Axis::Y, false),
        None
    );
}

#[test]
fn vertical_picks_by_the_blocks_above_and_below() {
    let rule = rule(Method::Vertical, 4);
    let at =
        |connected: &[[i32; 3]]| rule.select(&World::new(connected), Side::East, Axis::Y, false);
    assert_eq!(at(&[D, U]), Some(1));
    assert_eq!(at(&[D]), Some(2));
    assert_eq!(at(&[U]), Some(0));
    assert_eq!(at(&[]), Some(3));
    assert_eq!(
        rule.select(&World::new(&[U]), Side::Up, Axis::Y, false),
        None
    );
    // A log lying along X: its ends take no tile, its sides read west/east.
    assert_eq!(
        rule.select(&World::new(&[W]), Side::West, Axis::X, false),
        None
    );
    assert_eq!(
        rule.select(&World::new(&[W]), Side::Up, Axis::X, false),
        Some(2)
    );
    // Along Z: south/north.
    assert_eq!(
        rule.select(&World::new(&[S]), Side::Up, Axis::Z, false),
        Some(2)
    );
    assert_eq!(
        rule.select(&World::new(&[S]), Side::North, Axis::Z, false),
        None
    );
}

#[test]
fn horizontal_vertical_falls_back_to_the_vertical_tiles() {
    let hv = rule(Method::HorizontalVertical, 7);
    let at =
        |connected: &[[i32; 3]]| hv.select(&World::new(connected), Side::North, Axis::Y, false);
    assert_eq!(at(&[E, W]), Some(1), "horizontal wins");
    assert_eq!(
        at(&[E, U]),
        Some(2),
        "a horizontal neighbour beats a vertical one"
    );
    assert_eq!(at(&[D, U]), Some(5), "vertical both: 1 becomes 5");
    assert_eq!(at(&[D]), Some(6), "vertical first only: 2 becomes 6");
    assert_eq!(at(&[U]), Some(4), "vertical second only: 0 becomes 4");
    assert_eq!(at(&[]), Some(3));
    // On a top face the horizontal method has no answer for the Y axis
    // and the vertical one has none either.
    assert_eq!(hv.select(&World::new(&[]), Side::Up, Axis::Y, false), None);
}

#[test]
fn vertical_horizontal_prefers_the_vertical_tiles() {
    let vh = rule(Method::VerticalHorizontal, 7);
    let at =
        |connected: &[[i32; 3]]| vh.select(&World::new(connected), Side::North, Axis::Y, false);
    assert_eq!(at(&[D, U, E, W]), Some(1), "vertical wins");
    assert_eq!(at(&[E, W]), Some(5), "horizontal both: 1 becomes 5");
    assert_eq!(at(&[E]), Some(6));
    assert_eq!(at(&[W]), Some(4));
    assert_eq!(at(&[]), Some(3));
}

// ---- top ----

#[test]
fn top_shows_its_tile_under_a_neighbour_above() {
    let top = rule(Method::Top, 1);
    assert_eq!(
        top.select(&World::new(&[U]), Side::North, Axis::Y, false),
        Some(0)
    );
    assert_eq!(
        top.select(&World::new(&[]), Side::North, Axis::Y, false),
        None
    );
    assert_eq!(
        top.select(&World::new(&[U]), Side::Up, Axis::Y, false),
        None
    );
    assert_eq!(
        top.select(&World::new(&[E]), Side::Up, Axis::X, false),
        Some(0)
    );
    assert_eq!(
        top.select(&World::new(&[S]), Side::East, Axis::Z, false),
        Some(0)
    );
}

// ---- random, repeat, fixed ----

#[test]
fn random_is_deterministic_and_uses_every_tile() {
    let random = rule(Method::Random, 4);
    let pick = |x: i32, z: i32, side: Side| {
        let mut world = World::new(&[]);
        world.position = [x, 70, z];
        random.select(&world, side, Axis::Y, false).expect("a tile")
    };
    assert_eq!(pick(3, 5, Side::North), pick(3, 5, Side::North));
    let mut seen = [false; 4];
    for x in 0..32 {
        for z in 0..32 {
            seen[pick(x, z, Side::North)] = true;
        }
    }
    assert_eq!(seen, [true; 4]);
}

#[test]
fn random_weights_bias_the_pick() {
    let mut weighted = rule(Method::Random, 2);
    weighted.weights = Some(Weights {
        cumulative: vec![1, 100],
        total: 100,
    });
    let mut counts = [0usize; 2];
    for x in 0..64 {
        for z in 0..64 {
            let mut world = World::new(&[]);
            world.position = [x, 10, z];
            counts[weighted
                .select(&world, Side::Up, Axis::Y, false)
                .expect("a tile")] += 1;
        }
    }
    assert!(counts[1] > counts[0] * 20, "{counts:?}");
}

#[test]
fn random_symmetry_shares_a_pick_between_faces() {
    let mut random = rule(Method::Random, 8);
    let pick = |rule: &CtmRule, side| {
        let mut world = World::new(&[]);
        world.position = [11, 70, -4];
        rule.select(&world, side, Axis::Y, false)
    };
    random.symmetry = Symmetry::Opposite;
    assert_eq!(pick(&random, Side::North), pick(&random, Side::South));
    assert_eq!(pick(&random, Side::Down), pick(&random, Side::Up));
    random.symmetry = Symmetry::All;
    let all: Vec<_> = [
        Side::Down,
        Side::Up,
        Side::North,
        Side::South,
        Side::West,
        Side::East,
    ]
    .iter()
    .map(|side| pick(&random, *side))
    .collect();
    assert!(all.windows(2).all(|pair| pair[0] == pair[1]));
    random.symmetry = Symmetry::None;
    let any_differs = (0..4).any(|n| {
        let mut world = World::new(&[]);
        world.position = [n, 70, 0];
        random.select(&world, Side::North, Axis::Y, false)
            != random.select(&world, Side::South, Axis::Y, false)
    });
    assert!(any_differs);
}

#[test]
fn linked_random_picks_like_the_bottom_of_the_stack() {
    let mut linked = rule(Method::Random, 8);
    linked.linked = true;
    let mut stack = World::new(&[]);
    stack.position = [4, 70, 9];
    stack.same = [[0, -1, 0], [0, -2, 0]].into_iter().collect();
    let mut bottom = World::new(&[]);
    bottom.position = [4, 68, 9];
    assert_eq!(
        linked.select(&stack, Side::North, Axis::Y, false),
        linked.select(&bottom, Side::North, Axis::Y, false)
    );
}

#[test]
fn a_single_random_tile_is_always_chosen() {
    let random = rule(Method::Random, 1);
    assert_eq!(
        random.select(&World::new(&[]), Side::Up, Axis::Y, false),
        Some(0)
    );
}

#[test]
fn repeat_wraps_the_position_per_face() {
    let repeat = rule(
        Method::Repeat {
            width: 2,
            height: 3,
        },
        6,
    );
    let at = |position: [i32; 3], side| {
        let mut world = World::new(&[]);
        world.position = position;
        repeat.select(&world, side, Axis::Y, false).expect("a tile")
    };
    // Top: x then z.
    assert_eq!(at([0, 0, 0], Side::Up), 0);
    assert_eq!(at([1, 0, 0], Side::Up), 1);
    assert_eq!(at([0, 0, 1], Side::Up), 2);
    assert_eq!(at([3, 0, 4], Side::Down), 3);
    // South: x runs east, rows go down.
    assert_eq!(at([1, 0, 0], Side::South), 1);
    assert_eq!(at([0, -1, 0], Side::South), 2);
    assert_eq!(at([0, 5, 0], Side::South), 2, "y 5 maps to row 1 of 3");
    // North: x runs the other way.
    assert_eq!(at([0, 0, 0], Side::North), 1);
    assert_eq!(at([1, 0, 0], Side::North), 0);
    // West uses z, east uses -z-1.
    assert_eq!(at([0, 0, 1], Side::West), 1);
    assert_eq!(at([0, 0, 0], Side::East), 1);
    assert_eq!(at([0, 0, -2], Side::East), 1);
}

#[test]
fn fixed_is_always_its_tile() {
    let fixed = rule(Method::Fixed, 1);
    assert_eq!(
        fixed.select(&World::new(&[]), Side::West, Axis::Y, false),
        Some(0)
    );
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the natural-texture parser and the quad permutation.

use super::*;

const SQUARE: [[f32; 2]; 4] = [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]];

fn kind(text: &str) -> Natural {
    Natural::parse(text).expect("valid kind")
}

#[test]
fn the_five_kinds_parse_and_others_do_not() {
    for text in ["4", "2", "F", "4F", "2F"] {
        assert!(Natural::parse(text).is_ok(), "{text}");
    }
    for text in ["", "3", "f", "F4", "4 F"] {
        assert_eq!(
            Natural::parse(text),
            Err(NaturalError::UnknownKind(text.to_string()))
        );
    }
}

#[test]
fn a_file_keeps_good_lines_and_reports_bad_ones() {
    let rules = NaturalRules::parse(b"# c\ngrass_top = 4F\nstone=2\nbad=9\n").expect("parses");
    let names: Vec<_> = rules
        .entries
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(names, ["grass_top", "stone"]);
    assert_eq!(rules.rejected.len(), 1);
    assert_eq!(rules.rejected[0].0, "bad");
}

#[test]
fn the_choice_is_deterministic_per_position_and_face() {
    let natural = kind("4F");
    for x in -5..5 {
        let position = [x, 64, 3];
        assert_eq!(natural.variant(position, 1), natural.variant(position, 1));
    }
}

#[test]
fn every_turn_and_mirror_is_reachable_and_ranges_hold() {
    let mut seen = std::collections::HashSet::new();
    for x in 0..64 {
        let variant = kind("4F").variant([x, 70, 9], 1);
        assert!(variant.turns < 4);
        seen.insert((variant.turns, variant.flip));
    }
    assert_eq!(seen.len(), 8);
    for x in 0..64 {
        let half = kind("2").variant([x, 70, 9], 1);
        assert!(matches!(half.turns, 0 | 2) && !half.flip);
        assert_eq!(kind("F").variant([x, 70, 9], 1).turns, 0);
    }
}

#[test]
fn turning_four_times_is_the_identity_and_one_turn_shifts_the_corners() {
    let one = Variant {
        turns: 1,
        flip: false,
    };
    assert_eq!(
        one.apply(SQUARE),
        [SQUARE[1], SQUARE[2], SQUARE[3], SQUARE[0]]
    );
    let mut uvs = SQUARE;
    for _ in 0..4 {
        uvs = one.apply(uvs);
    }
    assert_eq!(uvs, SQUARE);
    assert!(Variant {
        turns: 0,
        flip: false
    }
    .is_identity());
}

#[test]
fn a_mirror_reverses_the_corner_order() {
    let flip = Variant {
        turns: 0,
        flip: true,
    };
    assert_eq!(
        flip.apply(SQUARE),
        [SQUARE[3], SQUARE[2], SQUARE[1], SQUARE[0]]
    );
    assert_eq!(flip.apply(flip.apply(SQUARE)), SQUARE);
}

#[test]
fn a_partial_face_is_never_turned() {
    let partial = [[0.0, 0.0], [0.0, 0.5], [1.0, 0.5], [1.0, 0.0]];
    assert_eq!(
        Variant {
            turns: 3,
            flip: false
        }
        .apply(partial),
        partial
    );
}

proptest::proptest! {
    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(proptest::num::u8::ANY, 0..256)) {
        let _ = NaturalRules::parse(&bytes);
    }

    #[test]
    fn a_variant_keeps_the_same_four_corners(x in -1000..1000, y in 0..256, z in -1000..1000, side in -1..6) {
        let variant = kind("4F").variant([x, y, z], side);
        let mut out = variant.apply(SQUARE).to_vec();
        let mut expected = SQUARE.to_vec();
        out.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
        expected.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
        proptest::prop_assert_eq!(out, expected);
    }
}

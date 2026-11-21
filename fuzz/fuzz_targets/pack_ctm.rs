// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fuzzes the connected-texture rule parser of `umicraft-packext`, and the
//! tile choice of every rule it accepts. Neither may panic on any input.
#![no_main]

use libfuzzer_sys::fuzz_target;
use umicraft_packext::ctm::{parse_rules, Axis, CtmNeighbours, Side};

/// A world where every other block connects.
struct Checker;

impl CtmNeighbours for Checker {
    fn position(&self) -> [i32; 3] {
        [-3, 70, 9]
    }
    fn connects(&self, offset: [i32; 3]) -> bool {
        (offset[0] + offset[1] + offset[2]) % 2 == 0
    }
    fn same_block(&self, offset: [i32; 3]) -> bool {
        offset[1] > -3
    }
}

fuzz_target!(|data: &[u8]| {
    let rules = parse_rules([("ctm/fuzz/rule.properties", data)], &|name| {
        (name == "minecraft:glass").then_some(20)
    });
    for rule in &rules.rules {
        for side in [Side::Down, Side::Up, Side::North, Side::South, Side::West, Side::East] {
            for axis in [Axis::Y, Axis::Z, Axis::X] {
                for fancy in [false, true] {
                    if let Some(tile) = rule.select(&Checker, side, axis, fancy) {
                        assert!(tile < rule.tiles.len());
                    }
                }
            }
        }
    }
});

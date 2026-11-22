// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fuzzes the pack-feature text formats of `umicraft-packext`: the
//! `.properties` parser, then every value it yields through the range-list,
//! biome and weather parsers. None of them may panic on any input.
#![no_main]

use libfuzzer_sys::fuzz_target;
use umicraft_packext::{BiomeFilter, Properties, RangeList, WeatherSet};

fuzz_target!(|data: &[u8]| {
    let Ok(props) = Properties::parse(data) else {
        return;
    };
    for key in props.keys() {
        let Some(value) = props.get(key) else {
            continue;
        };
        if let Ok(list) = RangeList::parse(value) {
            let _ = list.contains(64);
        }
        let _ = BiomeFilter::parse(value).matches("Extreme Hills");
        let _ = WeatherSet::parse(value);
    }
});

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fuzzes the custom entity model parser of `umicraft-packext`: the `.jem`
//! JSON, then every animation expression it holds, compiled and evaluated.
//! None of it may panic on any input.
#![no_main]

use libfuzzer_sys::fuzz_target;
use umicraft_packext::cem::{parse_jem, CemAnimator, EntityInputs};

fuzz_target!(|data: &[u8]| {
    let Ok(model) = parse_jem("cem/fuzz.jem", data, &|_| None) else {
        return;
    };
    let animator = CemAnimator::new(&model);
    let mut state = animator.new_state(1);
    animator.evaluate(&mut state, &EntityInputs::default(), &[], 0.016);
    for index in 0..animator.part_count() {
        let _ = animator.pose(&state, index);
    }
});

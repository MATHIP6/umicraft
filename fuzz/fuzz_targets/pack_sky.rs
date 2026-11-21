// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fuzzes the custom sky layer parser of `umicraft-packext`: arbitrary bytes
//! as a `sky<K>.properties` file, then the per-frame maths of the layer it
//! yields. Nothing may panic on any input.
#![no_main]

use libfuzzer_sys::fuzz_target;
use umicraft_packext::sky::{SkyInputs, SkyLayer, SkyPlan, SkyState};

fuzz_target!(|data: &[u8]| {
    let path = "assets/minecraft/optifine/sky/world0/sky1.properties";
    let Ok(layer) = SkyLayer::parse(data, path) else {
        return;
    };
    for time in [0, 5_999, 6_000, 23_999, 24_000, -1, i64::MAX] {
        let _ = layer.is_active(time);
        let _ = layer.rotation_degrees(time, 0.3);
    }
    let _ = layer.position_match("Desert", 64);
    let plan = SkyPlan::from_worlds(vec![vec![layer]]);
    let inputs = SkyInputs {
        world_time: 13_000,
        celestial_angle: 0.5,
        rain: 0.5,
        thunder: 0.25,
        biome_name: "Desert",
        eye_block_y: 64,
        frame_seconds: 0.016,
    };
    let (mut state, mut out) = (SkyState::default(), Vec::new());
    plan.evaluate(0, &inputs, &mut state, &mut out);
});

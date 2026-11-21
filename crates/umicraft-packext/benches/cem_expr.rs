// SPDX-License-Identifier: MIT OR Apache-2.0

//! Cost of evaluating compiled CEM animation expressions (ADR-0089 step 5.3).

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use umicraft_packext::cem::expr::{compile, EvalContext, Resolver, SmoothCell, ValueType, VarSlot};

/// `limb_swing`, `limb_speed` and `age` in slots 0 to 2.
struct Vars;

impl Resolver for Vars {
    fn resolve(&mut self, name: &str) -> Option<VarSlot> {
        let slot = match name {
            "limb_swing" => 0,
            "limb_speed" => 1,
            "age" => 2,
            _ => return None,
        };
        Some(VarSlot {
            slot,
            ty: ValueType::Float,
        })
    }
}

fn bench(c: &mut Criterion) {
    let memory = [3.2_f32, 0.7, 120.5];
    let cases = [
        ("walk_swing", "sin(limb_swing * 0.6662) * 1.4 * limb_speed"),
        (
            "branching",
            "if(limb_speed > 0.1, clamp(cos(age * 0.09) * 0.05 + limb_swing * 0.1, -1, 1), 0)",
        ),
        ("smoothed", "smooth(1, sin(age * 0.1) * 20, 0.2)"),
    ];
    for (name, source) in cases {
        let program = compile(source, &mut Vars).expect("benchmark expression compiles");
        let mut cells = vec![SmoothCell::default(); program.smooth_sites()];
        let mut rng = 1;
        c.bench_function(&format!("cem_expr_{name}"), |b| {
            b.iter(|| {
                let mut ctx = EvalContext {
                    smooth: &mut cells,
                    rng: &mut rng,
                    dt: 0.016,
                };
                black_box(program.eval(black_box(&memory), &mut ctx))
            });
        });
    }
}

criterion_group!(benches, bench);
criterion_main!(benches);

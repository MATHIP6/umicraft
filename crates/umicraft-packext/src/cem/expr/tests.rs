// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use super::*;

/// Variables `x`, `y` (floats) and `flag` (bool) in slots 0, 1, 2.
struct Vars(HashMap<&'static str, VarSlot>);

impl Vars {
    fn new() -> Self {
        let float = |slot| VarSlot {
            slot,
            ty: ValueType::Float,
        };
        Self(HashMap::from([
            ("x", float(0)),
            ("y", float(1)),
            (
                "flag",
                VarSlot {
                    slot: 2,
                    ty: ValueType::Bool,
                },
            ),
            ("time", float(3)),
        ]))
    }
}

impl Resolver for Vars {
    fn resolve(&mut self, name: &str) -> Option<VarSlot> {
        self.0.get(name).copied()
    }
}

const MEMORY: [f32; 4] = [3.0, -2.0, 1.0, 7.0];

fn eval(source: &str) -> f32 {
    let program = compile(source, &mut Vars::new()).unwrap_or_else(|e| panic!("{source}: {e}"));
    let mut cells = vec![SmoothCell::default(); program.smooth_sites()];
    let mut rng = 1;
    let mut ctx = EvalContext {
        smooth: &mut cells,
        rng: &mut rng,
        dt: 0.1,
    };
    program.eval(&MEMORY, &mut ctx)
}

fn error(source: &str) -> ExprError {
    compile(source, &mut Vars::new()).expect_err(source)
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-5
}

#[test]
fn arithmetic_follows_the_precedence_table() {
    let cases = [
        ("1 + 2 * 3", 7.0),
        ("(1 + 2) * 3", 9.0),
        ("10 - 4 - 3", 3.0),
        ("8 / 2 / 2", 2.0),
        ("-2 * 3", -6.0),
        ("- -4", 4.0),
        ("x * y + 1", -5.0),
        ("7 % 4", 3.0),
        ("-7 % 4", -3.0),
        ("7.5 % 2", 1.5),
        (".5 + .25", 0.75),
    ];
    for (source, want) in cases {
        assert!(close(eval(source), want), "{source} = {}", eval(source));
    }
}

#[test]
fn comparisons_and_logic_bind_in_the_documented_order() {
    let cases = [
        ("1 + 1 == 2", 1.0),
        ("2 + 1 == 3 && 1 < 2", 1.0), // arithmetic binds tighter than ==, == tighter than &&
        ("2 > 3 || 1 < 2 && 5 > 4", 1.0), // && (3) is tighter than || (2)
        ("!(1 > 2) && flag", 1.0),
        ("!flag || false", 0.0),
        ("x != 3", 0.0),
        ("x >= 3 && y <= -2", 1.0),
        ("true && false || true", 1.0),
    ];
    for (source, want) in cases {
        assert!(
            close(eval(&format!("if({source}, 1, 0)")), want),
            "{source}"
        );
    }
}

#[test]
fn every_one_argument_function() {
    let cases = [
        ("sin(0)", 0.0),
        ("cos(0)", 1.0),
        ("tan(0)", 0.0),
        ("asin(1)", std::f32::consts::FRAC_PI_2),
        ("acos(1)", 0.0),
        ("atan(1)", std::f32::consts::FRAC_PI_4),
        ("torad(180)", std::f32::consts::PI),
        ("todeg(pi)", 180.0),
        ("abs(-2.5)", 2.5),
        ("floor(-1.5)", -2.0),
        ("ceil(1.2)", 2.0),
        ("exp(0)", 1.0),
        ("frac(2.75)", 0.75),
        ("frac(-0.25)", 0.75),
        ("log(1)", 0.0),
        ("round(2.5)", 3.0),
        ("round(-2.5)", -2.0),
        ("round(1.4)", 1.0),
        ("signum(-9)", -1.0),
        ("signum(0)", 0.0),
        ("sqrt(16)", 4.0),
    ];
    for (source, want) in cases {
        assert!(close(eval(source), want), "{source} = {}", eval(source));
    }
}

#[test]
fn multi_argument_functions() {
    let cases = [
        ("atan2(1, 1)", std::f32::consts::FRAC_PI_4),
        ("pow(2, 10)", 1024.0),
        ("fmod(-1, 3)", 2.0),
        ("fmod(7, 3)", 1.0),
        ("min(3, 1, 2)", 1.0),
        ("max(3, 1, 2)", 3.0),
        ("min(x, y)", -2.0),
        ("clamp(5, 0, 1)", 1.0),
        ("clamp(-5, 0, 1)", 0.0),
        ("clamp(0.5, 0, 1)", 0.5),
        ("clamp(2, 3, 1)", 3.0), // reversed bounds do not panic
        ("lerp(0.25, 0, 8)", 2.0),
        ("if(false, 1, true, 2, 3)", 2.0),
        ("if(false, 1, false, 2, 3)", 3.0),
        ("if(1 < 2, 10, 20)", 10.0),
        ("pi", std::f32::consts::PI),
        ("time", 7.0),
        ("time()", 7.0),
    ];
    for (source, want) in cases {
        assert!(close(eval(source), want), "{source} = {}", eval(source));
    }
}

#[test]
fn boolean_functions() {
    let cases = [
        ("between(2, 1, 3)", 1.0),
        ("between(4, 1, 3)", 0.0),
        ("between(1, 1, 3)", 1.0),
        ("equals(1.0, 1.05, 0.1)", 1.0),
        ("equals(1.0, 1.5, 0.1)", 0.0),
        ("in(2, 1, 2, 3)", 1.0),
        ("in(5, 1, 2, 3)", 0.0),
        ("ifb(true, flag, false)", 1.0),
    ];
    for (source, want) in cases {
        assert!(
            close(eval(&format!("if({source}, 1, 0)")), want),
            "{source}"
        );
    }
}

#[test]
fn if_does_not_evaluate_the_branch_it_skips() {
    // The skipped `smooth` must not prime its cell.
    let program = compile("if(false, smooth(0, 5), 1)", &mut Vars::new()).unwrap();
    let mut cells = vec![SmoothCell::default(); program.smooth_sites()];
    let mut rng = 1;
    let mut ctx = EvalContext {
        smooth: &mut cells,
        rng: &mut rng,
        dt: 0.1,
    };
    assert!(close(program.eval(&MEMORY, &mut ctx), 1.0));
    assert_eq!(cells[0], SmoothCell::default());
}

#[test]
fn logic_short_circuits() {
    let program = compile("if(false && smooth(0, 1) > 0, 1, 2)", &mut Vars::new());
    // `smooth(...) > 0` is a bool, so this compiles; the right side is skipped.
    let program = program.unwrap();
    let mut cells = vec![SmoothCell::default(); program.smooth_sites()];
    let mut rng = 1;
    let mut ctx = EvalContext {
        smooth: &mut cells,
        rng: &mut rng,
        dt: 0.1,
    };
    assert!(close(program.eval(&MEMORY, &mut ctx), 2.0));
    assert_eq!(cells[0], SmoothCell::default());
}

#[test]
fn smooth_starts_at_the_target_then_approaches_it() {
    let program = compile("smooth(0, x, 1)", &mut Vars::new()).unwrap();
    let mut cells = vec![SmoothCell::default(); program.smooth_sites()];
    let mut rng = 1;
    let mut run = |memory: &[f32]| {
        let mut ctx = EvalContext {
            smooth: &mut cells,
            rng: &mut rng,
            dt: 0.5,
        };
        program.eval(memory, &mut ctx)
    };
    assert!(
        close(run(&[10.0]), 10.0),
        "the first call returns the target"
    );
    let step = run(&[0.0]);
    assert!(step > 0.0 && step < 10.0, "moved part of the way: {step}");
    let next = run(&[0.0]);
    assert!(next < step, "keeps falling");
}

#[test]
fn smooth_with_separate_fall_time_and_zero_fade() {
    let program = compile("smooth(0, x, 0)", &mut Vars::new()).unwrap();
    let mut cells = vec![SmoothCell::default(); 1];
    let mut rng = 1;
    let mut ctx = EvalContext {
        smooth: &mut cells,
        rng: &mut rng,
        dt: 0.1,
    };
    assert!(close(program.eval(&[4.0], &mut ctx), 4.0));
    assert!(
        close(program.eval(&[9.0], &mut ctx), 9.0),
        "no fade follows at once"
    );
}

#[test]
fn random_is_in_range_and_seeded_calls_repeat() {
    for source in ["random()", "random", "random(3)"] {
        let v = eval(source);
        assert!((0.0..1.0).contains(&v), "{source} = {v}");
    }
    assert_eq!(eval("random(5)"), eval("random(5)"));
    assert_ne!(eval("random(5)"), eval("random(6)"));
}

#[test]
fn unseeded_random_advances() {
    let program = compile("random()", &mut Vars::new()).unwrap();
    let mut rng = 1;
    let mut ctx = EvalContext {
        smooth: &mut [],
        rng: &mut rng,
        dt: 0.0,
    };
    let a = program.eval(&[], &mut ctx);
    let b = program.eval(&[], &mut ctx);
    assert_ne!(a, b);
}

#[test]
fn division_by_zero_gives_infinity_not_a_panic() {
    assert!(eval("1 / 0").is_infinite());
    assert!(eval("0 / 0").is_nan());
}

#[test]
fn variables_read_the_memory_slots() {
    assert!(close(eval("x + y"), 1.0));
    let program = compile("x", &mut Vars::new()).unwrap();
    let mut rng = 1;
    let mut ctx = EvalContext {
        smooth: &mut [],
        rng: &mut rng,
        dt: 0.0,
    };
    assert_eq!(
        program.eval(&[], &mut ctx),
        0.0,
        "outside the memory reads as 0"
    );
}

#[test]
fn types_are_checked() {
    let float_expected = |e: ExprError| {
        matches!(
            e,
            ExprError::Type {
                expected: ValueType::Float,
                found: ValueType::Bool
            }
        )
    };
    assert!(float_expected(error("flag + 1")));
    assert!(float_expected(error("-true")));
    assert!(float_expected(error("sin(1 < 2)")));
    let bool_expected = |e: ExprError| {
        matches!(
            e,
            ExprError::Type {
                expected: ValueType::Bool,
                found: ValueType::Float
            }
        )
    };
    assert!(bool_expected(error("!1")));
    assert!(bool_expected(error("1 && true")));
    assert!(bool_expected(error("if(1, 2, 3)")));
    assert!(matches!(error("if(true, flag, 3)"), ExprError::Type { .. }));
}

#[test]
fn bad_programs_report_why() {
    assert_eq!(error("nope + 1"), ExprError::UnknownVariable("nope".into()));
    assert_eq!(
        error("frobnicate(1)"),
        ExprError::UnknownFunction("frobnicate".into())
    );
    assert!(matches!(
        error("sin(1, 2)"),
        ExprError::Arity { found: 2, .. }
    ));
    assert!(matches!(error("sin()"), ExprError::Arity { found: 0, .. }));
    assert!(matches!(error("min(1)"), ExprError::Arity { .. }));
    assert!(matches!(error("if(true, 1)"), ExprError::Arity { .. }));
    assert!(matches!(error("smooth(1)"), ExprError::Arity { .. }));
    assert!(matches!(error("1 +"), ExprError::Syntax { .. }));
    assert!(matches!(error("(1"), ExprError::Syntax { .. }));
    assert!(matches!(error("1 2"), ExprError::Syntax { .. }));
    assert!(matches!(
        error("1 $ 2"),
        ExprError::UnexpectedChar { offset: 2, .. }
    ));
    assert!(matches!(error("1.2.3"), ExprError::BadNumber(_)));
    assert!(matches!(error(""), ExprError::Syntax { .. }));
}

#[test]
fn deep_nesting_is_refused_instead_of_overflowing() {
    let deep = format!("{}1{}", "(".repeat(5000), ")".repeat(5000));
    assert_eq!(error(&deep), ExprError::TooDeep);
    let negs = format!("{}1", "-".repeat(5000));
    assert_eq!(error(&negs), ExprError::TooDeep);
    let long_chain = format!("1{}", "+1".repeat(1000));
    assert_eq!(error(&long_chain), ExprError::TooDeep);
    let wide = format!("1{}", "+1".repeat(200));
    assert!(close(eval(&wide), 201.0), "reasonable chains are fine");
}

#[test]
fn the_stack_limit_is_enforced_at_compile_time() {
    let right_nested = format!("{}1{}", "1+(".repeat(40), ")".repeat(40));
    assert_eq!(error(&right_nested), ExprError::TooDeep);
}

#[test]
fn evaluation_reuses_its_buffers() {
    // The same cells and rng serve any number of evaluations.
    let program = compile("smooth(0, x) + random()", &mut Vars::new()).unwrap();
    let mut cells = vec![SmoothCell::default(); program.smooth_sites()];
    let mut rng = 9;
    for _ in 0..1000 {
        let mut ctx = EvalContext {
            smooth: &mut cells,
            rng: &mut rng,
            dt: 0.016,
        };
        assert!(program.eval(&MEMORY, &mut ctx).is_finite());
    }
}

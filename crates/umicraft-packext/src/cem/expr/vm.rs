// SPDX-License-Identifier: MIT OR Apache-2.0

//! The stack machine that runs compiled expressions.
//!
//! The stack is a fixed array on the machine stack and the variables are
//! read from a caller-owned slice, so [`run`] never allocates.

/// Depth of the evaluation stack; the compiler rejects deeper expressions.
pub(super) const STACK: usize = 32;

/// One-argument float functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum F1 {
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    ToRad,
    ToDeg,
    Abs,
    Floor,
    Ceil,
    Exp,
    Frac,
    Log,
    Round,
    Signum,
    Sqrt,
}

/// Two-argument float functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum F2 {
    Atan2,
    Pow,
    Fmod,
}

/// One instruction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Op {
    Const(f32),
    Load(u32),
    Neg,
    Not,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    F1(F1),
    F2(F2),
    Clamp,
    Between,
    Equals,
    Lerp,
    /// Minimum of the top `n` values.
    Min(u8),
    /// Maximum of the top `n` values.
    Max(u8),
    /// Whether the value under the top `n - 1` values equals one of them.
    In(u8),
    /// `smooth(id, value[, up[, down]])` with `argc` arguments, state at `site`.
    Smooth {
        site: u16,
        argc: u8,
    },
    /// `random()` or `random(seed)`.
    Random(u8),
    /// Pops a condition; jumps when it is false.
    JumpIfFalse(u32),
    /// Jumps when the top is false and keeps it, else pops it (`&&`).
    JumpIfFalseKeep(u32),
    /// Jumps when the top is true and keeps it, else pops it (`||`).
    JumpIfTrueKeep(u32),
    Jump(u32),
}

/// The state of one `smooth(...)` call site.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SmoothCell {
    value: f32,
    primed: bool,
}

/// What an evaluation needs besides the variables.
#[derive(Debug)]
pub struct EvalContext<'a> {
    /// One cell per `smooth` call site of the program.
    pub smooth: &'a mut [SmoothCell],
    /// State of the unseeded `random()`; any value, updated by each call.
    pub rng: &'a mut u32,
    /// Seconds since the previous evaluation, for `smooth`.
    pub dt: f32,
}

fn bool_f(b: bool) -> f32 {
    f32::from(u8::from(b))
}

fn f1(f: F1, x: f32) -> f32 {
    match f {
        F1::Sin => x.sin(),
        F1::Cos => x.cos(),
        F1::Tan => x.tan(),
        F1::Asin => x.asin(),
        F1::Acos => x.acos(),
        F1::Atan => x.atan(),
        F1::ToRad => x.to_radians(),
        F1::ToDeg => x.to_degrees(),
        F1::Abs => x.abs(),
        F1::Floor => x.floor(),
        F1::Ceil => x.ceil(),
        F1::Exp => x.exp(),
        F1::Frac => x - x.floor(),
        F1::Log => x.ln(),
        // Rounds halves up, as `floor(x + 0.5)`.
        F1::Round => (x + 0.5).floor(),
        F1::Signum => {
            if x == 0.0 || x.is_nan() {
                x
            } else {
                x.signum()
            }
        }
        F1::Sqrt => x.sqrt(),
    }
}

fn f2(f: F2, a: f32, b: f32) -> f32 {
    match f {
        F2::Atan2 => a.atan2(b),
        F2::Pow => a.powf(b),
        // Floored modulo: the result takes the sign of `b`.
        F2::Fmod => a - b * (a / b).floor(),
    }
}

/// `clamp` that does not panic when the bounds are reversed.
fn clamp(x: f32, lo: f32, hi: f32) -> f32 {
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

/// Maps `seed` to `[0, 1)` with a fixed integer hash, so a seed always gives
/// the same value.
fn hash_unit(seed: f32) -> f32 {
    let mut h = seed.to_bits().wrapping_mul(0x9E37_79B1);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 13;
    unit(h)
}

/// The top 24 bits of `bits` as a float in `[0, 1)`.
fn unit(bits: u32) -> f32 {
    (bits >> 8) as f32 / 16_777_216.0
}

fn next_random(state: &mut u32) -> f32 {
    // xorshift32; a zero state would stay zero, so it is nudged.
    let mut x = if *state == 0 { 0x6D2B_79F5 } else { *state };
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    unit(x)
}

/// Moves the cell of one `smooth` site toward `target`.
///
/// The first call returns `target`. Later calls approach it exponentially
/// with a time constant of `up` seconds when rising and `down` when falling;
/// a non-positive constant follows at once.
fn smooth(cell: &mut SmoothCell, target: f32, up: f32, down: f32, dt: f32) -> f32 {
    let fade = if target >= cell.value { up } else { down };
    if !cell.primed || fade <= 0.0 {
        cell.primed = true;
        cell.value = target;
    } else {
        cell.value += (target - cell.value) * (1.0 - (-dt.max(0.0) / fade).exp());
    }
    cell.value
}

struct Stack {
    items: [f32; STACK],
    len: usize,
}

impl Stack {
    fn push(&mut self, v: f32) {
        if let Some(slot) = self.items.get_mut(self.len) {
            *slot = v;
            self.len += 1;
        }
    }

    fn pop(&mut self) -> f32 {
        self.len = self.len.saturating_sub(1);
        self.items[self.len]
    }

    /// The top `n` values, oldest first, which stay on the stack.
    fn top(&self, n: usize) -> &[f32] {
        &self.items[self.len.saturating_sub(n)..self.len]
    }

    fn drop_n(&mut self, n: usize) {
        self.len = self.len.saturating_sub(n);
    }

    fn peek(&self) -> f32 {
        self.items[self.len.saturating_sub(1)]
    }

    fn binary(&mut self, f: impl FnOnce(f32, f32) -> f32) {
        let b = self.pop();
        let a = self.pop();
        self.push(f(a, b));
    }
}

/// Runs `ops` and returns the value left on the stack.
pub(super) fn run(ops: &[Op], memory: &[f32], ctx: &mut EvalContext<'_>) -> f32 {
    let mut st = Stack {
        items: [0.0; STACK],
        len: 0,
    };
    let mut pc = 0;
    while let Some(&op) = ops.get(pc) {
        pc += 1;
        match op {
            Op::Const(v) => st.push(v),
            Op::Load(slot) => st.push(memory.get(slot as usize).copied().unwrap_or(0.0)),
            Op::Neg => {
                let v = st.pop();
                st.push(-v);
            }
            Op::Not => {
                let v = st.pop();
                st.push(bool_f(v == 0.0));
            }
            Op::Add => st.binary(|a, b| a + b),
            Op::Sub => st.binary(|a, b| a - b),
            Op::Mul => st.binary(|a, b| a * b),
            Op::Div => st.binary(|a, b| a / b),
            // Truncating remainder: the result takes the sign of the dividend.
            Op::Rem => st.binary(|a, b| a - b * (a / b).trunc()),
            Op::Lt => st.binary(|a, b| bool_f(a < b)),
            Op::Le => st.binary(|a, b| bool_f(a <= b)),
            Op::Gt => st.binary(|a, b| bool_f(a > b)),
            Op::Ge => st.binary(|a, b| bool_f(a >= b)),
            Op::Eq => st.binary(|a, b| bool_f(a == b)),
            Op::Ne => st.binary(|a, b| bool_f(a != b)),
            Op::F1(f) => {
                let v = st.pop();
                st.push(f1(f, v));
            }
            Op::F2(f) => st.binary(|a, b| f2(f, a, b)),
            Op::Clamp => {
                let (hi, lo, x) = (st.pop(), st.pop(), st.pop());
                st.push(clamp(x, lo, hi));
            }
            Op::Between => {
                let (hi, lo, x) = (st.pop(), st.pop(), st.pop());
                st.push(bool_f(x >= lo && x <= hi));
            }
            Op::Equals => {
                let (eps, b, a) = (st.pop(), st.pop(), st.pop());
                st.push(bool_f((a - b).abs() <= eps));
            }
            Op::Lerp => {
                let (y, x, k) = (st.pop(), st.pop(), st.pop());
                st.push(x + k * (y - x));
            }
            Op::Min(n) => {
                let v = st.top(n.into()).iter().copied().reduce(f32::min);
                st.drop_n(n.into());
                st.push(v.unwrap_or(0.0));
            }
            Op::Max(n) => {
                let v = st.top(n.into()).iter().copied().reduce(f32::max);
                st.drop_n(n.into());
                st.push(v.unwrap_or(0.0));
            }
            Op::In(n) => {
                let args = st.top(n.into());
                let hit = args.split_first().is_some_and(|(x, rest)| rest.contains(x));
                st.drop_n(n.into());
                st.push(bool_f(hit));
            }
            Op::Smooth { site, argc } => {
                let args = st.top(argc.into());
                let value = args.get(1).copied().unwrap_or(0.0);
                let up = args.get(2).copied().unwrap_or(1.0);
                let down = args.get(3).copied().unwrap_or(up);
                st.drop_n(argc.into());
                let out = match ctx.smooth.get_mut(usize::from(site)) {
                    Some(cell) => smooth(cell, value, up, down, ctx.dt),
                    None => value,
                };
                st.push(out);
            }
            Op::Random(argc) => {
                let out = if argc == 0 {
                    next_random(ctx.rng)
                } else {
                    hash_unit(st.pop())
                };
                st.push(out);
            }
            Op::JumpIfFalse(target) => {
                if st.pop() == 0.0 {
                    pc = target as usize;
                }
            }
            Op::JumpIfFalseKeep(target) => {
                if st.peek() == 0.0 {
                    pc = target as usize;
                } else {
                    st.pop();
                }
            }
            Op::JumpIfTrueKeep(target) => {
                if st.peek() == 0.0 {
                    st.pop();
                } else {
                    pc = target as usize;
                }
            }
            Op::Jump(target) => pc = target as usize,
        }
    }
    st.peek()
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Type checking and code generation.
//!
//! Function names, arities and parameter types follow the public CEM
//! documentation (`cem_animation.txt`) and the function table of the
//! reference tree (ADR-0088).

use super::parser::{parse, Ast, BinOp};
use super::vm::{Op, F1, F2, STACK};
use super::{ExprError, Program, Resolver, ValueType};

/// Compiles `source`, looking its variables up through `resolver`.
///
/// # Errors
///
/// Any [`ExprError`]: bad syntax, unknown variable or function, wrong
/// argument count or type, or too deep nesting.
pub fn compile(source: &str, resolver: &mut dyn Resolver) -> Result<Program, ExprError> {
    let ast = parse(source)?;
    let mut gen = Generator {
        resolver,
        ops: Vec::new(),
        depth: 0,
        max_depth: 0,
        smooth_sites: 0,
    };
    let ty = gen.node(&ast)?;
    Ok(Program {
        ops: gen.ops,
        ty,
        smooth_sites: gen.smooth_sites,
    })
}

struct Generator<'a> {
    resolver: &'a mut dyn Resolver,
    ops: Vec<Op>,
    /// Values on the stack at this point of the code.
    depth: usize,
    max_depth: usize,
    smooth_sites: u16,
}

/// How many arguments a function accepts.
#[derive(Clone, Copy)]
enum Arity {
    Exact(usize),
    AtLeast(usize),
    Between(usize, usize),
}

impl Arity {
    fn accepts(self, n: usize) -> bool {
        match self {
            Self::Exact(k) => n == k,
            Self::AtLeast(k) => n >= k,
            Self::Between(lo, hi) => (lo..=hi).contains(&n),
        }
    }

    fn describe(self) -> String {
        match self {
            Self::Exact(k) => format!("exactly {k}"),
            Self::AtLeast(k) => format!("at least {k}"),
            Self::Between(lo, hi) => format!("{lo} to {hi}"),
        }
    }
}

fn float_fn1(name: &str) -> Option<F1> {
    Some(match name {
        "sin" => F1::Sin,
        "cos" => F1::Cos,
        "tan" => F1::Tan,
        "asin" => F1::Asin,
        "acos" => F1::Acos,
        "atan" => F1::Atan,
        "torad" => F1::ToRad,
        "todeg" => F1::ToDeg,
        "abs" => F1::Abs,
        "floor" => F1::Floor,
        "ceil" => F1::Ceil,
        "exp" => F1::Exp,
        "frac" => F1::Frac,
        "log" => F1::Log,
        "round" => F1::Round,
        "signum" => F1::Signum,
        "sqrt" => F1::Sqrt,
        _ => return None,
    })
}

fn float_fn2(name: &str) -> Option<F2> {
    Some(match name {
        "atan2" => F2::Atan2,
        "pow" => F2::Pow,
        "fmod" => F2::Fmod,
        _ => return None,
    })
}

impl Generator<'_> {
    fn emit(&mut self, op: Op, pops: usize, pushes: usize) -> Result<(), ExprError> {
        self.depth = self.depth.saturating_sub(pops) + pushes;
        self.max_depth = self.max_depth.max(self.depth);
        if self.max_depth > STACK {
            return Err(ExprError::TooDeep);
        }
        self.ops.push(op);
        Ok(())
    }

    fn here(&self) -> u32 {
        self.ops.len() as u32
    }

    /// Overwrites the jump emitted at `at` to land on the next instruction.
    fn patch(&mut self, at: usize) {
        let target = self.here();
        self.ops[at] = match self.ops[at] {
            Op::JumpIfFalse(_) => Op::JumpIfFalse(target),
            Op::JumpIfFalseKeep(_) => Op::JumpIfFalseKeep(target),
            Op::JumpIfTrueKeep(_) => Op::JumpIfTrueKeep(target),
            Op::Jump(_) => Op::Jump(target),
            other => other,
        };
    }

    fn expect(&mut self, ast: &Ast, want: ValueType) -> Result<(), ExprError> {
        let found = self.node(ast)?;
        if found == want {
            Ok(())
        } else {
            Err(ExprError::Type {
                expected: want,
                found,
            })
        }
    }

    fn node(&mut self, ast: &Ast) -> Result<ValueType, ExprError> {
        match ast {
            Ast::Num(n) => {
                self.emit(Op::Const(*n), 0, 1)?;
                Ok(ValueType::Float)
            }
            Ast::Name(name) => self.name(name),
            Ast::Neg(inner) => {
                self.expect(inner, ValueType::Float)?;
                self.emit(Op::Neg, 1, 1)?;
                Ok(ValueType::Float)
            }
            Ast::Not(inner) => {
                self.expect(inner, ValueType::Bool)?;
                self.emit(Op::Not, 1, 1)?;
                Ok(ValueType::Bool)
            }
            Ast::Bin(op, lhs, rhs) => self.binary(*op, lhs, rhs),
            Ast::Call(name, args) => self.call(name, args),
        }
    }

    /// A bare name: a constant or a variable.
    fn name(&mut self, name: &str) -> Result<ValueType, ExprError> {
        match name {
            "pi" => self.constant(std::f32::consts::PI, ValueType::Float),
            "true" => self.constant(1.0, ValueType::Bool),
            "false" => self.constant(0.0, ValueType::Bool),
            "random" => self.call(name, &[]),
            _ => self.variable(name),
        }
    }

    fn constant(&mut self, value: f32, ty: ValueType) -> Result<ValueType, ExprError> {
        self.emit(Op::Const(value), 0, 1)?;
        Ok(ty)
    }

    fn binary(&mut self, op: BinOp, lhs: &Ast, rhs: &Ast) -> Result<ValueType, ExprError> {
        use ValueType::{Bool, Float};
        match op {
            BinOp::And | BinOp::Or => return self.logic(op, lhs, rhs),
            _ => {}
        }
        self.expect(lhs, Float)?;
        self.expect(rhs, Float)?;
        let (instruction, ty) = match op {
            BinOp::Add => (Op::Add, Float),
            BinOp::Sub => (Op::Sub, Float),
            BinOp::Mul => (Op::Mul, Float),
            BinOp::Div => (Op::Div, Float),
            BinOp::Rem => (Op::Rem, Float),
            BinOp::Lt => (Op::Lt, Bool),
            BinOp::Le => (Op::Le, Bool),
            BinOp::Gt => (Op::Gt, Bool),
            BinOp::Ge => (Op::Ge, Bool),
            BinOp::Eq => (Op::Eq, Bool),
            BinOp::Ne => (Op::Ne, Bool),
            BinOp::And | BinOp::Or => unreachable!("handled above"),
        };
        self.emit(instruction, 2, 1)?;
        Ok(ty)
    }

    /// `&&` and `||`, which skip the right side when the left decides.
    fn logic(&mut self, op: BinOp, lhs: &Ast, rhs: &Ast) -> Result<ValueType, ExprError> {
        self.expect(lhs, ValueType::Bool)?;
        let jump = self.ops.len();
        let placeholder = if op == BinOp::And {
            Op::JumpIfFalseKeep(0)
        } else {
            Op::JumpIfTrueKeep(0)
        };
        // The jump keeps the left value; falling through pops it.
        self.emit(placeholder, 0, 0)?;
        self.depth -= 1;
        self.expect(rhs, ValueType::Bool)?;
        self.patch(jump);
        Ok(ValueType::Bool)
    }

    fn args(&mut self, args: &[Ast], ty: ValueType) -> Result<(), ExprError> {
        args.iter().try_for_each(|arg| self.expect(arg, ty))
    }

    fn check_arity(name: &str, arity: Arity, found: usize) -> Result<(), ExprError> {
        if arity.accepts(found) {
            Ok(())
        } else {
            Err(ExprError::Arity {
                name: name.to_string(),
                expected: arity.describe(),
                found,
            })
        }
    }

    fn call(&mut self, name: &str, args: &[Ast]) -> Result<ValueType, ExprError> {
        use ValueType::{Bool, Float};
        let n = args.len();
        if let Some(f) = float_fn1(name) {
            Self::check_arity(name, Arity::Exact(1), n)?;
            self.args(args, Float)?;
            self.emit(Op::F1(f), 1, 1)?;
            return Ok(Float);
        }
        if let Some(f) = float_fn2(name) {
            Self::check_arity(name, Arity::Exact(2), n)?;
            self.args(args, Float)?;
            self.emit(Op::F2(f), 2, 1)?;
            return Ok(Float);
        }
        let narg = u8::try_from(n).map_err(|_| ExprError::TooDeep)?;
        match name {
            "pi" | "true" | "false" => {
                Self::check_arity(name, Arity::Exact(0), n)?;
                self.name(name)
            }
            "time" => {
                Self::check_arity(name, Arity::Exact(0), n)?;
                self.variable("time")
            }
            "random" => {
                Self::check_arity(name, Arity::Between(0, 1), n)?;
                self.args(args, Float)?;
                self.emit(Op::Random(narg), n, 1)?;
                Ok(Float)
            }
            "min" | "max" => {
                Self::check_arity(name, Arity::AtLeast(2), n)?;
                self.args(args, Float)?;
                let op = if name == "min" {
                    Op::Min(narg)
                } else {
                    Op::Max(narg)
                };
                self.emit(op, n, 1)?;
                Ok(Float)
            }
            "clamp" | "lerp" => {
                Self::check_arity(name, Arity::Exact(3), n)?;
                self.args(args, Float)?;
                self.emit(if name == "clamp" { Op::Clamp } else { Op::Lerp }, 3, 1)?;
                Ok(Float)
            }
            "between" | "equals" => {
                Self::check_arity(name, Arity::Exact(3), n)?;
                self.args(args, Float)?;
                let op = if name == "between" {
                    Op::Between
                } else {
                    Op::Equals
                };
                self.emit(op, 3, 1)?;
                Ok(Bool)
            }
            "in" => {
                Self::check_arity(name, Arity::AtLeast(2), n)?;
                self.args(args, Float)?;
                self.emit(Op::In(narg), n, 1)?;
                Ok(Bool)
            }
            "smooth" => {
                Self::check_arity(name, Arity::Between(2, 4), n)?;
                self.args(args, Float)?;
                let site = self.smooth_sites;
                self.smooth_sites += 1;
                self.emit(Op::Smooth { site, argc: narg }, n, 1)?;
                Ok(Float)
            }
            "if" => self.conditional(args, Float),
            "ifb" => self.conditional(args, Bool),
            _ => Err(ExprError::UnknownFunction(name.to_string())),
        }
    }

    fn variable(&mut self, name: &str) -> Result<ValueType, ExprError> {
        let var = self
            .resolver
            .resolve(name)
            .ok_or_else(|| ExprError::UnknownVariable(name.to_string()))?;
        self.emit(Op::Load(var.slot), 0, 1)?;
        Ok(var.ty)
    }

    /// `if(c1, v1, c2, v2, ..., else)`: the first true condition picks its
    /// value; branches that are not taken are not evaluated.
    fn conditional(&mut self, args: &[Ast], ty: ValueType) -> Result<ValueType, ExprError> {
        let name = if ty == ValueType::Float { "if" } else { "ifb" };
        if args.len() < 3 || args.len().is_multiple_of(2) {
            return Err(ExprError::Arity {
                name: name.to_string(),
                expected: "an odd number, at least 3,".to_string(),
                found: args.len(),
            });
        }
        let (pairs, otherwise) = args.split_at(args.len() - 1);
        let mut to_end = Vec::new();
        for pair in pairs.chunks_exact(2) {
            self.expect(&pair[0], ValueType::Bool)?;
            let skip = self.ops.len();
            self.emit(Op::JumpIfFalse(0), 1, 0)?;
            self.expect(&pair[1], ty)?;
            to_end.push(self.ops.len());
            self.emit(Op::Jump(0), 0, 0)?;
            self.patch(skip);
            // Only one branch's value is on the stack at the join.
            self.depth -= 1;
        }
        self.expect(&otherwise[0], ty)?;
        to_end.into_iter().for_each(|at| self.patch(at));
        Ok(ty)
    }
}

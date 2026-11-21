// SPDX-License-Identifier: MIT OR Apache-2.0

//! The expression language of custom entity model animations.
//!
//! A `.jem` animation is a list of `"variable": "expression"` pairs, for
//! example `"head.ry": "head_yaw * 0.0174"`. Each expression is compiled once
//! (at resource reload) into a flat list of stack-machine instructions whose
//! variables are slot indices into one `f32` memory block, so that evaluating
//! it every frame allocates nothing and does no name lookup.
//!
//! The pipeline is [`lexer`] → [`parser`] (an AST with the operator
//! precedences) → [`compile`] (type checking and code generation) → [`vm`]
//! (evaluation). Values are `f32`; booleans are stored as `0.0` / `1.0` and
//! checked statically, so `if(1, 2, 3)` is a compile error, not a surprise.
//!
//! The grammar, the function list and the precedences follow the public
//! OptiFine CEM documentation (`cem_animation.txt`) and the shape of the
//! reference tree described in ADR-0088; the code is original.

mod compile;
mod lexer;
mod parser;
mod vm;

pub use compile::compile;
pub use vm::{EvalContext, SmoothCell};

/// The type of a value in an expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    /// A 32-bit float.
    Float,
    /// A boolean, stored as `0.0` or `1.0`.
    Bool,
}

/// Where a named variable lives in the memory block, and what type it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VarSlot {
    /// Index into the memory block.
    pub slot: u32,
    /// The type of the variable.
    pub ty: ValueType,
}

/// Looks the variable names of an expression up while it compiles.
pub trait Resolver {
    /// The slot of the variable `name`, or `None` when there is no such
    /// variable (which makes the expression, and the assignment that holds
    /// it, invalid).
    fn resolve(&mut self, name: &str) -> Option<VarSlot>;
}

/// Why an expression does not compile.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ExprError {
    /// A character that no token starts with.
    #[error("unexpected character {found:?} at offset {offset}")]
    UnexpectedChar {
        /// Byte offset in the source.
        offset: usize,
        /// The character.
        found: char,
    },
    /// A number that does not parse.
    #[error("malformed number {0:?}")]
    BadNumber(String),
    /// The tokens do not form an expression.
    #[error("syntax error: expected {expected} at offset {offset}")]
    Syntax {
        /// Byte offset of the token, or the source length at its end.
        offset: usize,
        /// What was expected there.
        expected: &'static str,
    },
    /// A variable the resolver does not know.
    #[error("unknown variable {0:?}")]
    UnknownVariable(String),
    /// A function that does not exist.
    #[error("unknown function {0:?}")]
    UnknownFunction(String),
    /// A function called with the wrong number of arguments.
    #[error("function {name} takes {expected} arguments, got {found}")]
    Arity {
        /// The function.
        name: String,
        /// A description of the accepted counts, such as `exactly 2`.
        expected: String,
        /// The count given.
        found: usize,
    },
    /// A float where a boolean is needed, or the reverse.
    #[error("type mismatch: expected {expected:?}, found {found:?}")]
    Type {
        /// The type the position needs.
        expected: ValueType,
        /// The type the expression has.
        found: ValueType,
    },
    /// The expression nests deeper than the evaluation stack.
    #[error("expression is too deep")]
    TooDeep,
}

/// A compiled expression: instructions, the type of its result and how many
/// `smooth` call sites it needs state for.
#[derive(Debug, Clone)]
pub struct Program {
    ops: Vec<vm::Op>,
    ty: ValueType,
    smooth_sites: u16,
}

impl Program {
    /// The type of the value the program produces.
    #[must_use]
    pub fn value_type(&self) -> ValueType {
        self.ty
    }

    /// How many `smooth(...)` call sites the program has; the caller
    /// provides that many [`SmoothCell`]s through [`EvalContext`].
    #[must_use]
    pub fn smooth_sites(&self) -> usize {
        usize::from(self.smooth_sites)
    }

    /// Evaluates the program against `memory`, without allocating.
    ///
    /// `memory` is indexed by the slots the [`Resolver`] handed out; a slot
    /// outside it reads as `0.0`.
    #[must_use]
    pub fn eval(&self, memory: &[f32], ctx: &mut EvalContext<'_>) -> f32 {
        vm::run(&self.ops, memory, ctx)
    }
}

#[cfg(test)]
mod tests;

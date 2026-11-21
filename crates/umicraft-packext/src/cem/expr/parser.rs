// SPDX-License-Identifier: MIT OR Apache-2.0

//! Builds the syntax tree, with the operator precedences of the reference
//! (public CEM documentation): unary `-` and `!` bind tightest (12), then
//! `* / %` (11), `+ -` (10), `< <= > >=` (8), `== !=` (7), `&&` (3), `||` (2).
//! Every binary operator is left-associative.

use super::lexer::{lex, Tok, Token};
use super::ExprError;

/// A binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BinOp {
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
    And,
    Or,
}

impl BinOp {
    fn of(tok: &Tok) -> Option<(Self, u8)> {
        Some(match tok {
            Tok::Plus => (Self::Add, 10),
            Tok::Minus => (Self::Sub, 10),
            Tok::Star => (Self::Mul, 11),
            Tok::Slash => (Self::Div, 11),
            Tok::Percent => (Self::Rem, 11),
            Tok::Lt => (Self::Lt, 8),
            Tok::Le => (Self::Le, 8),
            Tok::Gt => (Self::Gt, 8),
            Tok::Ge => (Self::Ge, 8),
            Tok::EqEq => (Self::Eq, 7),
            Tok::Ne => (Self::Ne, 7),
            Tok::AndAnd => (Self::And, 3),
            Tok::OrOr => (Self::Or, 2),
            _ => return None,
        })
    }
}

/// The syntax tree of an expression.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Ast {
    Num(f32),
    /// A name that is not followed by `(`: a variable or a constant.
    Name(String),
    Call(String, Vec<Ast>),
    Neg(Box<Ast>),
    Not(Box<Ast>),
    Bin(BinOp, Box<Ast>, Box<Ast>),
}

/// The deepest nesting accepted, so hostile input cannot overflow the stack
/// of the recursive parser.
const MAX_DEPTH: u32 = 64;

/// The most binary operators in one expression. A left-associative chain
/// nests as deep as it is long, so the code generator would recurse that far.
const MAX_OPERATORS: u32 = 256;

/// Parses `source` into a tree.
pub(super) fn parse(source: &str) -> Result<Ast, ExprError> {
    let tokens = lex(source)?;
    let mut parser = Parser {
        tokens: &tokens,
        pos: 0,
        end: source.len(),
        depth: 0,
        operators: 0,
    };
    let ast = parser.expr(0)?;
    match parser.peek() {
        None => Ok(ast),
        Some(_) => Err(parser.error("end of expression")),
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    end: usize,
    depth: u32,
    operators: u32,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos).map(|t| &t.kind)
    }

    fn error(&self, expected: &'static str) -> ExprError {
        ExprError::Syntax {
            offset: self.tokens.get(self.pos).map_or(self.end, |t| t.offset),
            expected,
        }
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        let hit = self.peek() == Some(tok);
        if hit {
            self.pos += 1;
        }
        hit
    }

    fn expect(&mut self, tok: &Tok, what: &'static str) -> Result<(), ExprError> {
        if self.eat(tok) {
            Ok(())
        } else {
            Err(self.error(what))
        }
    }

    /// Precedence climbing: parses operators of at least `min` precedence.
    fn expr(&mut self, min: u8) -> Result<Ast, ExprError> {
        self.enter()?;
        let mut lhs = self.unary()?;
        while let Some((op, prec)) = self.peek().and_then(BinOp::of).filter(|(_, p)| *p >= min) {
            self.pos += 1;
            self.operators += 1;
            if self.operators > MAX_OPERATORS {
                return Err(ExprError::TooDeep);
            }
            let rhs = self.expr(prec + 1)?;
            lhs = Ast::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        self.depth -= 1;
        Ok(lhs)
    }

    fn enter(&mut self) -> Result<(), ExprError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(ExprError::TooDeep);
        }
        Ok(())
    }

    fn unary(&mut self) -> Result<Ast, ExprError> {
        if self.eat(&Tok::Minus) {
            self.enter()?;
            let operand = self.unary()?;
            self.depth -= 1;
            return Ok(Ast::Neg(Box::new(operand)));
        }
        if self.eat(&Tok::Bang) {
            self.enter()?;
            let operand = self.unary()?;
            self.depth -= 1;
            return Ok(Ast::Not(Box::new(operand)));
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Ast, ExprError> {
        let Some(tok) = self.peek().cloned() else {
            return Err(self.error("a value"));
        };
        self.pos += 1;
        match tok {
            Tok::Num(n) => Ok(Ast::Num(n)),
            Tok::LParen => {
                let inner = self.expr(0)?;
                self.expect(&Tok::RParen, "`)`")?;
                Ok(inner)
            }
            Tok::Ident(name) if self.eat(&Tok::LParen) => {
                let args = self.arguments()?;
                Ok(Ast::Call(name, args))
            }
            Tok::Ident(name) => Ok(Ast::Name(name)),
            _ => {
                self.pos -= 1;
                Err(self.error("a value"))
            }
        }
    }

    /// The arguments after `(`, up to and including `)`.
    fn arguments(&mut self) -> Result<Vec<Ast>, ExprError> {
        let mut args = Vec::new();
        if self.eat(&Tok::RParen) {
            return Ok(args);
        }
        loop {
            args.push(self.expr(0)?);
            if self.eat(&Tok::RParen) {
                return Ok(args);
            }
            self.expect(&Tok::Comma, "`,` or `)`")?;
        }
    }
}

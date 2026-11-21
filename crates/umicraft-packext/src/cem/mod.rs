// SPDX-License-Identifier: MIT OR Apache-2.0

//! Custom entity models (CEM): `.jem` / `.jpm` files that replace or extend
//! the model of a mob, and the animation expressions that move their parts.
//!
//! - [`parse_jem`] reads a file into a [`CemModel`] (typed errors, no panics
//!   on any input);
//! - [`expr`] compiles the animation expressions;
//! - [`CemAnimator`] binds the expressions of a model to its parts and
//!   evaluates them each frame into [`PartPose`]s, without allocating.
//!
//! Turning the parts into geometry is the job of `umicraft-model`, and
//! choosing the model of an entity is the job of the client (ADR-0089).
//!
//! Sources: the public OptiFine CEM documentation (`cem_model.txt`,
//! `cem_part.txt`, `cem_animation.txt`) and ADR-0088; see
//! `docs/sources.md`.

pub mod expr;

mod animator;
mod model;
mod parse;

pub use animator::{CemAnimator, CemState, EntityInputs, PartPose, RootDelta};
pub use model::{BoxUv, CemAnimation, CemBox, CemEntry, CemModel, CemPart, PackPath};
pub use parse::{parse_jem, resolve_path, CemError, Loader};

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fighting: what the crosshair points at, and the packets an attack or an
//! interaction sends.
//!
//! The modules hold pure rules, tested against the MCP formulas; the client
//! maps their output onto its network commands (ADR-0029).
//!
//! | Module | Owns |
//! |---|---|
//! | [`hitbox`] | the bounding box of an entity and whether the ray hits it |
//! | [`target`] | the entity ray cast and its priority over blocks |
//! | [`interact`] | the packets of an entity click and what the client consumes |
//! | [`sprint`] | the sprint reset after a hit |

pub mod hitbox;
pub mod interact;
pub mod sprint;
pub mod target;

pub use hitbox::hitbox;
pub use interact::{attack_packets, interact_packets, EntityClick, InteractTick};
pub use sprint::{sprint_reset, SprintReset};
pub use target::{pick_target, EntityHit, Pointed, TargetQuery};

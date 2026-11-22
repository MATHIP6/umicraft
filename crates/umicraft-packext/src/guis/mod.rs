// SPDX-License-Identifier: MIT OR Apache-2.0

//! Custom GUIs: container screens whose textures a pack replaces (F6
//! phase 7).
//!
//! A pack puts `.properties` files under `gui/container/` (inside a
//! pack-feature folder). Each names a container, one or more replacement
//! textures, and optional conditions: the name of the container, the biome
//! and height where it stands, and container-specific facts (a large chest,
//! the levels of a beacon, a villager's profession).
//!
//! [`GuiSet::parse`] reads the rules, [`GuiSet::texture`] answers which
//! picture replaces a vanilla one for a [`GuiContext`]. The first rule whose
//! container and conditions match decides, even when it has no replacement for
//! the asked picture, as in OptiFine's `CustomGuis.getTexturePos`.
//!
//! Behaviour from OptiFine's `CustomGuis` and `CustomGuiProperties`
//! (decompiled 1.8.9 build, ADR-0088) and the public OptiFineDoc
//! `custom_guis.properties`.

mod rule;
mod set;

pub use rule::{Container, GuiContext, GuiRule, RuleError};
pub use set::GuiSet;

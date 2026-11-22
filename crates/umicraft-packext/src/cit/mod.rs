// SPDX-License-Identifier: MIT OR Apache-2.0

//! Custom Items (CIT): item textures chosen by rules (F6 phase 7).
//!
//! A pack puts `.properties` files under `cit/` (inside a pack-feature
//! folder). Each names the items it applies to, a replacement texture and,
//! optionally, conditions on damage, stack size, enchantments and NBT tags.
//! `cit.properties` holds the global `useGlint` switch.
//!
//! [`CitSet::parse`] reads every rule of the packs. [`CitSet::find`] returns
//! the rule for an item stack: rules are ordered by weight (highest first),
//! then by folder and name, and the first match wins. The module touches no
//! file and knows no item registry: names are resolved by the caller
//! ([`Names`]) and the stack is described by an [`ItemQuery`].
//!
//! Behaviour from OptiFine's `CustomItems`, `CustomItemProperties` and
//! `CustomItemsComparator` (decompiled 1.8.9 build, ADR-0088) and the public
//! OptiFineDoc `cit.properties` and `cit_single.properties`. Not read: the
//! `method`, `cap`, `fade` and `components.*` keys, and the types the 1.8
//! build does not have (`elytra`).

mod nbt;
mod rule;
mod set;

pub use nbt::NbtCondition;
pub use rule::{CitKind, CitRule, Damage, ItemQuery, Names};
pub use set::CitSet;

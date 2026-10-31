// SPDX-License-Identifier: MIT OR Apache-2.0

//! Neighbour-dependent block states (`Block.getActualState`, P7).
//!
//! A wire state is `id << 4 | metadata` and metadata has four bits, so it
//! cannot say that a fence touches its eastern neighbour or that a stair turns
//! a corner. The 1.8 client computes those properties from the surroundings
//! right before it picks the model; this module does the same.
//!
//! One submodule per block family computes [`Overrides`]: the blockstate
//! properties that differ from the metadata-derived key of
//! `umicraft_model::registry`. [`model_for`] merges them into that key and asks
//! [`BlockModelSet::model_by_key`] for the baked model. A block with no
//! override returns `None` and the mesher keeps the plain wire-state lookup, so
//! the common case costs one `match` on the block id.
//!
//! The allocation (one `String` for the merged key) only happens for blocks
//! that really depend on their neighbours.
//!
//! References (ADR-0001): each `Block*.getActualState` and the shared helpers
//! `BlockFence.canConnectTo` / `BlockPane.canPaneConnectToBlock`; see
//! `docs/sources.md`.

mod connect;
mod fence;
mod gate;
mod pane;
mod stairs;
#[cfg(test)]
mod test_support;
mod wall;

use std::collections::BTreeMap;

use umicraft_model::{definition, BlockModel, BlockModelSet};
use umicraft_world::BlockState;

use crate::BlockSource;

/// Properties of the registry key, by name (`facing` -> `north`).
pub(crate) type Props<'a> = BTreeMap<&'a str, &'a str>;

/// The most properties one block family overrides (a redstone wire has four
/// sides and a power level).
const MAX_OVERRIDES: usize = 5;

/// Blockstate properties computed from the neighbours.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Overrides {
    items: [(&'static str, &'static str); MAX_OVERRIDES],
    len: usize,
}

impl Overrides {
    /// Sets `name` to `value`.
    pub(crate) fn set(&mut self, name: &'static str, value: &'static str) {
        debug_assert!(self.len < MAX_OVERRIDES, "too many overridden properties");
        self.items[self.len] = (name, value);
        self.len += 1;
    }

    /// Sets a boolean property.
    pub(crate) fn set_flag(&mut self, name: &'static str, value: bool) {
        self.set(name, if value { "true" } else { "false" });
    }

    /// The overridden `(name, value)` pairs.
    pub(crate) fn as_slice(&self) -> &[(&'static str, &'static str)] {
        &self.items[..self.len]
    }
}

/// The baked model of `state` at `position` with its neighbour-dependent
/// properties applied.
///
/// `None` when the block does not depend on its neighbours, or when the
/// computed key names no variant: the caller then falls back to the wire-state
/// model, which is the default look of the block.
pub(crate) fn model_for<'a>(
    models: &'a BlockModelSet,
    source: &dyn BlockSource,
    position: [i32; 3],
    state: BlockState,
) -> Option<&'a BlockModel> {
    let family = family_of(state)?;
    let rule = definition(state.id())?.state(state.metadata())?;
    let base = parse_key(rule.variant);
    let context = Context {
        source,
        position,
        state,
        base: &base,
    };
    let overrides = (family.overrides)(&context)?;
    let key = merge_key(&base, overrides.as_slice());
    models.model_by_key(rule.blockstate, &key)
}

/// What a family needs to compute the properties of one block.
pub(crate) struct Context<'a> {
    /// The surroundings.
    pub(crate) source: &'a dyn BlockSource,
    /// World position of the block.
    pub(crate) position: [i32; 3],
    /// The wire state of the block.
    pub(crate) state: BlockState,
    /// Properties of the metadata-derived registry key.
    pub(crate) base: &'a Props<'a>,
}

/// One block family: which ids it owns and how it reads the neighbours.
struct Family {
    handles: fn(u16) -> bool,
    overrides: fn(&Context<'_>) -> Option<Overrides>,
}

/// Every family. Adding a block family is one module and one line here.
const FAMILIES: &[Family] = &[
    Family {
        handles: fence::handles,
        overrides: |c| Some(fence::overrides(c.source, c.position, c.state.id())),
    },
    Family {
        handles: pane::handles,
        overrides: |c| Some(pane::overrides(c.source, c.position)),
    },
    Family {
        handles: wall::handles,
        overrides: |c| Some(wall::overrides(c.source, c.position)),
    },
    Family {
        handles: gate::handles,
        overrides: |c| gate::overrides(c.source, c.position, c.base.get("facing")?),
    },
    Family {
        handles: stairs::handles,
        overrides: |c| Some(stairs::overrides(c.source, c.position, c.state)),
    },
];

/// The family owning `state`, if its look depends on the neighbours.
fn family_of(state: BlockState) -> Option<&'static Family> {
    FAMILIES.iter().find(|family| (family.handles)(state.id()))
}

/// Splits a registry key (`facing=north,half=top`) into its properties.
fn parse_key(key: &str) -> Props<'_> {
    key.split(',')
        .filter_map(|pair| pair.split_once('='))
        .collect()
}

/// Canonical key (properties sorted by name) of `base` with `overrides`
/// applied.
fn merge_key(base: &Props<'_>, overrides: &[(&str, &str)]) -> String {
    let mut merged = base.clone();
    merged.extend(overrides.iter().copied());
    let pairs: Vec<String> = merged
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    pairs.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_replace_the_base_and_the_key_is_sorted() {
        let base = parse_key("south=false,east=false,north=false,west=false");
        let key = merge_key(&base, &[("north", "true"), ("west", "true")]);
        assert_eq!(key, "east=false,north=true,south=false,west=true");
    }

    #[test]
    fn overrides_may_add_a_property_the_base_lacks() {
        let base = parse_key("facing=south,open=false");
        let key = merge_key(&base, &[("in_wall", "true")]);
        assert_eq!(key, "facing=south,in_wall=true,open=false");
    }

    #[test]
    fn a_block_without_a_family_has_no_override() {
        let models = BlockModelSet::default();
        let source = test_support::Grid::default();
        assert!(model_for(&models, &source, [0, 0, 0], BlockState::new(1, 0)).is_none());
    }
}

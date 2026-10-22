// SPDX-License-Identifier: MIT OR Apache-2.0

//! The 1.8 block-id to blockstate/model table.
//!
//! 1.8 has no registry file: the table is derived from the reference
//! implementation (`Block.java` registrations and each block's
//! `getStateFromMeta`) and cited in `docs/sources.md` (ADR-0001).

/// One metadata range that selects a blockstate document and variant key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateDef {
    /// Metadata mask; `0` matches every metadata value.
    pub mask: u8,
    /// Value the masked metadata must equal.
    pub value: u8,
    /// Blockstate document name without `.json`.
    pub blockstate: &'static str,
    /// Canonical variant key inside that document.
    pub variant: &'static str,
}

/// One 1.8 block id and the state mapping used by the model set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockDefinition {
    /// Numeric block id.
    pub id: u16,
    /// Registry name (also the blockstate document name for simple blocks).
    pub name: &'static str,
    /// Metadata-to-state rules, first match wins.
    pub states: &'static [StateDef],
}

impl BlockDefinition {
    /// State rule for a metadata value.
    #[must_use]
    pub fn state(&self, metadata: u8) -> Option<&StateDef> {
        self.states
            .iter()
            .find(|state| metadata & state.mask == state.value)
    }

    /// Blockstate document name for a metadata value.
    #[must_use]
    pub fn blockstate(&self, metadata: u8) -> Option<&'static str> {
        self.state(metadata).map(|state| state.blockstate)
    }
}

/// Definition for a numeric block id, if the id is known.
#[must_use]
pub fn definition(_id: u16) -> Option<&'static BlockDefinition> {
    None
}

/// Every known 1.8 block definition.
#[must_use]
pub fn definitions() -> &'static [BlockDefinition] {
    &[]
}

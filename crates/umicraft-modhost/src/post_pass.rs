// SPDX-License-Identifier: MIT OR Apache-2.0

//! Post-process passes a mod declares (F3.7): the slot that holds one, the registration
//! and parameter functions behind the `register_post_pass` and `post_set_param` host
//! functions, and the [`PostPassView`] the client reads.
//!
//! Passes are collected with the other declarations and sealed with them, so a mod
//! whose init failed contributes none. Only the parameters change afterwards.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use umicraft_modapi_sys::UmiStatus;

use crate::context::ModContext;
use crate::error::DeclareError;
use crate::manifest::ModId;
use crate::post::validate;
use crate::ui::validate::{check_key, check_text};
use crate::ui::MAX_LABEL_BYTES;

/// Passes one mod may register.
pub const MAX_POST_PASSES: usize = 8;
/// Number of `vec4` parameters of a pass.
const PARAM_VECS: usize = 4;

/// A registered pass: its composed shader and its live parameters.
///
/// The parameters are 16 `AtomicU32` holding `f32` bits, written by the mod from any
/// thread and read by the render thread every frame. Relaxed atomics keep both sides
/// wait-free; a vec4 torn between two writes shows for one frame at worst, which is
/// invisible for a value the mod updates continuously anyway.
#[derive(Debug)]
pub(crate) struct PostPassSlot {
    key: String,
    label: String,
    wgsl: String,
    params: [AtomicU32; PARAM_VECS * 4],
    refused: AtomicBool,
}

impl PostPassSlot {
    /// Validates the key, label and shader and builds the slot.
    pub(crate) fn new(key: &str, label: &str, source: &str) -> Result<Self, DeclareError> {
        check_key("post pass key", key)?;
        check_text("post pass label", label, MAX_LABEL_BYTES)?;
        let wgsl = validate(source).map_err(|source| DeclareError::PostShader {
            key: key.to_owned(),
            source,
        })?;
        Ok(Self {
            key: key.to_owned(),
            label: label.to_owned(),
            wgsl,
            params: std::array::from_fn(|_| AtomicU32::new(0)),
            refused: AtomicBool::new(false),
        })
    }

    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    fn set(&self, index: usize, value: [f32; 4]) {
        for (cell, component) in self.params[index * 4..][..4].iter().zip(value) {
            cell.store(component.to_bits(), Ordering::Relaxed);
        }
    }

    fn params(&self) -> [[f32; 4]; PARAM_VECS] {
        std::array::from_fn(|vec| {
            std::array::from_fn(|lane| {
                f32::from_bits(self.params[vec * 4 + lane].load(Ordering::Relaxed))
            })
        })
    }
}

/// A pass as the renderer needs it.
#[derive(Debug, Clone, Copy)]
pub struct PostPassView<'a> {
    /// The mod that registered the pass.
    pub mod_id: &'a ModId,
    /// The pass's key, unique within the mod.
    pub key: &'a str,
    /// Label for logs and the mods menu.
    pub label: &'a str,
    /// The composed WGSL (prelude plus the mod's source).
    pub wgsl: &'a str,
    /// Whether the pass should run: its mod is enabled and the GPU did not refuse it.
    pub enabled: bool,
    slot: &'a PostPassSlot,
}

impl PostPassView<'_> {
    /// The current `umi.params`, as the mod last set them (zero until set).
    pub fn params(&self) -> [[f32; 4]; 4] {
        self.slot.params()
    }
}

impl ModContext {
    /// Declares a post-process pass; the shader is validated against the contract.
    ///
    /// # Errors
    ///
    /// [`DeclareError::Late`] after init, [`DeclareError::Duplicate`] for a repeated
    /// key, [`DeclareError::PostShader`] for a shader that breaks the contract, and
    /// the key and label checks.
    pub fn register_post_pass(
        &self,
        key: &str,
        label: &str,
        source: &str,
    ) -> Result<(), DeclareError> {
        self.ui
            .register(|declared| declared.add_post_pass(PostPassSlot::new(key, label, source)?))
    }

    /// Sets `params[index]` of pass `key` from any thread. `INVALID_ARGUMENT` for an
    /// unknown key (including before init ends), an index above 3 or a non-finite
    /// value. Allowed while the mod is switched off: the value is just stored.
    pub fn post_set_param(&self, key: &str, index: u32, value: [f32; 4]) -> UmiStatus {
        let slot = self.ui.declared().post_pass(key);
        match slot {
            Some(slot) if (index as usize) < PARAM_VECS && value.iter().all(|v| v.is_finite()) => {
                slot.set(index as usize, value);
                UmiStatus::OK
            }
            _ => UmiStatus::INVALID_ARGUMENT,
        }
    }

    /// The mod's passes in registration order.
    pub(crate) fn post_passes(&self) -> impl Iterator<Item = PostPassView<'_>> {
        let enabled = self.is_enabled();
        self.ui
            .declared()
            .post_passes()
            .iter()
            .map(move |slot| PostPassView {
                mod_id: self.id(),
                key: &slot.key,
                label: &slot.label,
                wgsl: &slot.wgsl,
                enabled: enabled && !slot.refused.load(Ordering::Relaxed),
                slot,
            })
    }

    /// Marks pass `key` refused for good; `false` for an unknown key. Logs once.
    pub(crate) fn refuse_post_pass(&self, key: &str, reason: &str) -> bool {
        let Some(slot) = self.ui.declared().post_pass(key) else {
            return false;
        };
        if !slot.refused.swap(true, Ordering::AcqRel) {
            let mod_id = self.id();
            tracing::warn!(%mod_id, key, reason, "post pass refused by the GPU, never drawn");
        }
        true
    }
}

#[cfg(test)]
mod tests;

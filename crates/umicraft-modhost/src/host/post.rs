// SPDX-License-Identifier: MIT OR Apache-2.0

//! The post-process side of [`ModHost`] (F3.7): what the client hands to the renderer.

use super::ModHost;
use crate::manifest::ModId;
use crate::post_pass::PostPassView;

impl ModHost {
    /// Every post-process pass, in load order then registration order. Passes of
    /// switched-off or failed mods are listed with `enabled == false`, so the list
    /// (and the renderer's compiled passes) stays stable when the player toggles a mod.
    pub fn post_passes(&self) -> impl Iterator<Item = PostPassView<'_>> {
        self.mods.iter().flat_map(|m| m.context.post_passes())
    }

    /// Records that the GPU refused pass `key` of mod `mod_id`: it is never enabled
    /// again, the mod stays enabled, and the reason is logged once. Unknown ids and
    /// keys are ignored.
    pub fn refuse_post_pass(&self, mod_id: &ModId, key: &str, reason: &str) {
        if let Some(context) = self.context(mod_id) {
            context.refuse_post_pass(key, reason);
        }
    }
}

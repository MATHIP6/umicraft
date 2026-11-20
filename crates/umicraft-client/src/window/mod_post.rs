// SPDX-License-Identifier: MIT OR Apache-2.0

//! The post-process passes of the external mods on the render thread (F3.7,
//! ADR-0086).
//!
//! The passes are compiled once, when the renderer is created: the mods register
//! them during their `init`, so the list never grows afterwards. Every frame the
//! client only refreshes which passes run and their parameters, into a buffer it
//! keeps, so a frame allocates nothing.

use umicraft_render::{PostPassFrame, PostPassSource, Renderer};

use super::App;

impl App {
    /// Hands the mods' passes to a new renderer; a pass the GPU refuses is
    /// switched off for good (its mod keeps running) and logged.
    pub(super) fn install_post_passes(&mut self, renderer: &mut Renderer) {
        let Some(host) = self.external_mods.as_ref().and_then(|mods| mods.host()) else {
            return;
        };
        let sources: Vec<PostPassSource> = host
            .post_passes()
            .map(|pass| PostPassSource {
                label: format!("{}:{}", pass.mod_id, pass.key),
                wgsl: pass.wgsl.to_owned(),
            })
            .collect();
        if sources.is_empty() {
            return;
        }
        let ids: Vec<_> = host
            .post_passes()
            .map(|pass| (pass.mod_id.clone(), pass.key.to_owned()))
            .collect();
        for ((mod_id, key), outcome) in ids.iter().zip(renderer.set_post_passes(&sources)) {
            if let Err(reason) = outcome {
                host.refuse_post_pass(mod_id, key, &reason);
            }
        }
        self.post_frames = Vec::with_capacity(sources.len());
    }

    /// Tells the renderer which passes run this frame and with which parameters.
    pub(super) fn update_post_passes(&mut self, renderer: &mut Renderer) {
        let Some(host) = self.external_mods.as_ref().and_then(|mods| mods.host()) else {
            return;
        };
        if self.post_frames.capacity() == 0 {
            return;
        }
        self.post_frames.clear();
        self.post_frames
            .extend(host.post_passes().map(|pass| PostPassFrame {
                enabled: pass.enabled,
                params: pass.params(),
            }));
        renderer.update_post(&self.post_frames);
    }
}

#[cfg(test)]
mod tests {
    /// The mod API composes passes with its prelude and the renderer binds the
    /// resources of its own: they must be the same contract.
    #[test]
    fn the_mod_api_and_the_renderer_share_one_prelude() {
        assert_eq!(
            umicraft_modhost::PRELUDE.trim(),
            umicraft_render::POST_PRELUDE.trim()
        );
    }
}

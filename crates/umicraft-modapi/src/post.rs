// SPDX-License-Identifier: MIT OR Apache-2.0

//! Post-process passes over the picture of the world.

use umicraft_modapi_sys::UmiStatus;

use crate::key::InlineKey;
use crate::{host, ModError};

/// A post-process pass registered with [`InitCx::post_pass`](crate::InitCx::post_pass).
///
/// The host runs the mod's fragment shader over the finished picture of the
/// world (the HUD and menus stay sharp). The mod steers it by writing
/// `umi.params[0..4]`, four `vec4<f32>` that read zero until set.
///
/// `Copy + Send + Sync` so a closure can own one and any thread can call
/// [`set_param`](Self::set_param). It stores its key inline, since the host needs
/// the key again on each call.
///
/// The shader contract (entry point `fs_main`, the prelude names `umi_scene`,
/// `umi_sampler`, `umi.params` and `UmiVertexOut`, the limits) is in
/// `docs/modding/post.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostPass {
    key: InlineKey,
}

impl PostPass {
    /// Remembers `key`, or `None` when it is too long to be a valid key.
    pub(crate) fn new(key: &str) -> Option<Self> {
        InlineKey::new(key).map(|key| Self { key })
    }

    pub(crate) fn key(&self) -> &str {
        self.key.as_str()
    }

    /// Sets `umi.params[index]` to `value`; the next frame uses it.
    ///
    /// Callable from any thread, at any time after `init`. Setting a value
    /// every tick is cheap.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] with `INVALID_ARGUMENT` for an `index`
    /// above 3, or `UNSUPPORTED` before `init`.
    pub fn set_param(&self, index: u32, value: [f32; 4]) -> Result<(), ModError> {
        let [x, y, z, w] = value;
        host::call(|host, me| (host.post_set_param)(me, host::utf8(self.key()), index, x, y, z, w))
            .and_then(ModError::check)
    }
}

/// The refusal for a key longer than the host allows, without asking it.
pub(crate) fn key_too_long() -> ModError {
    ModError::HostRefused(UmiStatus::INVALID_ARGUMENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_round_trips_up_to_the_limit() {
        assert_eq!(
            PostPass::new("sepia").map(|p| p.key().to_owned()),
            Some("sepia".to_owned())
        );
        assert!(PostPass::new(&"a".repeat(65)).is_none());
    }

    /// This test binary never runs `init`, so there is no host.
    #[test]
    fn set_param_reports_unsupported_without_a_host() {
        let pass = PostPass::new("sepia").unwrap();
        assert_eq!(
            pass.set_param(0, [0.0; 4]),
            Err(ModError::HostRefused(UmiStatus::UNSUPPORTED))
        );
    }
}

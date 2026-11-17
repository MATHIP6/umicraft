// SPDX-License-Identifier: MIT OR Apache-2.0

//! Reading frames back: offscreen pixels and screenshots of the window.

use super::Image;
use super::Renderer;
use crate::capture;
use crate::error::RenderError;

impl Renderer {
    /// Reads the offscreen colour target as tightly packed RGBA8.
    ///
    /// Call [`Renderer::render`] or [`Renderer::render_with_ui`] first. The
    /// row padding required by WebGPU is removed before returning.
    pub fn read_pixels(&self) -> Result<Image, RenderError> {
        let Some(target) = self.offscreen.as_ref() else {
            return Err(RenderError::NotOffscreen);
        };
        capture::read_texture(
            &self.device,
            &self.queue,
            &target.texture,
            (self.width, self.height),
            false,
        )
    }

    /// Asks the windowed renderer to keep the next frame it presents; fetch
    /// it with [`Renderer::take_capture`]. The surface must allow copies
    /// (see [`Renderer::can_capture`]).
    pub fn request_capture(&mut self) {
        self.capture_requested = true;
    }

    /// Whether the window surface can be read back (`COPY_SRC` usage).
    #[must_use]
    pub fn can_capture(&self) -> bool {
        self.surface_config
            .as_ref()
            .is_some_and(|config| config.usage.contains(wgpu::TextureUsages::COPY_SRC))
    }

    /// The frame kept after [`Renderer::request_capture`], once it was
    /// presented; `None` while nothing is waiting.
    ///
    /// # Errors
    /// [`RenderError::Readback`] when the copy of the frame failed.
    pub fn take_capture(&mut self) -> Option<Result<Image, RenderError>> {
        self.captured.take()
    }

    /// Copies the frame about to be presented when a capture was requested.
    pub(super) fn capture_frame(&mut self, texture: &wgpu::Texture) {
        if !std::mem::take(&mut self.capture_requested) || !self.can_capture() {
            return;
        }
        let Some(format) = self.surface_config.as_ref().map(|config| config.format) else {
            return;
        };
        let blue_first = matches!(
            format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        );
        self.captured = Some(capture::read_texture(
            &self.device,
            &self.queue,
            texture,
            (self.width, self.height),
            blue_first,
        ));
    }
}

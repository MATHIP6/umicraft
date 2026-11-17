// SPDX-License-Identifier: MIT OR Apache-2.0

//! The adapter, the device and the presentation surface.

use super::Renderer;
use super::DEPTH_FORMAT;
use crate::adapter::AdapterSummary;
use crate::error::RenderError;
use crate::gpu_timer::{self};

/// Frames the presentation queue may hold (F2-5e). When vsync is active, 1 keeps
/// input latency minimal; when unconstrained without vsync, 2 lets the CPU pipeline
/// ahead of the presentation engine to maximise extreme framerates.
const FRAME_LATENCY_VSYNC: u32 = 1;

pub(super) const FRAME_LATENCY_UNCONSTRAINED: u32 = 2;

/// Offscreen colour target and the texture retained for readback.
pub(super) struct OffscreenTarget {
    pub(super) texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
}

/// The present mode of the "Use VSync" option: `Auto*` picks the best mode
/// the platform supports for the wish (Fifo for vsync, Immediate or Mailbox
/// without).
pub(super) fn present_mode(vsync: bool) -> wgpu::PresentMode {
    if vsync {
        wgpu::PresentMode::AutoVsync
    } else {
        wgpu::PresentMode::AutoNoVsync
    }
}

pub(super) fn request_adapter(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'static>>,
) -> Result<wgpu::Adapter, RenderError> {
    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: surface,
        apply_limit_buckets: false,
    }))
    .map_err(|error| RenderError::NoAdapter(format!("{error:?}")))
}

pub(super) fn request_device(
    adapter: &wgpu::Adapter,
) -> Result<(wgpu::Device, wgpu::Queue), RenderError> {
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("umicraft device"),
        // Timestamps are optional: without them F3 shows "n/a".
        required_features: if adapter.features().contains(gpu_timer::REQUIRED_FEATURES) {
            gpu_timer::REQUIRED_FEATURES
        } else {
            wgpu::Features::empty()
        },
        required_limits: adapter.limits(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    }))
    .map_err(|error| RenderError::NoDevice(format!("{error:?}")))
}

pub(super) fn create_depth(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        // Sampled by the motion blur, which works out the motion from the depth.
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

pub(super) fn create_offscreen(device: &wgpu::Device, width: u32, height: u32) -> OffscreenTarget {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen colour"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    OffscreenTarget { texture, view }
}

impl Renderer {
    /// The GPU adapter the renderer runs on, for the F3 screen.
    #[must_use]
    pub fn adapter(&self) -> &AdapterSummary {
        &self.adapter
    }

    /// Resizes the swap chain (windowed) or offscreen colour target.
    pub fn resize(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if width == self.width && height == self.height {
            return;
        }
        self.width = width;
        self.height = height;
        let device = &self.device;
        if let Some(config) = self.surface_config.as_mut() {
            config.width = width;
            config.height = height;
            if let Some(surface) = self.surface.as_ref() {
                surface.configure(device, config);
            }
        }
        self.depth_view = create_depth(&self.device, width, height);
        self.scene_blur
            .resize(&self.device, &self.queue, (width, height));
        self.motion_blur.invalidate();
        self.ui_pass.resize(width, height);
        if self.offscreen.is_some() {
            self.offscreen = Some(create_offscreen(&self.device, width, height));
        }
    }

    /// Turns vertical sync on or off (`GameSettings.enableVsync`). The
    /// surface is only reconfigured when the mode changes, so the window can
    /// call this every frame. Offscreen renderers ignore it.
    pub fn set_vsync(&mut self, vsync: bool) {
        let mode = present_mode(vsync);
        let latency = if vsync {
            FRAME_LATENCY_VSYNC
        } else {
            FRAME_LATENCY_UNCONSTRAINED
        };
        let (Some(config), Some(surface)) = (self.surface_config.as_mut(), self.surface.as_ref())
        else {
            return;
        };
        if config.present_mode != mode || config.desired_maximum_frame_latency != latency {
            config.present_mode = mode;
            config.desired_maximum_frame_latency = latency;
            surface.configure(&self.device, config);
        }
    }

    /// Current target size in physical pixels.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Access to the underlying wgpu device.
    #[must_use]
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Access to the underlying wgpu queue.
    #[must_use]
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub(super) fn reconfigure_surface(&mut self) -> Result<(), RenderError> {
        self.frame_pacing.reconfigure_count += 1;
        let config = self
            .surface_config
            .as_ref()
            .ok_or_else(|| RenderError::Surface("renderer has no surface config".into()))?;
        let surface = self
            .surface
            .as_ref()
            .ok_or_else(|| RenderError::Surface("renderer has no surface".into()))?;
        surface.configure(&self.device, config);
        Ok(())
    }

    pub(super) fn recreate_surface(&mut self) -> Result<(), RenderError> {
        let window = self
            .window
            .clone()
            .ok_or_else(|| RenderError::Surface("renderer has no window".into()))?;
        let config = self
            .surface_config
            .as_ref()
            .ok_or_else(|| RenderError::Surface("renderer has no surface config".into()))?;
        let surface = self
            .instance
            .create_surface(window)
            .map_err(|error| RenderError::Surface(format!("{error:?}")))?;
        surface.configure(&self.device, config);
        self.surface = Some(surface);
        Ok(())
    }
}

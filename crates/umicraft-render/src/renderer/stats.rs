// SPDX-License-Identifier: MIT OR Apache-2.0

//! The counters of the debug screens: GPU timings, draw calls, memory and
//! frame pacing.

use umicraft_mesh::TEXTURE_LAYERS;

use super::Renderer;
use crate::frame_pacing::{FramePacingStats, SurfaceInfo};
use crate::gpu_timer::{DrawCallStats, GpuTimer, GpuTimings, VramStats};
use crate::texture::TEXTURE_SIZE;

impl Renderer {
    /// The GPU time of the newest finished frame per group of draws, two or
    /// three frames old. `None` when the adapter has no timestamp queries or
    /// no frame has come back yet.
    #[must_use]
    pub fn gpu_timings(&self) -> Option<GpuTimings> {
        self.gpu_timer.as_ref().and_then(GpuTimer::latest)
    }

    /// Whether the adapter can time the GPU (F3 shows "n/a" otherwise).
    #[must_use]
    pub fn gpu_timing_supported(&self) -> bool {
        self.gpu_timer.is_some()
    }

    /// Returns the draw call breakdown recorded during the last rendered frame.
    #[must_use]
    pub fn draw_calls(&self) -> DrawCallStats {
        self.last_draw_calls.get()
    }

    /// Number of active weather vertices rendered in the last frame.
    #[must_use]
    pub fn weather_vertex_count(&self) -> usize {
        self.weather.vertex_count()
    }

    /// Returns the presentation duration of the last rendered frame in microseconds.
    #[must_use]
    pub fn presentation_time_us(&self) -> u32 {
        self.last_present_us.get()
    }

    /// Detailed configuration of the current WGPU presentation surface.
    #[must_use]
    pub fn surface_info(&self) -> Option<SurfaceInfo> {
        let config = self.surface_config.as_ref()?;
        Some(SurfaceInfo {
            backend: self.adapter.backend_type,
            backend_name: self.adapter.backend.clone(),
            adapter_name: self.adapter.name.clone(),
            adapter_vendor: self.adapter.vendor.clone(),
            format: config.format,
            present_mode: config.present_mode,
            desired_maximum_frame_latency: config.desired_maximum_frame_latency,
            width: config.width,
            height: config.height,
            usages: config.usage,
        })
    }

    /// Rolling frame presentation pacing and swapchain metrics.
    #[must_use]
    pub fn frame_pacing_stats(&self) -> FramePacingStats {
        self.frame_pacing.stats()
    }

    /// Estimates the GPU memory (VRAM) currently held by this renderer's
    /// chunk buffers, dynamic vertex/uniform buffers and textures.
    #[must_use]
    pub fn vram_stats(&self) -> VramStats {
        let mut chunk_buffers = self.quad_indices.byte_size();
        for slot in self.mesh_slots.iter().flatten() {
            let mesh = &slot.mesh;
            if let Some(opaque) = &mesh.opaque {
                chunk_buffers += opaque.vertex_buffer.size();
            }
            if let Some(transparent) = &mesh.transparent {
                chunk_buffers += transparent.vertex_buffer.size();
            }
        }
        let dynamic_buffers = self.overlay_capacity
            + self.camera_buffer.size()
            + self.local_camera_buffer.size()
            + self.hand_camera_buffer.size()
            + self.entity_batches.capacity_bytes()
            + self.additive_batches.capacity_bytes()
            + self.glint_batches.capacity_bytes()
            + self.ghost_batches.capacity_bytes()
            + self.shadow_batches.capacity_bytes()
            + self.hand.capacity_bytes();
        let pixel_count = u64::from(self.width) * u64::from(self.height);
        // Depth buffer (Depth32Float = 4B/px) + scene blur target (4B/px) + optional offscreen (4B/px).
        let target_bytes = pixel_count * if self.offscreen.is_some() { 12 } else { 8 };
        let atlas_bytes = match &self.official_blocks {
            Some((tex, w, h)) => {
                let base =
                    u64::from(*w) * u64::from(*h) * 4 * u64::from(tex.depth_or_array_layers());
                // Mip chain adds at most ~1/3 over base.
                if tex.mip_level_count() > 1 {
                    base + base / 3
                } else {
                    base
                }
            }
            None => {
                u64::from(TEXTURE_SIZE) * u64::from(TEXTURE_SIZE) * 4 * u64::from(TEXTURE_LAYERS)
            }
        };
        let font_bytes = u64::from(self.ui_atlas.width) * u64::from(self.ui_atlas.height);
        let icon_bytes =
            u64::from(crate::items::ICON_ATLAS_SIZE) * u64::from(crate::items::ICON_ATLAS_SIZE) * 8; // colour + depth
        VramStats {
            chunk_buffers,
            dynamic_buffers,
            textures: target_bytes + atlas_bytes + font_bytes + icon_bytes,
        }
    }
}

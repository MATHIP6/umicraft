// SPDX-License-Identifier: MIT OR Apache-2.0

//! One frame: the entry points, the camera uniforms, the overlay upload
//! and the submission to a window or an offscreen target.

use umicraft_ui::DrawList;

use super::Renderer;
use super::{CameraUniform, HAND_FOG_FREE, OVERLAY_MIN_BYTES};
use crate::camera::Camera;
use crate::error::RenderError;
use crate::items::IconProvider;
use crate::lightmap::{self, LightmapParams};
use crate::overlay::Overlay;
use crate::sky::SkyState;

impl Renderer {
    /// Draws one frame with the M3 HUD overlay only.
    ///
    /// Any UI geometry from a previous [`Renderer::render_with_ui`] call is
    /// discarded, so the two entry points can be interleaved safely. Skips
    /// presentation on timeout or occlusion and reconfigures the surface when
    /// the backend requests it.
    pub fn render(&mut self, camera: &Camera, overlay: &Overlay) -> Result<(), RenderError> {
        self.ui_pass.clear();
        self.render_frame(camera, overlay)
    }

    /// Draws one frame: terrain, then the `overlay` HUD, then `draw_list`.
    ///
    /// The draw list is built by the caller (on this same render thread)
    /// before the call; the renderer only converts it to vertices and submits
    /// them. It uses the atlas most recently passed to
    /// [`Renderer::set_font_atlas`], or the built-in fallback atlas when none
    /// was set. The UI is drawn above the HUD and shares the overlay pipeline,
    /// so no extra pass or thread is introduced.
    ///
    /// `Item` commands draw from the icon atlas; an icon not rendered yet is
    /// built by `icons` and rendered first (once per key), so a steady-state
    /// frame renders no item geometry.
    pub fn render_with_ui(
        &mut self,
        camera: &Camera,
        overlay: &Overlay,
        draw_list: &DrawList,
        icons: &mut dyn IconProvider,
    ) -> Result<(), RenderError> {
        self.prepare_icons(draw_list, icons);
        let atlas = &self.icon_atlas;
        self.ui_pass
            .rebuild_with_icons(draw_list, &self.ui_atlas, &|key| atlas.uv(key));
        self.render_frame(camera, overlay)
    }

    fn render_frame(&mut self, camera: &Camera, overlay: &Overlay) -> Result<(), RenderError> {
        umicraft_core::profile_scope!("render_frame");
        // One SkyState per frame: the clear colour, the fog uniform and the
        // sky pass must agree even if W3's model changes between calls.
        let sky_state = SkyState::at_looking(self.environment, camera.forward().to_array());
        let flicker = self.torch_flicker.at(self.cloud_ticks as u64);
        self.write_camera(camera, &sky_state, flicker);
        self.sky
            .update(&self.queue, camera, self.environment, &sky_state);
        self.clouds.update(
            &self.queue,
            camera,
            self.environment,
            &sky_state,
            self.cloud_ticks,
        );
        self.weather
            .update(&self.queue, camera, self.environment, self.cloud_ticks);
        self.particles
            .update(&self.queue, camera, &self.particle_sprites);
        self.screen
            .update(&self.queue, &self.screen_overlays, camera);
        // The reference's reach is `renderDistanceChunks * 16`.
        self.border.update(
            &self.queue,
            camera,
            f64::from(self.environment.view_distance) * 16.0,
        );
        self.upload_overlay(overlay);
        self.prepare_visible_chunks(camera);
        // The motion blur of this frame, unless a menu already blurs the game.
        self.motion_blur.prepare(
            &self.device,
            &self.queue,
            camera,
            (self.scene_blur.scene_view(), &self.depth_view),
            (self.width, self.height),
            !self.background_blur,
        );
        if self.surface.is_some() {
            self.render_window(overlay, &sky_state)
        } else {
            self.render_offscreen(overlay, &sky_state)
        }
    }

    fn write_camera(&mut self, camera: &Camera, sky: &SkyState, flicker: f32) {
        let [fog_start, fog_end, fog_density] = sky.fog.world.uniform();
        let origin = camera.render_origin();
        let uniform = CameraUniform {
            view_proj: camera.view_projection_from(origin).to_cols_array_2d(),
            camera_pos: camera.relative_eye().to_array(),
            sun_brightness: sky.sun_brightness,
            fog_color: sky.fog.color,
            fog_start,
            fog_end,
            gamma: self.environment.gamma,
            fog_density,
            _pad: 0.0,
            origin: origin.as_vec3().to_array(),
            _pad_origin: 0.0,
            lightmap: if self.fullbright {
                lightmap::unlit()
            } else {
                lightmap::colors(&LightmapParams::of(
                    &self.environment,
                    sky.sun_brightness,
                    flicker,
                ))
            },
        };
        let uniform_bytes = bytemuck::bytes_of(&uniform);
        if self
            .last_camera_uniform
            .as_ref()
            .is_none_or(|cached| bytemuck::bytes_of(cached) != uniform_bytes)
        {
            self.queue
                .write_buffer(&self.camera_buffer, 0, uniform_bytes);
            self.last_camera_uniform = Some(uniform);
        }

        // Same matrix and eye: the vertices of this one are already relative.
        let local = CameraUniform {
            origin: [0.0; 3],
            ..uniform
        };
        let local_bytes = bytemuck::bytes_of(&local);
        if self
            .last_local_uniform
            .as_ref()
            .is_none_or(|cached| bytemuck::bytes_of(cached) != local_bytes)
        {
            self.queue
                .write_buffer(&self.local_camera_buffer, 0, local_bytes);
            self.last_local_uniform = Some(local);
        }

        // The hand lives in camera space, far from any fog.
        let hand_uniform = CameraUniform {
            view_proj: camera.hand_projection().to_cols_array_2d(),
            camera_pos: [0.0; 3],
            fog_start: HAND_FOG_FREE,
            fog_end: HAND_FOG_FREE * 2.0,
            fog_density: 0.0,
            origin: [0.0; 3],
            ..uniform
        };
        let hand_bytes = bytemuck::bytes_of(&hand_uniform);
        if self
            .last_hand_uniform
            .as_ref()
            .is_none_or(|cached| bytemuck::bytes_of(cached) != hand_bytes)
        {
            self.queue
                .write_buffer(&self.hand_camera_buffer, 0, hand_bytes);
            self.last_hand_uniform = Some(hand_uniform);
        }
    }

    fn upload_overlay(&mut self, overlay: &Overlay) {
        let hud_bytes: &[u8] = bytemuck::cast_slice(overlay.vertices());
        let hud_len = hud_bytes.len();
        let ui_bytes: &[u8] = bytemuck::cast_slice(self.ui_pass.vertices());
        let ui_len = ui_bytes.len();
        let total = hud_len + ui_len;
        if total == 0 {
            return;
        }
        if total as u64 > self.overlay_capacity {
            let capacity = (total as u64).next_power_of_two().max(OVERLAY_MIN_BYTES);
            self.overlay_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("overlay vertices"),
                size: capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.overlay_capacity = capacity;
        }
        if let Some(size) = wgpu::BufferSize::new(total as u64) {
            if let Some(mut view) = self.queue.write_buffer_with(&self.overlay_buffer, 0, size) {
                if !hud_bytes.is_empty() {
                    view.slice(..hud_len).copy_from_slice(hud_bytes);
                }
                if !ui_bytes.is_empty() {
                    view.slice(hud_len..total).copy_from_slice(ui_bytes);
                }
            } else {
                if !hud_bytes.is_empty() {
                    self.queue.write_buffer(&self.overlay_buffer, 0, hud_bytes);
                }
                if !ui_bytes.is_empty() {
                    self.queue
                        .write_buffer(&self.overlay_buffer, hud_len as u64, ui_bytes);
                }
            }
        }
    }

    fn render_window(
        &mut self,
        overlay: &Overlay,
        sky_state: &SkyState,
    ) -> Result<(), RenderError> {
        umicraft_core::profile_scope!("render_window");
        let acquire_start = std::time::Instant::now();
        let frame = {
            let surface = self
                .surface
                .as_ref()
                .ok_or_else(|| RenderError::Surface("renderer has no surface".into()))?;
            surface.get_current_texture()
        };
        let acquire_us = acquire_start
            .elapsed()
            .as_micros()
            .min(u128::from(u32::MAX)) as u32;
        // A suboptimal texture (X11 reports it for a surface that no longer
        // matches the window) is still drawn and presented; the surface is
        // reconfigured after the present, once no texture of it is alive.
        let suboptimal = matches!(frame, wgpu::CurrentSurfaceTexture::Suboptimal(_));
        match frame {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                let encode_start = std::time::Instant::now();
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("frame encoder"),
                        });
                self.begin_gpu_timing();
                self.draw(&view, overlay, &mut encoder, sky_state);
                let encode_us = encode_start.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;

                let submit_start = std::time::Instant::now();
                self.submit_timed(encoder);
                let submit_us = submit_start.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;

                self.capture_frame(&frame.texture);
                if let Some(window) = &self.window {
                    window.pre_present_notify();
                }
                let present_start = std::time::Instant::now();
                self.queue.present(frame);
                let present_now = std::time::Instant::now();
                let present_us = present_now
                    .saturating_duration_since(present_start)
                    .as_micros()
                    .min(u128::from(u32::MAX)) as u32;
                self.last_present_us.set(present_us);

                self.frame_pacing.record_present(
                    present_now,
                    acquire_us,
                    encode_us,
                    submit_us,
                    present_us,
                );
                if suboptimal {
                    self.frame_pacing.suboptimal_count += 1;
                    self.reconfigure_surface()?;
                }
                Ok(())
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.frame_pacing.outdated_count += 1;
                self.reconfigure_surface()?;
                Ok(())
            }
            wgpu::CurrentSurfaceTexture::Timeout => {
                self.frame_pacing.timeout_count += 1;
                Ok(())
            }
            wgpu::CurrentSurfaceTexture::Occluded => {
                self.frame_pacing.occluded_count += 1;
                Ok(())
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.frame_pacing.lost_count += 1;
                self.recreate_surface()?;
                Ok(())
            }
            wgpu::CurrentSurfaceTexture::Validation => Err(RenderError::SurfaceLost),
        }
    }

    fn render_offscreen(
        &mut self,
        overlay: &Overlay,
        sky_state: &SkyState,
    ) -> Result<(), RenderError> {
        self.begin_gpu_timing();
        let Some(target) = self.offscreen.as_ref() else {
            return Err(RenderError::NotOffscreen);
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("offscreen encoder"),
            });
        self.draw(&target.view, overlay, &mut encoder, sky_state);
        self.submit_timed(encoder);
        self.last_present_us.set(0);
        Ok(())
    }

    fn begin_gpu_timing(&mut self) {
        if let Some(timer) = self.gpu_timer.as_mut() {
            timer.begin_frame();
        }
    }

    /// Submits the frame and collects the finished GPU timings.
    fn submit_timed(&mut self, mut encoder: wgpu::CommandEncoder) {
        if let Some(timer) = self.gpu_timer.as_mut() {
            timer.resolve(&mut encoder);
        }
        self.queue.submit(Some(encoder.finish()));
        if let Some(timer) = self.gpu_timer.as_mut() {
            timer.after_submit(&self.device);
        }
    }

    pub(super) fn stamp(&self, pass: &mut wgpu::RenderPass<'_>, index: u32) {
        if let Some(timer) = &self.gpu_timer {
            timer.stamp(pass, index);
        }
    }

    /// Blocks until the GPU finished every submitted frame, for benchmarks
    /// that time a whole frame.
    pub fn wait_idle(&self) {
        // A failed poll means the device is lost; the next draw reports it.
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }
}

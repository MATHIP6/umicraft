// SPDX-License-Identifier: MIT OR Apache-2.0

//! The render passes of a frame, in their order: sky, terrain, entities,
//! transparent terrain, effects, hand and overlays.

use super::pipelines::draw_gpu_pass;
use super::Renderer;
use crate::gpu_timer::DrawCallStats;
use crate::overlay::Overlay;
use crate::sky::SkyState;
use crate::ui_pass::UiBatchKind;

impl Renderer {
    /// The first-person hand and the item in it: camera-space vertices under a
    /// squeezed depth range, so the hand always wins the depth test.
    fn draw_hand(&self, pass: &mut wgpu::RenderPass<'_>) {
        if !self.hand.is_empty() {
            self.entity_renderer
                .render(pass, &self.hand_camera_bind_group, self.hand.iter());
        }

        pass.set_pipeline(&self.terrain_pipeline);
        pass.set_bind_group(0, &self.hand_item_bind_group, &[]);
        self.hand_items.draw(pass);
        self.world_glint.draw_hand(pass, &self.hand_item_bind_group);
    }

    pub(super) fn draw(
        &self,
        view: &wgpu::TextureView,
        overlay: &Overlay,
        encoder: &mut wgpu::CommandEncoder,
        sky_state: &SkyState,
    ) {
        self.panorama.prepare(encoder);
        // The reference clears to the fog colour; the dome fades into it.
        let clear = sky_state.fog.color;
        // With a menu that blurs the game, the world goes to a texture first.
        let blur = self.background_blur;
        // With a motion blur, the world goes to the texture too, and the hand and
        // the HUD are drawn over the blurred picture, sharp.
        let motion = self.motion_blur.is_active();
        let world_view = if blur || motion {
            self.scene_blur.scene_view()
        } else {
            view
        };
        let color_attachment = Some(wgpu::RenderPassColorAttachment {
            view: world_view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: f64::from(clear[0]),
                    g: f64::from(clear[1]),
                    b: f64::from(clear[2]),
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("main pass"),
            color_attachments: &[color_attachment],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.stamp(&mut pass, 0);
        let mut draw_stats = DrawCallStats {
            sky: 1,
            ..DrawCallStats::default()
        };
        self.sky.draw(&mut pass);
        self.panorama.draw(&mut pass);
        self.stamp(&mut pass, 1);

        draw_stats.opaque += self.draw_opaque_terrain(&mut pass);
        self.stamp(&mut pass, 2);

        draw_stats.entities += self.draw_entity_batches(&mut pass);

        // `renderEntities` draws the labels with the entities, before the
        // particles and the translucent terrain.
        // With the motion blur they come after it instead: the blur moves a pixel
        // by the depth under it, and a tag seen through a wall has the wall's.
        if !motion {
            self.nametags
                .draw(&mut pass, &self.overlay_pipeline, &self.ui_bind_group);
        }

        // Dropped items share the terrain pipeline and its texture array.
        pass.set_pipeline(&self.terrain_pipeline);
        pass.set_bind_group(0, &self.local_terrain_bind_group, &[]);
        self.world_items.draw(&mut pass);
        self.world_glint
            .draw_world(&mut pass, &self.local_terrain_bind_group);
        self.stamp(&mut pass, 3);

        // `renderWorldPass` draws the selection box right after the entities,
        // before the particles, the rain and the translucent layer: they hide
        // it and water and glass tint it. With the eyes in water it comes
        // after the translucent layer instead.
        let eyes_in_water = self.screen_overlays.underwater.is_some();
        if !eyes_in_water {
            self.outline.draw(&mut pass, &self.camera_bind_group);
        }

        // The clouds hide behind terrain and hide the water behind them, as in
        // `renderWorldPass`: before the translucent layer from below.
        self.clouds.draw_below(&mut pass);

        // `renderParticles` follows the entities; `renderRainSnow` comes after
        // both and before the translucent terrain.
        self.particles
            .draw(&mut pass, &self.local_terrain_bind_group);
        draw_stats.translucent += self.weather.draw(&mut pass, &self.local_camera_bind_group);

        draw_stats.translucent += self.draw_transparent_terrain(&mut pass);

        // The world border follows the weather and precedes the high clouds.
        self.border.draw(&mut pass);

        // From above 128 the clouds come after the translucent layer.
        self.clouds.draw_above(&mut pass);

        if let Some(cracks) = self.crack_mesh.as_ref() {
            pass.set_pipeline(&self.crack_pipeline);
            pass.set_bind_group(0, &self.terrain_bind_group, &[]);
            draw_gpu_pass(&mut pass, cracks);
        }

        // Under water the box comes after the translucent layer instead.
        if eyes_in_water {
            self.outline.draw(&mut pass, &self.camera_bind_group);
        }
        self.stamp(&mut pass, 4);

        if motion {
            // The hand goes into the picture that is blurred: it trails when it
            // moves. The overlays and the HUD come after, sharp.
            self.draw_hand(&mut pass);
            // The world is done: blur it, lay it over the screen, and draw the
            // overlays and the HUD sharp on top. The depth buffer is kept.
            drop(pass);
            self.motion_blur.run(encoder);
            pass = composite_pass(
                encoder,
                view,
                &self.depth_view,
                "motion blur composite pass",
                wgpu::LoadOp::Load,
            );
            self.motion_blur.blit(&mut pass);
            // Sharp, and still tested against the depth buffer of the world.
            self.nametags
                .draw(&mut pass, &self.overlay_pipeline, &self.ui_bind_group);
        }

        if !motion {
            self.draw_hand(&mut pass);
        }

        // `renderOverlays` follows the hand; the GUI's vignette, pumpkin and
        // portal come before the hotbar.
        self.screen.draw(&mut pass, &self.terrain_bind_group);

        if blur {
            // The world is done: smooth it, lay it over the screen, and draw
            // the HUD and the menu sharp on top.
            drop(pass);
            self.scene_blur.blur(encoder);
            pass = composite_pass(
                encoder,
                view,
                &self.depth_view,
                "blurred pass",
                wgpu::LoadOp::Clear(1.0),
            );
            self.scene_blur.blit(&mut pass);
        }

        draw_stats.ui += self.draw_ui(&mut pass, overlay);
        self.last_draw_calls.set(draw_stats);
        self.stamp(&mut pass, 5);
    }

    /// The opaque terrain, nearest chunk first: the SOLID quads (no alpha
    /// test, early depth rejection) and then the CUTOUT ones, which discard
    /// texels. Both write depth and do not depend on the order. Returns the
    /// draw calls.
    fn draw_opaque_terrain(&self, pass: &mut wgpu::RenderPass<'_>) -> u32 {
        let mut draws = 0;
        self.quad_indices.bind(pass);
        pass.set_bind_group(0, &self.chunk_bind_group, &[]);
        if !self.visible_solid.is_empty() {
            pass.set_pipeline(&self.terrain_solid_pipeline);
            draws += self.draw_visible_opaque(pass, true);
        }
        if !self.visible_cutout.is_empty() {
            pass.set_pipeline(&self.terrain_pipeline);
            draws += self.draw_visible_opaque(pass, false);
        }
        self.moving_blocks.draw_opaque(pass);
        draws
    }

    /// The entity batches, between the opaque and the translucent terrain:
    /// shadows, bodies, beams, ghosts, the apparition, the glow and the
    /// enchantment glint. Returns the batches drawn.
    fn draw_entity_batches<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) -> u32 {
        let mut draws = 0;
        // The shadows lie on the ground, drawn before the entities.
        if !self.shadow_batches.is_empty() {
            draws += self.shadow_batches.batch_count();
            let batches = self.shadow_batches.iter();
            self.entity_renderer
                .render_shadows(pass, &self.local_camera_bind_group, batches);
        }

        // Entities: depth write on, cutout discard, rendered between opaque and transparent passes.
        if !self.entity_batches.is_empty() {
            draws += self.entity_batches.batch_count();
            let batches = self.entity_batches.iter();
            self.entity_renderer
                .render(pass, &self.local_camera_bind_group, batches);
        }

        // The guardian beams, opaque like the entities they join.
        if !self.tinted_batches.is_empty() {
            let batches = self.tinted_batches.iter();
            self.entity_renderer
                .render_tinted(pass, &self.local_camera_bind_group, batches);
        }

        // The invisible entities a spectator sees, over the opaque ones.
        if !self.ghost_batches.is_empty() {
            draws += self.ghost_batches.batch_count();
            let batches = self.ghost_batches.iter();
            self.entity_renderer
                .render_ghosts(pass, &self.local_camera_bind_group, batches);
        }

        // The elder guardian apparition, a particle drawn after the entities.
        if !self.apparition_batches.is_empty() {
            let batches = self.apparition_batches.iter();
            self.entity_renderer
                .render_apparition(pass, &self.local_camera_bind_group, batches);
        }

        // The glow of the entities is added over them, without writing depth.
        if !self.additive_batches.is_empty() {
            draws += self.additive_batches.batch_count();
            let batches = self.additive_batches.iter();
            self.entity_renderer
                .render_additive(pass, &self.local_camera_bind_group, batches);
        }

        if !self.glint_batches.is_empty() {
            draws += self.glint_batches.batch_count();
            let batches = self.glint_batches.iter();
            self.entity_renderer
                .render_glint(pass, &self.local_camera_bind_group, batches);
        }
        draws
    }

    /// The translucent terrain: water and glass, farthest chunk first, with
    /// depth testing but no depth writes so blended faces do not occlude each
    /// other by submission order; then the moving blocks. Returns the draw
    /// calls.
    fn draw_transparent_terrain(&self, pass: &mut wgpu::RenderPass<'_>) -> u32 {
        let mut draws = 0;
        pass.set_pipeline(&self.terrain_transparent_pipeline);
        pass.set_bind_group(0, &self.chunk_bind_group, &[]);
        self.quad_indices.bind(pass);
        for draw in &self.visible_transparent {
            let Some(slot) = self
                .mesh_slots
                .get(draw.slot as usize)
                .and_then(Option::as_ref)
            else {
                continue;
            };
            let Some(transparent) = &slot.mesh.transparent else {
                continue;
            };
            let mut bound = false;
            for range in draw.runs.as_slice() {
                if !bound {
                    pass.set_vertex_buffer(0, transparent.vertex_buffer.slice(..));
                    bound = true;
                }
                pass.draw_indexed(range.clone(), 0, 0..1);
                draws += 1;
            }
        }
        // Drawn after the chunks' blended faces: a handful of quads, so no
        // sorting against them.
        self.moving_blocks.draw_transparent(pass);
        draws
    }

    /// The HUD (`overlay`) and the UI batches over the finished picture.
    /// Returns the draw calls.
    fn draw_ui(&self, pass: &mut wgpu::RenderPass<'_>, overlay: &Overlay) -> u32 {
        let mut draws = 0;
        if !overlay.is_empty() || !self.ui_pass.is_empty() {
            pass.set_vertex_buffer(0, self.overlay_buffer.slice(..));
            // The HUD uses the overlay pipeline with its own fallback atlas.
            if !overlay.is_empty() {
                pass.set_pipeline(&self.overlay_pipeline);
                pass.set_bind_group(0, &self.overlay_bind_group, &[]);
                pass.draw(0..overlay.vertex_count(), 0..1);
            }
            // UI batches alternate between the font and GUI pipelines.
            let hud_offset = overlay.vertex_count();
            for batch in self.ui_pass.batches() {
                let first = hud_offset + batch.first;
                let end = first + batch.count;
                match batch.kind {
                    UiBatchKind::Font => {
                        pass.set_pipeline(&self.overlay_pipeline);
                        pass.set_bind_group(0, &self.ui_bind_group, &[]);
                    }
                    UiBatchKind::BrandFont => {
                        // `UiPass` only emits this kind with a brand atlas,
                        // which `set_brand_font` uploads together.
                        let Some(brand) = &self.brand_bind_group else {
                            continue;
                        };
                        pass.set_pipeline(&self.brand_pipeline);
                        pass.set_bind_group(0, brand, &[]);
                    }
                    UiBatchKind::Icon => {
                        pass.set_pipeline(&self.gui_overlay_pipeline);
                        pass.set_bind_group(0, &self.icon_gui_bind_group, &[]);
                    }
                    UiBatchKind::IconGlint => {
                        if !self.glint.bind(pass) {
                            continue;
                        }
                    }
                    UiBatchKind::Gui | UiBatchKind::GuiInverted => {
                        if let Some(gui_bg) = &self.gui_bind_group {
                            pass.set_pipeline(if batch.kind == UiBatchKind::Gui {
                                &self.gui_overlay_pipeline
                            } else {
                                &self.gui_inverted_pipeline
                            });
                            pass.set_bind_group(0, gui_bg, &[]);
                        } else {
                            // No GUI atlas uploaded — skip this batch.
                            continue;
                        }
                    }
                }
                pass.draw(first..end, 0..1);
                draws += 1;
            }
        }
        if !overlay.is_empty() {
            draws += 1;
        }
        draws
    }

    /// Draws the SOLID (`solid`) or the CUTOUT quads of every visible opaque
    /// chunk, nearest first. The quad index buffer and the pipeline are bound
    /// by the caller. Pre-resolved ChunkRuns avoid per-frame section bit loops
    /// and eliminate hash map lookups.
    fn draw_visible_opaque(&self, pass: &mut wgpu::RenderPass<'_>, solid: bool) -> u32 {
        let mut draws = 0_u32;
        let list = if solid {
            &self.visible_solid
        } else {
            &self.visible_cutout
        };
        for draw in list {
            let Some(slot) = self
                .mesh_slots
                .get(draw.slot as usize)
                .and_then(Option::as_ref)
            else {
                continue;
            };
            let Some(opaque) = &slot.mesh.opaque else {
                continue;
            };
            let mut bound = false;
            for range in draw.runs.as_slice() {
                if !bound {
                    pass.set_vertex_buffer(0, opaque.vertex_buffer.slice(..));
                    bound = true;
                }
                pass.draw_indexed(range.clone(), 0, 0..1);
                draws += 1;
            }
        }
        draws
    }
}

/// A pass over the final `view` after a blur of the world: cleared to black,
/// on the depth buffer of the world (kept with `LoadOp::Load`).
fn composite_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    view: &'a wgpu::TextureView,
    depth: &'a wgpu::TextureView,
    label: &str,
    depth_load: wgpu::LoadOp<f32>,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: depth,
            depth_ops: Some(wgpu::Operations {
                load: depth_load,
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

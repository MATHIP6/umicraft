// SPDX-License-Identifier: MIT OR Apache-2.0

//! One displayed frame: the world sync, the music, the overlay and the
//! submission to the renderer.

use std::time::{Duration, Instant};

use umicraft_game::inventory::HOTBAR_START;
use umicraft_mesh::item::ItemMesh;
use umicraft_render::{NoIcons, RenderEnvironment, Renderer, ScreenOverlays, SkyState};
use winit::event_loop::ActiveEventLoop;

use super::mods_menu;
use super::session_control::lifecycle_step;
use super::App;
use super::BACKGROUND_FPS;
use crate::audio_bridge::listener::listener_of;
use crate::audio_bridge::music::MusicWorld;
use crate::debug::cpu_split::Stage;
use crate::debug::metrics::InputLatencyMetrics;
use crate::game::GameEvent;
use crate::hud::dials::DialInputs;
use crate::options::{self};
use crate::resources::item_icons::IconSource;
use crate::scene::block_entity::{mesh_block_entities, BlockEntityScene};
use crate::scene::item_world::{entity_items, push_held_item, WorldItems};
use crate::scene::map::map_id_of;
use crate::screens::portrait::{carry, turned_towards};
use crate::screens::Screen;
use crate::world_view::WorldView;

/// The frame-rate cap in force: the option, and at most [`BACKGROUND_FPS`] without
/// the focus.
pub(super) fn frame_cap(max_fps: u32, focused: bool) -> u32 {
    if focused {
        max_fps
    } else {
        max_fps.min(BACKGROUND_FPS)
    }
}

/// The stack in the selected hotbar slot of the local player.
fn held_stack(world: &WorldView) -> Option<&umicraft_protocol::packets::play::ItemStack> {
    let selected = usize::from(world.hud().selected_slot).min(8);
    world.window_view().player_slots[HOTBAR_START + selected].as_ref()
}

impl App {
    pub(super) fn resize(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.resize(width, height);
        }
        self.overlay.resize(width, height);
        self.camera.set_aspect(width, height);
    }

    /// Moves the world view forward, follows the player with the camera and
    /// applies the session lifecycle: leaving the connecting screen, showing
    /// the disconnected screen (ADR-0044).
    fn sync_world(&mut self, now: Instant) {
        umicraft_core::profile_scope!("sync_world");
        let (Some(session), Some(world), Some(renderer)) = (
            self.session.as_ref(),
            self.world.as_mut(),
            self.renderer.as_mut(),
        ) else {
            return;
        };
        world.sync(session.frames(), session.chunks(), renderer);
        let video = &self.options.video;
        world.update_camera(
            &mut self.camera,
            now,
            (
                video.view_bobbing,
                video.dynamic_fov,
                !self.mods.no_hurt_cam(),
            ),
        );
        world.focus_on(&self.camera);
        self.audio.play_cues(&world.take_sounds());
        self.audio
            .set_listener(listener_of(self.camera.position(), self.camera.yaw));
        let step = lifecycle_step(self.screen, world.placed(), world.ended());
        self.apply_lifecycle(step);
    }

    /// Drives the background music: the menu music until the server placed the
    /// player, then the music of the dimension and game mode.
    fn update_music(&mut self, now: Instant) {
        let in_world = self.session.is_some()
            && !matches!(self.screen, Screen::Connecting | Screen::Disconnected);
        let world = match self.world.as_ref() {
            Some(world) if in_world => {
                let (dimension, gamemode) = world.music_world();
                MusicWorld::Playing {
                    dimension,
                    gamemode,
                }
            }
            _ => MusicWorld::Menu,
        };
        self.audio.update_music(now, world, &self.options.sound);
    }

    /// Builds the frame-time graph; the rest of the HUD is drawn by the UI
    /// pass (`build_ui_frame`).
    fn build_overlay(&mut self) {
        let (width, height) = self
            .renderer
            .as_ref()
            .map_or((self.args.width, self.args.height), Renderer::size);
        self.overlay.clear();
        if !self.show_overlay || !self.world_visible() {
            return;
        }
        let graph_width = 240.0_f32.min(width as f32 - 20.0);
        let graph_height = 40.0_f32.min(height as f32 - 70.0);
        if graph_width > 0.0 && graph_height > 0.0 {
            self.overlay.frame_graph(
                width as f32 - graph_width - 10.0,
                height as f32 - graph_height - 10.0,
                graph_width,
                graph_height,
                self.clock.frame_times.make_contiguous(),
            );
        }
    }

    /// Steps the compass and the clock and writes their new frames into the
    /// block array.
    fn advance_dials(&mut self, elapsed: Duration, environment: RenderEnvironment) {
        let inputs = self.world.as_ref().map(|world| {
            let (_, spawn) = world.compass_world();
            DialInputs {
                surface: environment.dimension == 0,
                position: (self.camera.eye().x, self.camera.eye().z),
                // The camera yaw is the server yaw turned by half a circle.
                yaw_degrees: f64::from((self.camera.yaw - std::f32::consts::PI).to_degrees()),
                spawn: spawn.map(|(x, _, z)| (f64::from(x), f64::from(z))),
                celestial_angle: f64::from(SkyState::at(environment).celestial_angle),
            }
        });
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };
        self.dials
            .advance(elapsed, inputs.as_ref(), |layer, pixels| {
                renderer.write_block_layer(layer, pixels);
            });
    }

    pub(super) fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        self.poll_asset_loading();
        umicraft_core::profiling::new_frame();
        umicraft_core::profile_scope!("redraw");
        let now = Instant::now();
        self.cpu.start(now);
        let elapsed = self.clock.tick(now, self.show_overlay);
        // Phase E.9: the game thread takes network events once per frame.
        if let Some(session) = self.session.as_ref() {
            session.note_frame(Duration::from_secs_f32(elapsed));
        }
        self.update_mods(now);

        // The Zoom mod narrows the field of view, with a glide in and out.
        let zooming = self.zoom_held();
        let zoom_scale = self.zoom.step(elapsed, zooming, self.mods.zoom().as_ref());
        self.camera
            .set_fov_degrees(self.options.video.fov * zoom_scale);
        self.advance_menus(Duration::from_secs_f32(elapsed));
        self.publish_input();
        self.sync_world(now);
        let mut environment = self
            .world
            .as_ref()
            .map(|world| world.environment(now, self.options.video.render_distance))
            .unwrap_or_default();
        environment.gamma = self.options.video.gamma;
        self.update_server_pack();
        self.update_music(now);
        self.process_completions();
        self.update_death_screen(now);
        self.update_container_cursor();
        self.build_overlay();

        if self.exit_error.is_some() {
            // The session ended (the server disconnected): leave the loop.
            event_loop.exit();
            return;
        }

        let (width, height) = self
            .renderer
            .as_ref()
            .map_or((self.args.width, self.args.height), Renderer::size);
        // The mods' camera layers move what is drawn, never the player.
        self.place_mod_camera(elapsed, now);
        // The camera is where it stays for this frame: the entities are made once,
        // relative to its render origin. After the camera layers, which may move the
        // origin: built before, every entity would shift with the camera.
        if let Some(world) = self.world.as_ref() {
            world.entities_into(now, self.camera.render_origin(), &mut self.frame.entities);
        } else {
            self.frame.entities.clear();
        }
        // The camera is final: the external mods see it in `Frame` and draw
        // their widgets with it, before the HUD is built.
        self.publish_frame((width, height));
        // The dials first, so the icon keys of this frame name the frames the
        // block array shows.
        self.advance_dials(Duration::from_secs_f32(elapsed), environment);
        self.cpu.lap(Stage::World);
        let ui_frame = self.build_ui_frame(width, height);
        self.cpu.lap(Stage::Hud);
        if let Some(action) = self.menu_action.take() {
            self.apply_game_menu_action(action);
        }
        if let Some(command) = self.menu_command.take() {
            self.apply_menu_command(command, event_loop);
        }
        let outcome = std::mem::take(&mut self.options_menu.outcome);
        self.apply_options_outcome(outcome);
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let effects = self.options_menu.video_applier.apply(
            &self.options.video,
            self.window.as_deref(),
            renderer,
            self.assets.entity_textures.as_ref(),
        );
        if effects.reload_atlas {
            self.reload_resources();
        } else if effects.remesh {
            self.send_event(GameEvent::ReMeshAll);
        }
        // The renderer leaves `self` while the scene is handed to it, so the
        // steps can read the rest of the window state alongside it.
        let Some(mut renderer) = self.renderer.take() else {
            return;
        };
        let partial_tick = environment.partial_tick;
        self.send_scene(&mut renderer, now, environment, elapsed, (width, height));
        self.upload_entities(&mut renderer, now, partial_tick);
        self.draw_portrait(&mut renderer);
        self.update_post_passes(&mut renderer);
        let renderer = self.renderer.insert(renderer);
        if let (Some(frame), Some(items), Some(textures)) = (
            ui_frame.as_ref(),
            self.assets.items.as_deref(),
            self.assets.entity_textures.as_ref(),
        ) {
            self.entity_icons
                .prepare(&frame.draw_list, items, textures, renderer);
        }
        if std::mem::take(&mut self.presses.screenshot) {
            renderer.request_capture();
        }
        self.cpu.lap(Stage::Scene);
        let rendered = match (ui_frame.as_ref(), self.assets.items.as_deref()) {
            (Some(frame), Some(items)) => renderer.render_with_ui(
                &self.camera,
                &self.overlay,
                &frame.draw_list,
                &mut IconSource(items),
            ),
            (Some(frame), None) => {
                renderer.render_with_ui(&self.camera, &self.overlay, &frame.draw_list, &mut NoIcons)
            }
            (None, _) => renderer.render(&self.camera, &self.overlay),
        };
        self.cpu.lap(Stage::Render);
        self.cpu.finish();
        let present_us = renderer.presentation_time_us();
        let captured = renderer.take_capture();
        self.record_input_latency(present_us);
        if let Some(frame) = ui_frame {
            self.ui.recycle_output(frame);
        }
        if let Some(captured) = captured {
            self.save_screenshot(captured);
        }
        if let Err(error) = rendered {
            self.exit_error = Some(error.to_string());
            event_loop.exit();
            return;
        }
        // The frame-rate cap: the deadline of the next frame, for the
        // event-driven scheduling of `about_to_wait`.
        let cap = frame_cap(self.options.video.max_fps, self.focused);
        let budget = (!self.options.video.vsync && cap < options::UNLIMITED_FPS)
            .then(|| Duration::from_secs_f32(1.0 / cap as f32));
        self.clock.schedule_next(budget, Instant::now());
    }

    /// Hands the renderer what this frame shows besides the entities: the
    /// environment, the clouds, rain and particles, the screen overlays, the
    /// border, the animated textures, the selection, the blurs, the name tags,
    /// the cracks and the moving blocks.
    fn send_scene(
        &mut self,
        renderer: &mut Renderer,
        now: Instant,
        environment: RenderEnvironment,
        elapsed: f32,
        (width, height): (u32, u32),
    ) {
        // One environment per frame: the sky and the fog read the same clock
        // and interpolation fraction as the camera.
        renderer.set_environment(environment);
        renderer.set_cloud_mode(self.options.video.cloud_mode());
        // 20 client ticks per second drive the cloud drift.
        renderer.set_cloud_ticks((now - self.clock.started).as_secs_f64() * 20.0);
        if let Some(world) = self.world.as_ref() {
            renderer.set_precipitation(world.precipitation(), self.options.video.fancy_graphics);
            world.particle_sprites_into(
                &mut self.frame.particle_sprites,
                environment.partial_tick,
                self.camera.render_origin(),
                self.assets.items.as_deref(),
            );
            renderer.set_particles(&self.frame.particle_sprites);
        }
        renderer.set_screen_overlays(
            self.world
                .as_mut()
                .map_or_else(ScreenOverlays::default, |world| {
                    world.screen_overlays(now, self.options.video.fancy_graphics)
                }),
        );
        renderer.set_world_border(
            self.world.as_ref().and_then(WorldView::border),
            (now - self.clock.started).as_secs_f64(),
        );
        renderer.set_glint_time((now - self.clock.started).as_secs_f32());
        self.assets
            .texture_animator
            .advance(Duration::from_secs_f32(elapsed), |layer, pixels| {
                renderer.write_block_layer(layer, pixels);
            });
        renderer.set_selection_outline(self.world.as_ref().map_or(&[], WorldView::selection));
        renderer.set_debug_lines(&self.debug_lines());
        // The mods menu blurs the game behind it to keep the focus on itself.
        let blur = mods_menu::background_blur(self.screen, &self.menu);
        renderer.set_background_blur(blur);
        // The Motion Blur mod: the settings cost nothing while they are the same.
        renderer.set_motion_blur(self.mods.motion_blur());
        renderer.set_fullbright(self.mods.fullbright());
        // The body of the player is part of the picture in the third person.
        renderer.set_motion_blur_body(
            self.world
                .as_ref()
                .and_then(|world| world.third_person_body(now)),
        );
        renderer.set_nametags(&self.frame.nametags, (width, height));
        if let Some((vertices, indices)) = self.world.as_mut().and_then(WorldView::take_crack_mesh)
        {
            renderer.set_block_cracks(&vertices, &indices);
        }

        if let Some(world) = self.world.as_ref() {
            world.moving_blocks_mesh_into(
                environment.partial_tick,
                &mut self.frame.moving_blocks_mesh,
            );
            renderer.set_moving_blocks(&self.frame.moving_blocks_mesh);
        }
    }

    /// Builds and uploads the entities of the frame: their textures and
    /// meshes, the block entities, the dropped items, the maps and the
    /// first-person hand with what it holds.
    fn upload_entities(&mut self, renderer: &mut Renderer, now: Instant, partial_tick: f32) {
        // No world, no hand: set again below when there is one.
        renderer.set_motion_blur_hand_moving(false);
        let style = self.mods.biped_style();
        if let (Some(entity_textures), Some(world)) =
            (self.assets.entity_textures.as_ref(), self.world.as_ref())
        {
            // Everything below is built relative to this origin and the eye
            // relative to it (F2-1b); the entity pass draws with a camera
            // whose origin is zero.
            let render_origin = self.camera.render_origin();
            let render_origin_blocks = [
                render_origin.x as i32,
                render_origin.y as i32,
                render_origin.z as i32,
            ];
            let entities = &self.frame.entities;
            let hand = world.hand_pose(now);
            // The motion blur follows the hand too: a swing blurs it, a still hand not.
            renderer.set_motion_blur_hand_moving(
                hand.visible && self.frame.last_hand.is_some_and(|last| last != hand),
            );
            self.frame.last_hand = hand.visible.then_some(hand);
            let local_body = world.local_body();
            let hand_skin = entity_textures.hand_skin(local_body.as_ref());
            entity_textures.set_view_yaw(self.camera.yaw);
            entity_textures.set_view_pitch(self.camera.pitch);
            entity_textures.set_mob_appearance(world.mob_appearance(partial_tick));
            entity_textures.set_camera_position(self.camera.relative_eye());
            entity_textures.set_hand_swing(hand.swing);
            entity_textures.set_biped_style(style);
            entity_textures.set_capes(
                self.mods.capes(),
                local_body
                    .as_ref()
                    .and_then(|body| body.uuid)
                    .map(u128::from_be_bytes),
            );
            entity_textures.set_cull_view(self.camera.relative_eye(), self.camera.forward());
            entity_textures.set_viewer_spectator(
                umicraft_game::gamemode::GameMode::from_wire(world.status().gamemode)
                    == umicraft_game::gamemode::GameMode::Spectator,
            );
            let held_map = held_stack(world)
                .filter(|_| hand.visible)
                .and_then(|stack| map_id_of(stack.item_id, stack.damage));
            let framed = self
                .assets
                .map_textures
                .as_mut()
                .map_or_else(Vec::new, |maps| {
                    maps.frame_batches(entities, world.maps(), renderer)
                });
            // The UI font may have been swapped since the textures were made
            // (theme font, GUI scale): the sign text must sample the atlas
            // its glyphs are laid out with.
            if let Some(font) = self.assets.ui_atlas.as_ref() {
                entity_textures.sync_sign_font(renderer, font);
            }
            let mut block_entities = if world.has_block_entities() {
                mesh_block_entities(
                    world.block_entities(),
                    &BlockEntityScene {
                        lids: world.chest_lids(),
                        signs: world.signs(),
                        data: world.block_entity_data(),
                        skins: entity_textures,
                        seconds: (now - self.clock.started).as_secs_f32(),
                        font: self.assets.ui_atlas.as_deref(),
                        partial_tick,
                        camera: self.camera.relative_eye(),
                        origin: render_origin_blocks,
                    },
                )
            } else {
                crate::scene::block_entity::BlockEntityMeshes::default()
            };
            let mut hand_item = None;
            if let Some(items) = self.assets.items.as_deref() {
                entity_items::push(&mut block_entities, entities, items, entity_textures);
                hand_item = held_stack(world)
                    .filter(|_| hand.visible)
                    .and_then(|stack| {
                        entity_items::hand_item(
                            &mut block_entities,
                            (stack.item_id, stack.damage, stack.nbt.as_deref()),
                            &hand,
                            (self.camera.view_bob(), world.hand_light()),
                            items,
                            entity_textures,
                        )
                    });
            }
            entity_textures.set_block_entities(block_entities);
            entity_textures.mesh_and_upload_with(entities, &framed, renderer);
            match (held_map, self.assets.map_textures.as_mut()) {
                (Some(map_id), Some(maps)) => {
                    let bob = self.camera.view_bob();
                    let held = maps.hand_batches(
                        world.maps(),
                        map_id,
                        (&hand, bob, world.hand_light()),
                        renderer,
                    );
                    entity_textures.upload_hand_map(&hand, bob, hand_skin, &held, renderer);
                }
                _ => match &hand_item {
                    Some((slot, vertices)) => entity_textures.upload_hand_item(
                        &hand,
                        self.camera.view_bob(),
                        hand_skin,
                        (*slot, vertices),
                        renderer,
                    ),
                    None => entity_textures.upload_hand(
                        &hand,
                        self.camera.view_bob(),
                        hand_skin,
                        renderer,
                    ),
                },
            }
            if let Some(items) = self.assets.items.as_deref() {
                self.world_items
                    .set_view(self.camera.yaw, self.camera.pitch);
                self.world_items.set_biped_style(style);
                let dropped = self.world_items.build(entities, items);
                renderer.set_world_items(&dropped.items);
                renderer.set_world_glint(&dropped.glint);
                self.assets.hand_layers.clear();
                if let Some(stack) = held_stack(world).filter(|stack| {
                    hand.visible && map_id_of(stack.item_id, stack.damage).is_none()
                }) {
                    let nbt = stack.nbt.as_deref();
                    push_held_item(
                        &mut self.assets.hand_layers,
                        (stack.item_id, stack.damage, nbt),
                        &hand,
                        (self.camera.view_bob(), world.hand_light()),
                        items,
                    );
                }
                renderer.set_hand_items(&self.assets.hand_layers.items);
                renderer.set_hand_glint(&self.assets.hand_layers.glint);
            }
        }
    }

    /// Draws the player into the portrait of the inventory screen, turned
    /// towards the mouse.
    fn draw_portrait(&mut self, renderer: &mut Renderer) {
        if let (Some(spot), Some(textures), Some(body)) = (
            self.portrait,
            self.assets.entity_textures.as_ref(),
            self.world.as_ref().and_then(|world| world.local_body()),
        ) {
            let mouse = spot.mouse_offset(self.input.pointer);
            let posed = turned_towards(body, mouse);
            let matrix = spot.placement.matrix(mouse);
            // The item in the hand lives in the item layers, not in the entity mesh.
            let mut held = ItemMesh::default();
            if let Some(items) = self.assets.items.as_deref() {
                let mut portrait_items = WorldItems::default();
                portrait_items.set_biped_style(self.mods.biped_style());
                held.clone_from(
                    &portrait_items
                        .build(std::slice::from_ref(&posed), items)
                        .items,
                );
                carry(&mut held, matrix);
            }
            textures.render_portrait(&posed, matrix, &held, renderer);
        }
    }

    /// Records the input latency of the frame for F3: how long the game
    /// thread took the last input and how long it waited, the time from the
    /// last input to this frame, and `present_us`, the presentation time.
    fn record_input_latency(&mut self, present_us: u32) {
        let (proc_us, queue_us) = self.world.as_ref().map_or((0, 0), |w| {
            let debug = w.debug_world();
            (
                debug.input_processing_micros,
                debug.input_queue_latency_micros,
            )
        });
        let input_to_frame_ms = (Instant::now()
            .saturating_duration_since(self.input.last_time)
            .as_secs_f32()
            * 1000.0)
            .min(999.0);
        self.clock
            .metrics_window
            .set_input_latency(InputLatencyMetrics {
                input_processing_us: proc_us,
                event_queue_latency_us: queue_us,
                input_to_frame_ms,
                presentation_time_ms: present_us as f32 / 1000.0,
            });
    }
}

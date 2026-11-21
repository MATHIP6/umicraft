// SPDX-License-Identifier: MIT OR Apache-2.0

//! Window glue of the options screens: opening them from the title screen or
//! the game menu, applying what they report, saving, and reloading the
//! language and the resource packs (ADR-0043, ADR-0044).
//!
//! A child of [`super`] so it reaches the window state without widening its
//! visibility.

use std::sync::Arc;

use winit::event::{ElementState, KeyEvent, MouseButton};
use winit::keyboard::{KeyCode, PhysicalKey};

use super::startup::{
    load_packs_and_terrain, upload_custom_sky, upload_glint_texture, upload_gui_atlas,
    upload_sky_textures,
};
use super::App;
use crate::game::GameEvent;
use crate::hud::dials::DialAnimator;
use crate::language;
use crate::meshing::pool::worker_count;
use crate::options::effects::{open_folder, pack_paths, packs_folder, ServerSettings};
use crate::options::video_sections::AnimationOptions;
use crate::options::Binding;
use crate::resources::texture_animator::{AnimationFilter, TextureAnimator};
use crate::scene::entity::EntityTextures;
use crate::scene::map::MapTextures;
use crate::screens::options::{Catalog, Outcome as OptionsOutcome, Page};
use crate::screens::Screen;
use umicraft_game::particles::{ParticleCategory, ParticleFilter};

/// The animated block textures the Animations page leaves running. A free
/// function so it can be called while the renderer is borrowed from the window.
pub(super) fn animation_filter(animations: &AnimationOptions) -> AnimationFilter {
    AnimationFilter {
        water: animations.water,
        lava: animations.lava,
        fire: animations.fire,
        portal: animations.portal,
        terrain: animations.terrain,
    }
}

impl App {
    /// Handles a key while the options screen is open. While a key binding
    /// waits for its key, the next press is the key (Escape gives up).
    pub(super) fn handle_options_key(&mut self, event: KeyEvent) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        let pressed = event.state == ElementState::Pressed && !event.repeat;
        if self.options_menu.screen.capturing().is_some() {
            if pressed {
                if code == KeyCode::Escape {
                    self.options_menu.screen.cancel_capture();
                } else {
                    self.options_menu
                        .screen
                        .capture(&mut self.options, Binding::Key(code));
                }
            }
            return;
        }
        if pressed && code == KeyCode::Escape {
            self.close_options();
            return;
        }
        self.ui_input.key(code, event.state, event.text.as_deref());
    }

    /// Binds a mouse button pressed while the controls page waits for a key
    /// (`GuiControls.mouseClicked`). Returns whether the click was taken.
    pub(super) fn capture_mouse_binding(
        &mut self,
        screen: Screen,
        state: ElementState,
        button: MouseButton,
    ) -> bool {
        if screen != Screen::Options
            || state != ElementState::Pressed
            || self.options_menu.screen.capturing().is_none()
        {
            return false;
        }
        if let Some(binding) = Binding::from_mouse(button) {
            self.options_menu.screen.capture(&mut self.options, binding);
        }
        true
    }

    /// Writes the options to disk when they changed since the last write.
    pub(super) fn save_options(&mut self) {
        if self.options == self.options_menu.saved {
            return;
        }
        if let Some(path) = &self.options_menu.path {
            if let Err(error) = self.options.save(path) {
                tracing::warn!(path = %path.display(), %error, "could not save the options");
            }
        }
        self.options_menu.saved = self.options.clone();
    }

    /// Tells the game thread which particle categories may be created.
    pub(super) fn send_particle_categories(&self) {
        let animations = &self.options.video.animations;
        let categories = [
            (ParticleCategory::Redstone, animations.redstone_particles),
            (ParticleCategory::Explosion, animations.explosion_particles),
            (ParticleCategory::Flame, animations.flame_particles),
            (ParticleCategory::Smoke, animations.smoke_particles),
            (ParticleCategory::Void, animations.void_particles),
            (ParticleCategory::Water, animations.water_particles),
            (ParticleCategory::RainSplash, animations.rain_splash),
            (ParticleCategory::Portal, animations.portal_particles),
            (ParticleCategory::Potion, animations.potion_particles),
            (ParticleCategory::Dripping, animations.dripping_particles),
            (ParticleCategory::Firework, animations.firework_particles),
        ]
        .into_iter()
        .fold(ParticleFilter::default(), |filter, (category, on)| {
            filter.with(category, on)
        });
        self.send_event(GameEvent::ParticleCategories(categories));
    }

    /// Tells the game thread how many particles to create.
    pub(super) fn send_particle_setting(&self) {
        self.send_event(GameEvent::ParticleSetting(self.options.video.particles));
    }

    /// Opens the options screen on `page`, listing the languages and packs as
    /// they are now. Escape and Done lead back to the screen it was opened
    /// from: the title screen or the game menu.
    pub(super) fn open_options(&mut self, page: Page) {
        self.options_menu.origin = self.screen;
        self.options_menu
            .screen
            .set_over_world(self.options_menu.origin == Screen::GameMenu);
        self.options_menu.screen.set_catalog(Catalog::discover());
        self.sync_pack_icons();
        if page != Page::Main {
            self.options_menu.screen.open_page(page, &self.options);
        }
        self.set_screen(Screen::Options);
    }

    /// Puts the `pack.png` of the listed resource packs into the GUI atlas
    /// and points the packs page at them. The atlas keeps one set of runtime
    /// images, so the favicons of the server list are put back on its next
    /// visit.
    fn sync_pack_icons(&mut self) {
        let (Some(atlas), Some(renderer)) =
            (self.assets.gui_atlas.as_ref(), self.renderer.as_mut())
        else {
            return;
        };
        let grown = crate::resources::official::grow_atlas(
            Some(atlas),
            &self.options_menu.screen.pack_icon_images(),
        );
        upload_gui_atlas(renderer, Some(&grown));
        self.options_menu.screen.assign_pack_icon_sheets(&grown);
        self.assets.gui_atlas = Some(grown);
        self.server_icons_revision = u64::MAX;
    }

    /// Rebuilds the language handle after the language page picked another.
    fn reload_language(&mut self) {
        let lang = Arc::new(language::load(&self.options.language.code));
        language::install(Arc::clone(&lang));
        self.lang = lang;
        // The server learns the new locale.
        self.send_event(ServerSettings::of(&self.options).event());
    }

    /// Applies what the options screen reported in its last frame.
    pub(super) fn apply_options_outcome(&mut self, outcome: OptionsOutcome) {
        if outcome.language_changed {
            self.reload_language();
        }
        if outcome.packs_changed {
            self.reload_resources();
        }
        if outcome.open_packs_folder {
            if let Some(folder) = packs_folder() {
                open_folder(&folder);
            }
        }
        if outcome.done {
            self.close_options();
        }
    }

    /// Closes the options screen: saves the options, tells the server about
    /// a changed render distance, chat setting or skin layer (when a session
    /// runs) and goes back to the screen that opened it.
    pub(super) fn close_options(&mut self) {
        self.options_menu.screen.reset();
        self.options.clamp();
        let changed =
            ServerSettings::of(&self.options) != ServerSettings::of(&self.options_menu.saved);
        let particles_changed =
            self.options.video.particles != self.options_menu.saved.video.particles;
        let settings = ServerSettings::of(&self.options);
        let animations_changed =
            self.options.video.animations != self.options_menu.saved.video.animations;
        self.save_options();
        if particles_changed {
            self.send_particle_setting();
        }
        if animations_changed {
            self.assets
                .texture_animator
                .set_filter(animation_filter(&self.options.video.animations));
            self.send_particle_categories();
        }
        if changed {
            self.send_event(settings.event());
        }
        self.set_screen(self.options_menu.origin);
    }

    /// Hot-reloads the active resource packs, rebinding the block atlas and
    /// re-meshing all loaded chunk columns.
    pub(super) fn reload_resources(&mut self) {
        tracing::info!("reloading resource packs...");
        let (terrain, files) = load_packs_and_terrain(&pack_paths(
            &self.args.resource_packs,
            &self.options,
            packs_folder().as_deref(),
            self.server_pack.active_path(),
        ));
        self.assets.files = files;
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let installed = terrain.install(renderer);
        // The packs page can be opened again without a new catalog: keep its icons.
        let icons = self.options_menu.screen.pack_icon_images();
        let gui_atlas = crate::resources::official::grow_atlas(installed.gui_atlas(), &icons);
        upload_gui_atlas(renderer, Some(&gui_atlas));
        self.options_menu.screen.assign_pack_icon_sheets(&gui_atlas);
        upload_glint_texture(renderer, installed.glint_texture());
        upload_sky_textures(renderer, &self.assets.files);
        self.assets.custom_sky = upload_custom_sky(renderer, &self.assets.files);
        self.assets.gui_atlas = Some(gui_atlas);
        self.hud_cache.invalidate();
        self.server_icons_revision = u64::MAX;
        self.assets.items = installed.items();
        self.dials = DialAnimator::new(installed.dials());
        self.assets.leaf_layers = installed.leaf_layers();
        self.assets.texture_animator = TextureAnimator::for_terrain(
            &installed,
            animation_filter(&self.options.video.animations),
        );
        self.assets.entity_textures = Some(EntityTextures::load_with_font(
            renderer,
            self.assets.ui_atlas.as_ref(),
            self.assets.files.clone(),
        ));
        self.assets.map_textures = Some(MapTextures::load(renderer, &self.assets.files));
        if let Some(world) = self.world.as_mut() {
            world.reload_terrain(
                &installed,
                worker_count(self.options.video.mesh_threads),
                renderer,
            );
            self.send_event(GameEvent::ReMeshAll);
        }
        tracing::info!("resource packs reloaded");
    }
}

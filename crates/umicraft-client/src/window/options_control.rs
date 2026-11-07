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

use super::{upload_glint_texture, upload_gui_atlas, upload_sky_textures, App};
use crate::dials::DialAnimator;
use crate::entity_render::EntityTextures;
use crate::game::GameEvent;
use crate::language;
use crate::map_render::MapTextures;
use crate::mesher::default_worker_count;
use crate::options::Binding;
use crate::options_effects::{open_folder, pack_paths, packs_folder, ServerSettings};
use crate::options_screen::{Catalog, Outcome as OptionsOutcome, Page};
use crate::screen::Screen;
use crate::terrain_textures::TerrainTextures;
use crate::texture_animator::TextureAnimator;

impl App {
    /// Handles a key while the options screen is open. While a key binding
    /// waits for its key, the next press is the key (Escape gives up).
    pub(super) fn handle_options_key(&mut self, event: KeyEvent) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        let pressed = event.state == ElementState::Pressed && !event.repeat;
        if self.options_screen.capturing().is_some() {
            if pressed {
                if code == KeyCode::Escape {
                    self.options_screen.cancel_capture();
                } else {
                    self.options_screen
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
            || self.options_screen.capturing().is_none()
        {
            return false;
        }
        if let Some(binding) = Binding::from_mouse(button) {
            self.options_screen.capture(&mut self.options, binding);
        }
        true
    }

    /// Writes the options to disk when they changed since the last write.
    pub(super) fn save_options(&mut self) {
        if self.options == self.saved_options {
            return;
        }
        if let Some(path) = &self.options_path {
            if let Err(error) = self.options.save(path) {
                tracing::warn!(path = %path.display(), %error, "could not save the options");
            }
        }
        self.saved_options = self.options.clone();
    }

    /// Tells the game thread how many particles to create.
    pub(super) fn send_particle_setting(&self) {
        self.send_event(GameEvent::ParticleSetting(self.options.video.particles));
    }

    /// Opens the options screen on `page`, listing the languages and packs as
    /// they are now. Escape and Done lead back to the screen it was opened
    /// from: the title screen or the game menu.
    pub(super) fn open_options(&mut self, page: Page) {
        self.options_origin = self.screen;
        self.options_screen
            .set_over_world(self.options_origin == Screen::GameMenu);
        self.options_screen.set_catalog(Catalog::discover());
        if page != Page::Main {
            self.options_screen.open_page(page, &self.options);
        }
        self.set_screen(Screen::Options);
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
        self.options_screen.reset();
        self.options.clamp();
        let changed = ServerSettings::of(&self.options) != ServerSettings::of(&self.saved_options);
        let particles_changed = self.options.video.particles != self.saved_options.video.particles;
        let settings = ServerSettings::of(&self.options);
        self.save_options();
        if particles_changed {
            self.send_particle_setting();
        }
        if changed {
            self.send_event(settings.event());
        }
        self.set_screen(self.options_origin);
    }

    /// Hot-reloads the active resource packs, rebinding the block atlas and
    /// re-meshing all loaded chunk columns.
    pub(super) fn reload_resources(&mut self) {
        tracing::info!("reloading resource packs...");
        let terrain = TerrainTextures::load_with_packs(&pack_paths(
            &self.args.resource_packs,
            &self.options,
            packs_folder().as_deref(),
            self.server_pack.active_path(),
        ));
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let installed = terrain.install(renderer);
        let gui_atlas = crate::official::grow_atlas(installed.gui_atlas(), &[]);
        upload_gui_atlas(renderer, Some(&gui_atlas));
        upload_glint_texture(renderer, installed.glint_texture());
        upload_sky_textures(renderer);
        self.gui_atlas = Some(gui_atlas);
        self.server_icons_revision = u64::MAX;
        self.items = installed.items();
        self.dials = DialAnimator::new(installed.dials());
        self.texture_animator = TextureAnimator::new(installed.animations().to_vec());
        self.entity_textures = Some(EntityTextures::load_with_font(
            renderer,
            self.ui_atlas.as_ref(),
        ));
        self.map_textures = Some(MapTextures::load(renderer));
        if let Some(world) = self.world.as_mut() {
            world.reload_terrain(&installed, default_worker_count(), renderer);
            self.send_event(GameEvent::ReMeshAll);
        }
        tracing::info!("resource packs reloaded");
    }
}

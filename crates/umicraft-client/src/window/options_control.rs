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

use super::startup::{upload_glint_texture, upload_gui_atlas, upload_sky_textures};
use super::App;
use crate::game::GameEvent;
use crate::hud::dials::DialAnimator;
use crate::language;
use crate::meshing::pool::worker_count;
use crate::options::effects::{open_folder, pack_paths, packs_folder, ServerSettings};
use crate::options::Binding;
use crate::resources::terrain::TerrainTextures;
use crate::resources::texture_animator::TextureAnimator;
use crate::scene::entity::EntityTextures;
use crate::scene::map::MapTextures;
use crate::screens::options::{
    Catalog, ModKey, ModKeyChange, ModKeyGroup, Outcome as OptionsOutcome, Page,
};
use crate::screens::Screen;

impl App {
    /// Handles a key while the options screen is open. While a key binding
    /// waits for its key, the next press is the key (Escape gives up).
    pub(super) fn handle_options_key(&mut self, event: KeyEvent) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        let pressed = event.state == ElementState::Pressed && !event.repeat;
        if self.options_menu.screen.is_capturing() {
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

    /// Gives the controls page the key bindings of the enabled external mods. Built
    /// only while that page is shown, because it runs every frame.
    pub(super) fn sync_mod_keys(&mut self) {
        let groups = match &self.external_mods {
            Some(mods) if self.options_menu.screen.page() == Page::Controls => {
                mods.host().map_or_else(Vec::new, mod_key_groups)
            }
            _ => Vec::new(),
        };
        self.options_menu.screen.set_mod_keys(groups);
    }

    /// Writes what the player rebound on the controls page to the mods and saves
    /// each mod's choices. A binding the host refuses is logged.
    fn apply_mod_key_changes(&mut self) {
        let changes = self.options_menu.screen.take_mod_key_changes();
        let Some(mods) = &self.external_mods else {
            return;
        };
        let Some(host) = mods.host() else {
            return;
        };
        for change in changes {
            match set_mod_key(host, &change) {
                Ok(id) => mods.save_config(&id),
                Err(error) => tracing::warn!(%error, "ignored a key binding change"),
            }
        }
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
            || !self.options_menu.screen.is_capturing()
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
        self.apply_mod_key_changes();
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
        self.save_options();
        if particles_changed {
            self.send_particle_setting();
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
        // The packs page can be opened again without a new catalog: keep its icons.
        let icons = self.options_menu.screen.pack_icon_images();
        let gui_atlas = crate::resources::official::grow_atlas(installed.gui_atlas(), &icons);
        upload_gui_atlas(renderer, Some(&gui_atlas));
        self.options_menu.screen.assign_pack_icon_sheets(&gui_atlas);
        upload_glint_texture(renderer, installed.glint_texture());
        upload_sky_textures(renderer);
        self.assets.gui_atlas = Some(gui_atlas);
        self.hud_cache.invalidate();
        self.server_icons_revision = u64::MAX;
        self.assets.items = installed.items();
        self.dials = DialAnimator::new(installed.dials());
        self.assets.texture_animator = TextureAnimator::new(installed.animations().to_vec());
        self.assets.entity_textures = Some(EntityTextures::load_with_font(
            renderer,
            self.assets.ui_atlas.as_ref(),
        ));
        self.assets.map_textures = Some(MapTextures::load(renderer));
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

/// The bindings of the enabled mods, one group per mod, named after it. A mod that
/// is off or failed has no row: its keys do nothing, so they would only mislead.
fn mod_key_groups(host: &umicraft_modhost::ModHost) -> Vec<ModKeyGroup> {
    host.mods()
        .filter(|view| view.enabled && !view.failed)
        .map(|view| ModKeyGroup {
            name: view.name.to_owned(),
            keys: view
                .keys()
                .map(|key| ModKey {
                    mod_id: key.mod_id.to_string(),
                    key: key.key.to_owned(),
                    label: key.label.to_owned(),
                    default: key.default_binding.to_owned(),
                    current: key.current,
                })
                .collect(),
        })
        .filter(|group| !group.keys.is_empty())
        .collect()
}

/// Why a rebinding could not be applied.
#[derive(Debug, thiserror::Error)]
enum KeyChangeError {
    #[error(transparent)]
    Id(#[from] umicraft_modhost::IdError),
    #[error(transparent)]
    Configure(#[from] umicraft_modhost::ConfigureError),
}

/// Applies one rebinding to its mod and returns the mod, so its file is saved.
fn set_mod_key(
    host: &umicraft_modhost::ModHost,
    change: &ModKeyChange,
) -> Result<umicraft_modhost::ModId, KeyChangeError> {
    let id: umicraft_modhost::ModId = change.mod_id.parse()?;
    host.set_key(&id, &change.key, &change.binding)?;
    Ok(id)
}

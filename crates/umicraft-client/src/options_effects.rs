// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the options do outside their screen (ADR-0043): the settings the
//! server is told, the video options applied to the window and renderer, and
//! the resource pack list the loaders get.
//!
//! The window calls these from its frame and from the moment the options
//! close; each piece is kept small and pure where it can be, so it is tested
//! without a GPU or a window.

use std::path::{Path, PathBuf};

use umicraft_assets::cache::data_dir;
use umicraft_render::Renderer;
use winit::window::{Fullscreen, Window};

use crate::entity_render::EntityTextures;
use crate::game::GameEvent;
use crate::options::{Options, VideoOptions, PACKS_FOLDER};

/// The options of `Client Settings` (0x15): what the server learns about the
/// player. Compared before and after the options screen to decide whether to
/// send the packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerSettings {
    /// The render distance in chunks.
    pub view_distance: u8,
    /// The chat visibility byte.
    pub chat_mode: u8,
    /// Whether the chat shows colours.
    pub chat_colors: bool,
    /// The skin-parts byte (`SkinOptions::parts_mask`).
    pub skin_parts: u8,
}

impl ServerSettings {
    /// The settings `options` select.
    #[must_use]
    pub fn of(options: &Options) -> Self {
        Self {
            view_distance: options.video.render_distance,
            chat_mode: options.chat.visibility.wire(),
            chat_colors: options.chat.colors,
            skin_parts: options.skin.parts_mask(),
        }
    }

    /// The event that sends these settings from the game thread.
    #[must_use]
    pub fn event(self) -> GameEvent {
        GameEvent::ClientSettings {
            view_distance: self.view_distance,
            chat_mode: self.chat_mode,
            chat_colors: self.chat_colors,
            skin_parts: self.skin_parts,
        }
    }
}

/// The folder the resource packs live in, when the platform has a data
/// directory.
#[must_use]
pub fn packs_folder() -> Option<PathBuf> {
    data_dir().map(|folder| folder.join(PACKS_FOLDER))
}

/// The packs to load, lowest priority first.
///
/// A `--resource-pack` on the command line wins over the saved list, as the
/// command-line render distance does over the saved one.
#[must_use]
pub fn pack_paths(cli: &[PathBuf], options: &Options, folder: Option<&Path>) -> Vec<PathBuf> {
    if !cli.is_empty() {
        return cli.to_vec();
    }
    folder.map_or_else(Vec::new, |folder| options.resource_packs.paths_in(folder))
}

/// Opens `folder` in the file manager of the platform (the "Open resource
/// pack folder" button). A failure is logged; the screen goes on.
pub fn open_folder(folder: &Path) {
    if let Err(error) = std::fs::create_dir_all(folder) {
        tracing::warn!(folder = %folder.display(), %error, "could not create the packs folder");
        return;
    }
    let opener = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if let Err(error) = std::process::Command::new(opener).arg(folder).spawn() {
        tracing::warn!(opener, %error, "could not open the packs folder");
    }
}

/// Applies the video options that act on the window, the renderer and the
/// GUI, once per frame. It remembers what it already did, so a steady option
/// costs nothing.
#[derive(Debug, Default)]
pub struct VideoApplier {
    fullscreen: bool,
    /// The mip levels the block atlas was last built with, once known.
    mipmap_levels: Option<u32>,
}

/// What a changed video option asks the window to redo.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct VideoEffects {
    /// An option the mesh workers read changed: re-mesh the loaded columns.
    pub remesh: bool,
    /// The mip levels changed: rebuild the block atlas (the reference
    /// reloads the texture map the same way).
    pub reload_atlas: bool,
}

impl VideoApplier {
    /// The fullscreen state to switch to, or `None` when `wanted` is what is
    /// already set.
    fn fullscreen_change(&mut self, wanted: bool) -> Option<bool> {
        (self.fullscreen != wanted).then(|| {
            self.fullscreen = wanted;
            wanted
        })
    }

    /// Whether the mip levels differ from the ones the atlas was built with.
    /// The first call only records them: the atlas was built from the same
    /// options at start-up.
    fn mipmap_change(&mut self, levels: u32) -> bool {
        self.mipmap_levels
            .replace(levels)
            .is_some_and(|previous| previous != levels)
    }

    /// Applies `video` for this frame: the GUI scale, vsync, fullscreen and
    /// the entity shadows. Brightness, bobbing and the cloud mode are read
    /// where they are used (the render environment, the camera and the cloud
    /// pass). Returns what the window has to redo.
    #[must_use]
    pub fn apply(
        &mut self,
        video: &VideoOptions,
        window: Option<&Window>,
        renderer: &mut Renderer,
        entities: Option<&EntityTextures>,
    ) -> VideoEffects {
        umicraft_ui::set_gui_scale_option(u32::from(video.gui_scale));
        renderer.set_vsync(video.vsync);
        if let Some(entities) = entities {
            entities.set_shadows_enabled(video.entity_shadows);
        }
        if let (Some(window), Some(fullscreen)) = (window, self.fullscreen_change(video.fullscreen))
        {
            // Borderless on the current monitor: no video mode switch, so
            // the change is instant and safe on every platform.
            window.set_fullscreen(fullscreen.then_some(Fullscreen::Borderless(None)));
        }
        let levels = u32::from(video.mipmap_levels);
        renderer.set_mipmap_levels(levels);
        VideoEffects {
            remesh: crate::mesh_settings::set_smooth_lighting(video.smooth_lighting),
            reload_atlas: self.mipmap_change(levels),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::SkinPart;

    #[test]
    fn the_server_settings_follow_the_options() {
        let mut options = Options::default();
        options.video.render_distance = 12;
        options.chat.colors = false;
        options.skin.toggle(SkinPart::Hat);
        let settings = ServerSettings::of(&options);
        assert_eq!(settings.view_distance, 12);
        assert!(!settings.chat_colors);
        assert_eq!(settings.skin_parts, 0b0011_1111);
    }

    #[test]
    fn a_skin_change_alone_changes_the_server_settings() {
        let before = Options::default();
        let mut after = before.clone();
        assert_eq!(ServerSettings::of(&before), ServerSettings::of(&after));
        after.skin.toggle(SkinPart::Cape);
        assert_ne!(ServerSettings::of(&before), ServerSettings::of(&after));
    }

    #[test]
    fn the_event_carries_every_field() {
        let settings = ServerSettings {
            view_distance: 6,
            chat_mode: 2,
            chat_colors: false,
            skin_parts: 5,
        };
        let GameEvent::ClientSettings {
            view_distance,
            chat_mode,
            chat_colors,
            skin_parts,
        } = settings.event()
        else {
            panic!("not a ClientSettings event");
        };
        assert_eq!(
            (view_distance, chat_mode, chat_colors, skin_parts),
            (6, 2, false, 5)
        );
    }

    #[test]
    fn the_command_line_packs_win_over_the_saved_list() {
        let mut options = Options::default();
        options.resource_packs.enabled = vec!["saved.zip".to_string()];
        let folder = Path::new("/data/resourcepacks");
        let cli = vec![PathBuf::from("cli_pack")];
        assert_eq!(pack_paths(&cli, &options, Some(folder)), cli);
        assert_eq!(
            pack_paths(&[], &options, Some(folder)),
            [folder.join("saved.zip")]
        );
        assert!(pack_paths(&[], &options, None).is_empty());
    }

    #[test]
    fn a_new_mipmap_level_asks_for_an_atlas_reload_once() {
        let mut applier = VideoApplier::default();
        assert!(!applier.mipmap_change(4), "the first frame only records");
        assert!(!applier.mipmap_change(4));
        assert!(applier.mipmap_change(2));
        assert!(!applier.mipmap_change(2));
        assert!(applier.mipmap_change(0));
    }

    #[test]
    fn fullscreen_is_only_switched_when_it_changes() {
        let mut applier = VideoApplier::default();
        assert_eq!(applier.fullscreen_change(false), None);
        assert_eq!(applier.fullscreen_change(true), Some(true));
        assert_eq!(applier.fullscreen_change(true), None);
        assert_eq!(applier.fullscreen_change(false), Some(false));
    }
}

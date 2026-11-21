// SPDX-License-Identifier: MIT OR Apache-2.0

//! The fonts, atlases and textures the window loads, some in the background.

use std::sync::Arc;

use umicraft_ui::FontAtlas;

use crate::resources::game_files::GameFiles;
use crate::resources::item_icons::ItemResources;
use crate::resources::sky::CustomSky;
use crate::resources::texture_animator::TextureAnimator;
use crate::resources::theme_font::FontChoice;
use crate::resources::ui_font::LoadedFont;
use crate::scene::entity::EntityTextures;
use crate::scene::item_world::ItemLayers;
use crate::scene::map::MapTextures;
use crate::window::startup::AsyncLoadedAssets;

/// The fonts, atlases and textures loaded for the session.
pub(in crate::window) struct Assets {
    /// The font loaded at startup, before the renderer exists.
    pub(in crate::window) font: LoadedFont,
    /// The active packs over the jar, which the sky, entity and map
    /// textures are read from.
    pub(in crate::window) files: GameFiles,
    /// The block-array layers of the leaf textures ("Trees: Fast").
    pub(in crate::window) leaf_layers: Vec<u32>,
    /// Background asset loader channel receiver.
    pub(in crate::window) receiver: Option<std::sync::mpsc::Receiver<AsyncLoadedAssets>>,
    /// The atlas the renderer accepted; the UI metrics must match it.
    pub(in crate::window) ui_atlas: Option<Arc<FontAtlas>>,
    /// Which font `ui_atlas` was built for, so a theme or scale change swaps it.
    pub(in crate::window) font_choice: FontChoice,
    /// The brand atlas the renderer accepted; `None` keeps the classic
    /// title screen (ADR-0056).
    pub(in crate::window) brand_atlas: Option<FontAtlas>,
    /// The GUI texture atlas for inventory/container backgrounds.
    pub(in crate::window) gui_atlas: Option<umicraft_assets::GuiAtlas>,
    /// The baked item models, once the official assets are bound.
    pub(in crate::window) items: Option<std::sync::Arc<ItemResources>>,
    /// The first-person held item geometry of the frame.
    pub(in crate::window) hand_layers: ItemLayers,
    /// Steps the water, lava and fire textures once per game tick.
    pub(in crate::window) texture_animator: TextureAnimator,
    /// Entity textures bound for GPU rendering.
    pub(in crate::window) entity_textures: Option<EntityTextures>,
    /// The pack-defined sky layers (Custom Sky) and their per-frame state.
    pub(in crate::window) custom_sky: CustomSky,
    /// The pack's custom sky, fog and lightmap colours.
    pub(in crate::window) world_colors: crate::resources::custom_colors::WorldColors,
    /// The textures of filled maps, created with the entity textures.
    pub(in crate::window) map_textures: Option<MapTextures>,
}

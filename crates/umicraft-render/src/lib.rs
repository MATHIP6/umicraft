// SPDX-License-Identifier: MIT OR Apache-2.0

//! Umicraft renderer: wgpu window/offscreen targets, terrain pipeline, a
//! block texture array, a free camera, a debug overlay, and the immediate-mode
//! UI pass.
//!
//! The renderer deliberately owns no Mojang assets: the block array starts as
//! procedurally generated textures and [`Renderer::set_block_atlas`] accepts
//! pixels decoded at runtime from the user's installation, which are never
//! bundled with this crate. The overlay font is built in. Meshes are produced
//! by `umicraft-mesh` and uploaded per chunk column. The UI layer lives in the
//! pure `umicraft-ui` crate; this crate only uploads its [`ui_pass::UiPass`]
//! vertices and font atlas.

mod adapter;
mod block_atlas;
mod border;
mod camera;
mod capture;
mod chunk_pass;
mod clouds;
mod dynamic_blocks;
pub mod entity_pass;
mod environment;
mod error;
mod fog;
mod font;
mod gpu_timer;
pub mod hud;
pub mod hud_hearts;
mod items;
mod lightmap;
mod mipmap;
mod nametag;
mod outline;
mod overlay;
mod overlays;
mod panorama;
mod particles;
mod renderer;
mod sky;
mod texture;
mod ui_pass;
mod weather;

pub use adapter::AdapterSummary;
pub use block_atlas::BlockAtlasError;
pub use border::{BorderStatus, BorderView};
pub use camera::{Camera, Frustum};
pub use clouds::CloudMode;
pub use entity_pass::{EntityBatch, EntityRenderer, EntityTexture, EntityVertex};
pub use environment::RenderEnvironment;
pub use error::RenderError;
pub use fog::{light_brightness, FogContext, FogMedium, FogMode, FogParams};
pub use gpu_timer::{GpuTimings, GROUP_NAMES};
pub use hud::{chat_run_at, draw_hud_gui, HudSheets};
pub use hud::{
    plain_text, ActionBarView, BossView, ChatInputView, ChatLineView, ChatRunView, ChatStyle,
    DebugF3View, HudState, SidebarLineView, SidebarView, TabEntryView, TabListView, TabValueStyle,
    TitleView,
};
pub use hud_hearts::{HeartsTint, HeartsView};
pub use items::{
    IconProvider, NoIcons, ICON_ATLAS_SIZE, ICON_CELL_SIZE, PORTRAIT_KEY, PORTRAIT_SIZE,
};
pub use nametag::Nametag;
pub use outline::{edge_vertices, OutlineBox, OUTLINE_GROWTH};
pub use overlay::{Overlay, OverlayVertex};
pub use overlays::{Portal, ScreenOverlays, Underwater};
pub use particles::{ParticleSprite, MAX_SPRITES};
pub use renderer::{Image, Renderer};
pub use sky::{SkyImage, SkyState, SkyTextures};
pub use texture::{fallback_texture_data, LAYER_BYTES, TEXTURE_SIZE};
pub use ui_pass::{fallback_atlas, FontAtlasError, UiBatch, UiBatchKind, UiPass};
pub use umicraft_ui::screens::gui_scale;
pub use umicraft_ui::SpriteSheet;
pub use weather::{PrecipitationCell, PrecipitationField, FIELD_RADIUS};

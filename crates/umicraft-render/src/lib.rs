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

mod block_atlas;
mod camera;
pub mod entity_pass;
mod environment;
mod error;
mod font;
mod hud;
mod items;
mod outline;
mod overlay;
mod renderer;
mod sky;
mod texture;
mod ui_pass;

pub use block_atlas::BlockAtlasError;
pub use camera::Camera;
pub use entity_pass::{EntityBatch, EntityRenderer, EntityTexture, EntityVertex};
pub use environment::RenderEnvironment;
pub use error::RenderError;
pub use hud::{
    draw as draw_hud, selected_index, status_fills, Fill, HudLayout, HudState, Rect, StatusRow,
};
pub use items::{IconProvider, NoIcons, ICON_ATLAS_SIZE, ICON_CELL_SIZE};
pub use outline::{edge_vertices, OutlineBox, OUTLINE_GROWTH};
pub use overlay::{Overlay, OverlayVertex};
pub use renderer::{Image, Renderer};
pub use sky::SkyState;
pub use texture::{fallback_texture_data, LAYER_BYTES, TEXTURE_SIZE};
pub use ui_pass::{fallback_atlas, FontAtlasError, UiBatch, UiBatchKind, UiPass};

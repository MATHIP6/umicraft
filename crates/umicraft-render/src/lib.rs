// SPDX-License-Identifier: MIT OR Apache-2.0

//! Umicraft renderer: wgpu window/offscreen targets, terrain pipeline,
//! procedurally generated block textures, a free camera, a debug overlay, and
//! the immediate-mode UI pass.
//!
//! The renderer deliberately owns no Mojang assets: block textures are
//! generated at startup and the overlay font is built in. Meshes are produced
//! by `umicraft-mesh` and uploaded per chunk column. The UI layer lives in the
//! pure `umicraft-ui` crate; this crate only uploads its [`ui_pass::UiPass`]
//! vertices and font atlas.

mod camera;
mod error;
mod font;
mod hud;
mod overlay;
mod renderer;
mod texture;
mod ui_pass;

pub use camera::Camera;
pub use error::RenderError;
pub use hud::{
    draw as draw_hud, selected_index, status_fills, Fill, HudLayout, HudState, ItemStack, Rect,
    StatusRow,
};
pub use overlay::{Overlay, OverlayVertex};
pub use renderer::{Image, Renderer};
pub use texture::{fallback_texture_data, LAYER_BYTES, TEXTURE_SIZE};
pub use ui_pass::{fallback_atlas, FontAtlasError, UiPass};

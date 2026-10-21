// SPDX-License-Identifier: MIT OR Apache-2.0

//! Umicraft renderer: wgpu window/offscreen targets, terrain pipeline,
//! procedurally generated block textures, a free camera, and a debug overlay.
//!
//! The renderer deliberately owns no Mojang assets: block textures are
//! generated at startup and the overlay font is built in. Meshes are produced
//! by `umicraft-mesh` and uploaded per chunk column.

mod camera;
mod error;
mod font;
mod overlay;
mod renderer;
mod texture;

pub use camera::Camera;
pub use error::RenderError;
pub use overlay::{Overlay, OverlayVertex};
pub use renderer::{Image, Renderer};
pub use texture::{fallback_texture_data, LAYER_BYTES, TEXTURE_SIZE};

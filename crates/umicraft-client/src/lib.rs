// SPDX-License-Identifier: MIT OR Apache-2.0

//! # umicraft-client
//!
//! M3 client: a network thread that owns the socket, a game thread that owns
//! the world and the 20 Hz simulation, and a render thread that draws the
//! player camera and the HUD (ADR-0005).
//!
//! The crate is also usable as a library for the acceptance driver and tests:
//! [`args`] parses the CLI, [`network`] owns the connection, [`game`] owns the
//! world and the player, [`input`] turns the keys into the intent of the
//! game logic, [`mesher`] runs the CPU mesher pool, [`headless`] plays a
//! scripted session without a window, [`screenshot`] writes a PNG,
//! [`ui_demo`] drives the M4 debug screen without a window or a server,
//! [`block_demo`] proves the M5 model/atlas path without a server, and
//! [`ui_font`] loads the user's 1.8 font or the built-in fallback.

pub mod args;
pub mod block_demo;
pub mod chat;
pub mod cracks;
pub mod creative_input;
mod creative_layout;
mod creative_screen;
pub mod creative_view;
pub mod dials;
pub mod entity_demo;
pub mod entity_render;
pub mod error;
pub mod game;
pub mod headless;
pub mod hud_demo;
pub mod input;
pub mod inventory_input;
mod inventory_screen;
mod inventory_winit;
pub mod item_icons;
pub mod item_tints;
pub mod item_world;
pub mod mesher;
pub mod model_resources;
pub mod nametag_draw;
pub mod network;
mod official;
pub mod options;
pub mod options_screen;
pub mod screen;
pub mod profiles;
pub mod screenshot;
pub mod setup_cli;
pub mod terrain_textures;
#[cfg(test)]
mod test_support;
pub mod texture_animator;
mod tooltip_draw;
pub mod ui_demo;
pub mod ui_font;
pub mod ui_input;
pub mod window;
pub mod world_view;

pub use error::ClientError;

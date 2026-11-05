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

mod anvil_field;
pub mod args;
pub mod block_demo;
mod block_entity_render;
mod border_view;
mod caption;
pub mod chat;
pub mod cracks;
pub mod creative_input;
mod creative_layout;
mod creative_screen;
pub mod creative_view;
mod debug_f3_view;
pub mod debug_world;
pub mod dials;
pub mod entity_demo;
mod entity_icons;
pub mod entity_render;
pub mod error;
mod fog_context;
pub mod game;
pub mod headless;
pub mod hud_demo;
mod hud_demo_panorama;
mod hud_demo_servers;
mod hud_view;
pub mod input;
pub mod inventory_input;
mod inventory_screen;
mod inventory_winit;
pub mod item_icons;
pub mod item_tints;
pub mod item_world;
mod language;
pub mod map_render;
mod menu_flow;
mod menu_rows;
pub mod mesher;
pub mod model_resources;
pub mod moving_blocks_draw;
pub mod nametag_draw;
pub mod network;
mod official;
pub mod options;
pub mod options_effects;
pub mod options_screen;
pub mod overlay_state;
pub mod overlay_view;
mod panorama_faces;
pub mod particle_ambient;
pub mod particle_view;
pub(crate) mod portrait;
mod process_memory;
pub mod profiles;
pub mod screen;
pub mod screenshot;
pub mod servers;
pub mod setup_cli;
mod sky_textures;
mod social_view;
pub mod terrain_textures;
#[cfg(test)]
mod test_support;
mod text_screens;
pub mod texture_animator;
mod tooltip_draw;
pub mod ui_demo;
pub mod ui_font;
pub mod ui_input;
pub mod weather_view;
pub mod window;
pub mod world_view;

pub use error::ClientError;

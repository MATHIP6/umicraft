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
//! game logic, [`headless`] plays a scripted session without a window and
//! [`screenshot`] writes a PNG.
//!
//! # Layout
//!
//! * The threads: [`network`], [`game`], [`window`] and the render thread's
//!   [`world_view`]; [`headless`] runs the game thread without a window.
//! * What the window draws: [`screens`] (menus, inventories, options), [`hud`],
//!   [`debug`] (F3 and the frame metrics) and [`scene`] (entities, items,
//!   particles, weather, overlays); [`meshing`] builds the chunk meshes and
//!   [`resources`] loads the textures, models and fonts.
//! * [`mods`] is the client side of the built-in mods.
//! * [`demos`] holds the server-less runs (`--block-demo`, `--hud-demo`, ...).
//! * The rest are the session's settings and services: [`options`],
//!   [`servers`], [`profiles`], [`audio_bridge`], [`discord`], the command line
//!   ([`args`], [`launch_session`], [`setup_cli`]) and the errors.

mod accounts;
pub mod args;
pub mod audio_bridge;
pub(crate) mod discord;
pub mod error;
pub mod game;
pub mod headless;
pub mod input;
mod language;
pub mod launch_session;
pub(crate) mod network;
pub(crate) mod options;
pub(crate) mod pack_offer;
pub(crate) mod profiles;
pub mod screenshot;
pub(crate) mod servers;
pub mod setup_cli;
#[cfg(test)]
mod test_support;
pub mod thread_priority;
pub mod timer_resolution;
pub mod window;
pub mod world_view;

pub mod debug;
pub mod demos;
pub mod hud;
pub(crate) mod meshing;
pub(crate) mod mods;
pub mod resources;
pub mod scene;
pub(crate) mod screens;

pub use error::ClientError;

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
//! scripted session without a window, [`screenshot`] writes a PNG, and
//! [`ui_font`] loads the user's 1.8 font or the built-in fallback.

pub mod args;
pub mod chat;
pub mod error;
pub mod game;
pub mod headless;
pub mod input;
pub mod mesher;
pub mod network;
pub mod screenshot;
pub mod ui_font;
pub mod ui_input;
pub mod window;

pub use error::ClientError;

// SPDX-License-Identifier: MIT OR Apache-2.0

//! # umicraft-client
//!
//! M2 graphical client: streams play packets on a network thread, applies
//! chunks/blocks on the render thread, meshes chunk snapshots in a worker pool,
//! and draws with wgpu either to a winit window or to an offscreen screenshot.
//!
//! The crate is also usable as a library for the acceptance driver and tests:
//! [`args`] parses the CLI, [`network`] owns the connection, [`scene`] applies
//! events, [`mesher`] runs the CPU mesher pool, and [`screenshot`] is the
//! headless pipeline.

pub mod args;
pub mod error;
pub mod mesher;
pub mod network;
pub mod scene;
pub mod screenshot;
pub mod window;

pub use error::ClientError;

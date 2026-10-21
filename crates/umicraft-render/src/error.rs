// SPDX-License-Identifier: MIT OR Apache-2.0

//! Renderer errors.

use thiserror::Error;

/// Errors from GPU or target initialization and readback.
#[derive(Debug, Error)]
pub enum RenderError {
    /// No GPU adapter matched the requested options.
    #[error("no suitable GPU adapter: {0}")]
    NoAdapter(String),
    /// Logical device creation failed.
    #[error("failed to create GPU device: {0}")]
    NoDevice(String),
    /// The window surface could not be created.
    #[error("failed to create window surface: {0}")]
    Surface(String),
    /// The adapter does not support a default surface configuration.
    #[error("surface has no default configuration")]
    NoSurfaceConfig,
    /// The surface texture was invalidated by the backend.
    #[error("surface texture is no longer valid")]
    SurfaceLost,
    /// GPU readback failed.
    #[error("GPU readback failed: {0}")]
    Readback(String),
    /// A readback was requested from a windowed renderer.
    #[error("readback requires an offscreen renderer")]
    NotOffscreen,
}

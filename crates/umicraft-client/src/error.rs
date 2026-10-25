// SPDX-License-Identifier: MIT OR Apache-2.0

//! Client error types.

use thiserror::Error;

use umicraft_render::{FontAtlasError, RenderError};
use umicraft_world::WorldError;

use crate::model_resources::ModelResourcesError;

/// Fatal client failure.
#[derive(Debug, Error)]
pub enum ClientError {
    /// Network setup or server-reported failure.
    #[error("network: {0}")]
    Network(String),
    /// PNG encoding or output-path failure.
    #[error("screenshot: {0}")]
    Screenshot(String),
    /// Window/event-loop failure.
    #[error("window: {0}")]
    Window(String),
    /// World decode/application failure.
    #[error("world: {0}")]
    World(#[from] WorldError),
    /// Renderer initialization, draw, or readback failure.
    #[error(transparent)]
    Render(#[from] RenderError),
    /// The renderer rejected both the loaded font atlas and the fallback.
    #[error("font atlas: {0}")]
    FontAtlas(#[from] FontAtlasError),
    /// The GPU rejected the official block texture array where no procedural
    /// fallback exists (`--block-demo`); the warning logged by the terrain
    /// installation carries the reason.
    #[error("block atlas: the GPU rejected the official texture array (see the warning above)")]
    OfficialTexturesRejected,
    /// The official block models or atlas could not be loaded.
    #[error("block resources: {0}")]
    ModelResources(#[from] ModelResourcesError),
    /// Filesystem failure while writing a screenshot.
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    /// No world/join information arrived before the screenshot deadline.
    #[error("no world was received before the deadline")]
    NoWorld,
    /// The world arrived but contained no chunks.
    #[error("no chunks were received before the deadline")]
    NoChunks,
    /// Mesh worker thread could not be started.
    #[error("mesh worker: {0}")]
    Worker(String),
    /// Asset downloading, verification or caching failure.
    #[error("assets: {0}")]
    Assets(String),
    /// A worker or network thread panicked.
    #[error("background thread failed: {0}")]
    Thread(String),
}

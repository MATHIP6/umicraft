// SPDX-License-Identifier: MIT OR Apache-2.0

//! Typed errors for the model pipeline.
//!
//! Every failure carries the virtual resource path that caused it, so a broken
//! resource pack is reported in terms of the file the user can fix rather than
//! an internal model key.

/// A model or resource failure.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    /// A resource source could not provide a file.
    #[error("resource {path}: {message}")]
    Resource {
        /// Virtual path, e.g. `assets/minecraft/models/block/stone.json`.
        path: String,
        /// Human-readable reason.
        message: String,
    },
    /// A resource is not valid JSON.
    #[error("invalid JSON in {path}: {source}")]
    Json {
        /// Virtual path of the document.
        path: String,
        /// Parser error.
        #[source]
        source: serde_json::Error,
    },
    /// A document is valid JSON but not a valid 1.8 blockstate or model.
    #[error("invalid model data in {path}: {message}")]
    Invalid {
        /// Virtual path of the document.
        path: String,
        /// What is wrong.
        message: String,
    },
    /// A texture reference could not be resolved to a concrete texture key.
    #[error("unresolved texture reference {reference} in {path}")]
    UnresolvedTexture {
        /// The `#` reference as written in the model.
        reference: String,
        /// Model that contains the reference.
        path: String,
    },
    /// The `parent` links of a model form a cycle.
    #[error("parent cycle at model {path}")]
    ParentCycle {
        /// Model key (relative to `models/block`) that closes the cycle.
        path: String,
    },
    /// A `parent` chain is longer than the supported limit.
    #[error("parent chain through {path} exceeds the depth limit of {limit}")]
    ParentDepth {
        /// Model key (relative to `models/block`) that exceeded the limit.
        path: String,
        /// Maximum accepted chain length.
        limit: usize,
    },
}

/// Result alias for the model pipeline.
pub type ModelResult<T> = Result<T, ModelError>;

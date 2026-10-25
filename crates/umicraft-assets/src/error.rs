// SPDX-License-Identifier: MIT OR Apache-2.0

//! Typed errors for the asset transport layer.
//!
//! Everything that touches the network, the filesystem, or a downloaded byte
//! stream is untrusted input (spec §14.3), so every failure mode is a typed
//! variant of [`AssetsError`] instead of a panic or an `unwrap`.

/// Error type for every fallible operation in `umicraft-assets`.
#[derive(Debug, thiserror::Error)]
pub enum AssetsError {
    /// The HTTP request itself failed (DNS, TLS, connection reset, truncated
    /// body, ...).
    #[error("http request to `{url}` failed: {source}")]
    Http {
        /// The URL that was requested.
        url: String,
        /// Underlying transport error.
        #[source]
        source: reqwest::Error,
    },

    /// The server answered with a non-success status code.
    #[error("http request to `{url}` returned status {status}")]
    HttpStatus {
        /// The URL that was requested.
        url: String,
        /// The status code the server replied with.
        status: u16,
    },

    /// A JSON document could not be parsed, or did not have the expected shape.
    #[error("invalid json in `{what}`: {source}")]
    Json {
        /// What was being parsed (a URL or a file path), for diagnostics.
        what: String,
        /// Underlying parse error.
        #[source]
        source: serde_json::Error,
    },

    /// A file could not be read or written.
    #[error("io error on `{path}`: {source}")]
    Io {
        /// The path that failed, for diagnostics.
        path: String,
        /// Underlying filesystem error.
        #[source]
        source: std::io::Error,
    },

    /// The downloaded bytes do not have the expected SHA-1.
    #[error("sha1 mismatch for `{what}`: expected `{expected}`, got `{actual}`")]
    HashMismatch {
        /// What was hashed (usually an object hash).
        what: String,
        /// The hash the index promised.
        expected: String,
        /// The hash the bytes actually have.
        actual: String,
    },

    /// The downloaded bytes do not have the expected length.
    #[error("size mismatch for `{what}`: expected {expected} bytes, got {actual}")]
    SizeMismatch {
        /// What was measured.
        what: String,
        /// The size the index promised.
        expected: u64,
        /// The size the bytes actually have.
        actual: u64,
    },

    /// The requested game version is not listed in the version manifest.
    #[error("game version `{0}` is not in the version manifest")]
    UnknownVersion(String),

    /// An asset index entry carries a hash that is not a SHA-1 digest.
    #[error("asset `{path}` carries the malformed hash `{hash}`")]
    InvalidHash {
        /// The logical asset path of the entry.
        path: String,
        /// The hash that was rejected.
        hash: String,
    },

    /// A referenced file is missing on disk.
    #[error("`{what}` is missing (looked at `{path}`)")]
    Missing {
        /// What was being looked for.
        what: String,
        /// Where it was expected.
        path: String,
    },

    /// The target of the operation is not writable by design.
    ///
    /// Used by [`crate::install::OfficialInstall`]: an existing official
    /// installation is only ever read (spec AST-2), never written to.
    #[error("`{0}` is read-only")]
    ReadOnly(String),

    /// A zip container could not be read.
    #[error("zip container error in `{what}`: {reason}")]
    Zip {
        /// The container that failed.
        what: String,
        /// Why it failed.
        reason: String,
    },

    /// A resource pack was invalid, corrupt or could not be loaded.
    #[error("resource pack `{name}` is invalid: {reason}")]
    Pack {
        /// Name or path of the pack.
        name: String,
        /// Description of the failure.
        reason: String,
    },

    /// The platform data directory could not be determined.
    #[error("the platform data directory could not be determined")]
    NoDataDir,

    /// A background download task panicked or was cancelled.
    #[error("a download task failed: {0}")]
    TaskFailed(String),

    /// A texture is not a decodable PNG or has an invalid size.
    #[error("texture `{path}` is invalid: {reason}")]
    Image {
        /// Jar entry or path of the image.
        path: String,
        /// Why it was rejected.
        reason: String,
    },

    /// A blockstate/model or atlas layout failed.
    #[error(transparent)]
    Model(#[from] umicraft_model::ModelError),
}

/// Convenience alias for results carrying [`AssetsError`].
pub type AssetsResult<T> = Result<T, AssetsError>;

/// Builds an [`AssetsError::Image`] with a consistent diagnostic.
pub(crate) fn image_error(path: &str, reason: impl Into<String>) -> AssetsError {
    AssetsError::Image {
        path: path.to_string(),
        reason: reason.into(),
    }
}

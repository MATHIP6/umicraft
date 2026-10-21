// SPDX-License-Identifier: MIT OR Apache-2.0

//! Typed errors for `umicraft-core`.
//!
//! All fallible core operations surface errors through [`CoreError`] instead of
//! panicking: server/file/mod-shaped input is untrusted (spec §14.3), and the
//! core must degrade into a clean error path rather than abort the client.
//!
//! ```
//! use umicraft_core::CoreError;
//!
//! let err = CoreError::UnknownService("render.mesher".to_string());
//! assert!(err.to_string().contains("render.mesher"));
//! ```

/// The unified error type returned by core runtime operations.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// Loading a module found an id that is already registered.
    #[error("duplicate module id `{0}`")]
    DuplicateModule(String),

    /// A module's `on_load` hook failed; the core may hold partially
    /// initialized state, so the caller should discard it.
    #[error("module `{module}` failed to load")]
    LoadFailed {
        /// Id of the module whose hook failed.
        module: String,
        /// The underlying error raised by the hook.
        #[source]
        source: Box<CoreError>,
    },

    /// Registering a service whose name already exists.
    #[error("service `{service}` is already provided by `{existing}`")]
    ServiceConflict {
        /// Name of the clashing service.
        service: String,
        /// Provider that already owns the name.
        existing: String,
    },

    /// Looking up a service name that was never registered.
    #[error("unknown service `{0}`")]
    UnknownService(String),

    /// Looking up a service registered under a different concrete type.
    #[error("service `{service}` holds `{found}`, caller expected `{expected}`")]
    ServiceTypeMismatch {
        /// Name of the service.
        service: String,
        /// The type the caller requested, as spelled by `type_name`.
        expected: &'static str,
        /// The type actually stored, as spelled by `type_name`.
        found: &'static str,
    },

    /// Calling [`crate::Core::start`] (or adding modules) after startup finished.
    #[error("core has already been started; modules load exactly once")]
    AlreadyStarted,

    /// A configuration file could not be read from disk.
    #[error("cannot read configuration file `{path}`")]
    ConfigIo {
        /// Path that failed, for diagnostics only.
        path: String,
        /// Underlying filesystem error.
        #[source]
        source: std::io::Error,
    },

    /// A configuration file parsed but contained invalid TOML.
    #[error("invalid configuration: {0}")]
    ConfigParse(#[from] toml::de::Error),

    /// A configuration could not be serialized when writing it out.
    #[error("configuration could not be serialized: {0}")]
    ConfigSerialize(#[from] toml::ser::Error),
}

/// Convenience alias for results carrying [`CoreError`].
///
/// ```
/// use umicraft_core::error::CoreResult;
///
/// fn checked_div(a: u32, b: u32) -> CoreResult<u32> {
///     if b == 0 {
///         Err(umicraft_core::CoreError::AlreadyStarted)
///     } else {
///         Ok(a / b)
///     }
/// }
/// ```
pub type CoreResult<T> = Result<T, CoreError>;

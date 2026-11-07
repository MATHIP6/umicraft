// SPDX-License-Identifier: MIT OR Apache-2.0

//! CPU profiling spans (F2-0a, ADR-0059).
//!
//! With the `profiling` cargo feature, [`profile_scope!`](crate::profile_scope)
//! records a named span with [puffin](https://crates.io/crates/puffin) and
//! [`serve`] streams the data to `puffin_viewer`. Without the feature the macro
//! expands to nothing and [`serve`] is a no-op, so release builds pay nothing.
//!
//! ```
//! umicraft_core::profile_scope!("example");
//! umicraft_core::profiling::new_frame();
//! ```

/// Opens a span named `$name` that ends at the end of the enclosing block.
///
/// The name must be a string literal (puffin stores the pointer, not a copy).
#[macro_export]
macro_rules! profile_scope {
    ($name:expr) => {
        #[cfg(feature = "profiling")]
        $crate::profiling::puffin::profile_scope!($name);
    };
}

/// Re-export so [`profile_scope!`] works in crates that do not depend on puffin.
#[cfg(feature = "profiling")]
#[doc(hidden)]
pub use puffin;

/// Marks the end of a frame so the viewer can split the timeline.
///
/// Call once per rendered frame (or game tick for the game thread).
pub fn new_frame() {
    #[cfg(feature = "profiling")]
    puffin::GlobalProfiler::lock().new_frame();
}

/// Turns the profiler on and serves its data on `127.0.0.1:8585`, the
/// `puffin_viewer` default.
///
/// Returns the server guard; dropping it stops the stream. `None` when the
/// feature is off or the port is busy (a warning is logged in that case).
#[must_use]
pub fn serve() -> Option<Box<dyn std::any::Any>> {
    #[cfg(feature = "profiling")]
    {
        match puffin_http::Server::new("127.0.0.1:8585") {
            Ok(server) => {
                puffin::set_scopes_on(true);
                tracing::info!("puffin server on 127.0.0.1:8585 (run `puffin_viewer`)");
                return Some(Box::new(server));
            }
            Err(error) => tracing::warn!("cannot start the puffin server: {error}"),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn spans_compile_and_frames_advance_without_a_viewer() {
        crate::profile_scope!("test");
        super::new_frame();
        // Port availability is not guaranteed in CI: only check it never panics.
        drop(super::serve());
    }
}

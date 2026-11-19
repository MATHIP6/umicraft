//! Rust helper for Umicraft mod authors (ADR-0086).
//!
//! A mod implements [`Mod`], registers handlers through [`InitCx`] and exports
//! the C entry points with [`umicraft_mod!`]. All `unsafe` lives in the private
//! `ffi` module (ADR-0087); everything a mod author touches is safe.
//!
//! ```ignore
//! struct Hello;
//!
//! impl umicraft_modapi::Mod for Hello {
//!     fn init(cx: &mut umicraft_modapi::InitCx<'_>) -> Result<Self, umicraft_modapi::ModError> {
//!         let log = cx.logger();
//!         cx.observe::<umicraft_modapi::PreTick>(0, move |e| log.info(&format!("tick {}", e.tick())))?;
//!         Ok(Hello)
//!     }
//! }
//!
//! umicraft_modapi::umicraft_mod!(Hello);
//! ```

mod error;
mod event;
mod ffi;
mod init;
mod logger;
#[doc(hidden)]
pub mod private;

pub use error::ModError;
pub use event::{Control, Event, PostTick, PreTick};
pub use init::{InitCx, Mod};
pub use logger::Logger;

/// Exports `umi_mod_api_version`, `umi_mod_init` and `umi_mod_shutdown` for a
/// type implementing [`Mod`]; use it once per mod library.
///
/// The macro is a thin shim: all logic (panic containment, state, validation)
/// lives in the hidden `private` module so it can be tested and kept out of
/// the mod's own crate.
#[macro_export]
macro_rules! umicraft_mod {
    ($mod_ty:ty) => {
        static UMI_MOD_STATE: $crate::private::ModState<$mod_ty> = $crate::private::ModState::new();

        /// C entry point: API version this mod was built for.
        #[allow(unsafe_code)]
        #[unsafe(no_mangle)]
        pub extern "C" fn umi_mod_api_version() -> $crate::private::UmiVersion {
            $crate::private::api_version()
        }

        /// C entry point: builds the mod and registers its handlers.
        #[allow(unsafe_code)]
        #[unsafe(no_mangle)]
        pub extern "C" fn umi_mod_init(
            host: *const $crate::private::UmiHost,
            me: $crate::private::UmiModHandle,
        ) -> $crate::private::UmiStatus {
            $crate::private::init(&UMI_MOD_STATE, host, me)
        }

        /// C entry point: shuts the mod down and frees its handlers.
        #[allow(unsafe_code)]
        #[unsafe(no_mangle)]
        pub extern "C" fn umi_mod_shutdown() {
            $crate::private::shutdown(&UMI_MOD_STATE)
        }
    };
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The three symbols every mod library exports.

use crate::{UmiHost, UmiModHandle, UmiStatus, UmiVersion};

/// Symbol name of [`UmiModApiVersionFn`], NUL-terminated for `libloading`.
pub const UMI_MOD_API_VERSION_SYMBOL: &[u8] = b"umi_mod_api_version\0";
/// Symbol name of [`UmiModInitFn`], NUL-terminated for `libloading`.
pub const UMI_MOD_INIT_SYMBOL: &[u8] = b"umi_mod_init\0";
/// Symbol name of [`UmiModShutdownFn`], NUL-terminated for `libloading`.
pub const UMI_MOD_SHUTDOWN_SYMBOL: &[u8] = b"umi_mod_shutdown\0";

/// `umi_mod_api_version`: the API version the mod was built for.
pub type UmiModApiVersionFn = extern "C" fn() -> UmiVersion;
/// `umi_mod_init`: registers everything the mod needs through `host`.
pub type UmiModInitFn = extern "C" fn(host: *const UmiHost, me: UmiModHandle) -> UmiStatus;
/// `umi_mod_shutdown`: called once before the library is unloaded.
pub type UmiModShutdownFn = extern "C" fn();

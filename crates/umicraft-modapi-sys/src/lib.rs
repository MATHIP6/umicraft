// SPDX-License-Identifier: MIT OR Apache-2.0

//! The C ABI of the Umicraft modding API (ADR-0087).
//!
//! Everything here is `#[repr(C)]` or `#[repr(transparent)]`, so C, C++, Zig
//! and Rust mods see the same layout. The crate is the single source of truth
//! for `include/umicraft.h` (generated with cbindgen, see `tools/gen-header.sh`).
//!
//! The crate contains no `unsafe`: it only declares types. Reading through the
//! raw pointers is the job of the `ffi` modules of `umicraft-modapi` and
//! `umicraft-modhost`. Structs holding raw pointers or `extern "C"` callbacks
//! are deliberately not `Send`/`Sync`; wrappers that move them across threads
//! must justify that themselves.

mod camera;
mod events;
mod exports;
mod host;
mod services;
mod types;
mod ui;

pub use camera::{UmiCameraCx, UmiCameraFn, UmiCameraInput, UmiRenderCamera};
pub use events::{
    UmiChatReceived, UmiChatSend, UmiConnected, UmiDisconnected, UmiEventHeader, UmiEventId,
    UmiFrame, UmiKeyPressed, UmiPacket, UmiPostTick, UmiPreRender, UmiPreTick, UmiScreen,
    UMI_CHAT_POSITION_ACTION_BAR, UMI_CHAT_POSITION_CHAT, UMI_CHAT_POSITION_SYSTEM,
    UMI_EVENT_CANCELLABLE, UMI_EVENT_CANCELLED, UMI_EVENT_MUTABLE,
};
pub use exports::{
    UmiModApiVersionFn, UmiModInitFn, UmiModShutdownFn, UMI_MOD_API_VERSION_SYMBOL,
    UMI_MOD_INIT_SYMBOL, UMI_MOD_SHUTDOWN_SYMBOL,
};
pub use host::{UmiHandlerFn, UmiHost, UmiLevel, UmiMode};
pub use services::{
    UmiEntity, UmiEntityVisitFn, UmiPhysicsCx, UmiPhysicsFn, UmiPhysicsInput, UmiPlayerPhysics,
    UmiServiceMode, UMI_ENTITY_INVISIBLE, UMI_ENTITY_LIVING, UMI_ENTITY_ON_FIRE,
    UMI_ENTITY_SNEAKING, UMI_ENTITY_SPRINTING, UMI_ENTITY_USING_ITEM, UMI_GAME_MODE_ADVENTURE,
    UMI_GAME_MODE_CREATIVE, UMI_GAME_MODE_SPECTATOR, UMI_GAME_MODE_SURVIVAL, UMI_INPUT_JUMP,
    UMI_INPUT_SNEAK, UMI_INPUT_SPRINT, UMI_PHYSICS_ALLOW_FLYING, UMI_PHYSICS_FLYING,
    UMI_PHYSICS_IN_WATER, UMI_PHYSICS_ON_GROUND, UMI_PHYSICS_SNEAKING, UMI_PHYSICS_SPRINTING,
};
pub use types::{
    UmiBytes, UmiModContext, UmiModHandle, UmiStatus, UmiStr, UmiVersion, UMI_API_VERSION,
};
pub use ui::{
    UmiCommandFn, UmiHudCx, UmiHudDrawFn, UmiHudPlacement, UmiSettingDef, UmiSettingKind,
    UmiSettingValue, UMI_HUD_ANCHOR_SCREEN,
};

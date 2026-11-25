// SPDX-License-Identifier: MIT OR Apache-2.0

//! Settings, HUD widgets, key bindings and client commands (API 0.3, F3.4).
//!
//! A mod declares these during `umi_mod_init` only; the host lists them in
//! the mods menu and the controls page, persists the player's choices and
//! calls back into the mod. Texts are literal UTF-8 (external mods bring
//! their own words, not language keys) and are copied by the host.

use core::ffi::c_void;

use crate::{UmiStatus, UmiStr};

/// Kind of a setting; selects which fields of [`UmiSettingDef`] and
/// [`UmiSettingValue`] are meaningful.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiSettingKind(pub u32);

impl UmiSettingKind {
    /// On/off switch.
    pub const TOGGLE: Self = Self(1);
    /// Number in `[min, max]` moved by `step`.
    pub const SLIDER: Self = Self(2);
    /// One of a list of labelled options.
    pub const CHOICE: Self = Self(3);
    /// An `0xRRGGBB` colour.
    pub const COLOR: Self = Self(4);
}

/// A setting a mod declares with `UmiHost::register_setting`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct UmiSettingDef {
    /// Size of this struct as the mod built it.
    pub size: u32,
    /// Which fields below apply.
    pub kind: UmiSettingKind,
    /// Stable key, unique within the mod (`^[a-z][a-z0-9_]{0,63}$`); the
    /// player's value is saved under it.
    pub key: UmiStr,
    /// Label shown in the mods menu.
    pub label: UmiStr,
    /// `TOGGLE`: default state (0 or 1).
    pub toggle_default: u8,
    /// `SLIDER`: lowest value.
    pub slider_min: f32,
    /// `SLIDER`: highest value.
    pub slider_max: f32,
    /// `SLIDER`: increment, greater than 0.
    pub slider_step: f32,
    /// `SLIDER`: default value, within `[min, max]`.
    pub slider_default: f32,
    /// `CHOICE`: the option labels, `choices_len` of them (at least one).
    pub choices: *const UmiStr,
    /// `CHOICE`: number of options.
    pub choices_len: usize,
    /// `CHOICE`: index of the default option.
    pub choice_default: u32,
    /// `COLOR`: default colour, `0xRRGGBB`.
    pub color_default: u32,
}

/// The current value of a setting, read with `UmiHost::setting_get`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UmiSettingValue {
    /// Kind of the setting; only the matching field below is meaningful.
    pub kind: UmiSettingKind,
    /// `TOGGLE`: 0 or 1.
    pub toggle: u8,
    /// `SLIDER`: the value.
    pub slider: f32,
    /// `CHOICE`: the selected index.
    pub choice: u32,
    /// `COLOR`: `0xRRGGBB`.
    pub color: u32,
}

/// Where a HUD widget sits: an anchor on the screen plus an offset in GUI
/// pixels and a scale, like the built-in widgets (ADR-0065).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UmiHudPlacement {
    /// 0..=8, row by row: top left, top centre, top right, middle left,
    /// centre, middle right, bottom left, bottom centre, bottom right; or
    /// [`UMI_HUD_ANCHOR_SCREEN`] (API 1.1).
    pub anchor: u32,
    /// Horizontal offset from the anchor, in GUI pixels.
    pub offset_x: f32,
    /// Vertical offset from the anchor, in GUI pixels.
    pub offset_y: f32,
    /// Scale, clamped by the host to the built-in widgets' range.
    pub scale: f32,
}

/// [`UmiHudPlacement::anchor`]: the offset is absolute GUI pixels from the
/// top-left corner of the screen, the scale is 1 and the player cannot move the
/// widget; for annotations placed with the `FRAME` projection. Added in API 1.1.
pub const UMI_HUD_ANCHOR_SCREEN: u32 = 9;

/// The host's buffer a widget fills while it is drawn; opaque to mods.
#[repr(C)]
pub struct UmiHudCx {
    _private: [u8; 0],
}

/// Fills a HUD widget for the current frame through the `hud_*` host
/// functions. Called on the render thread, within a time budget.
pub type UmiHudDrawFn = extern "C" fn(hud: *mut UmiHudCx, user: *mut c_void) -> UmiStatus;

/// Runs a client command: `args` is the text after `/<name> `, borrowed for
/// the call. Called on the game thread, within a time budget.
pub type UmiCommandFn = extern "C" fn(args: UmiStr, user: *mut c_void) -> UmiStatus;

/// A module a mod declares with `UmiHost::register_module`: a card of its own in
/// the mods menu, with a switch the player controls. Added in API 1.2.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct UmiModuleDef {
    /// Size of this struct as the mod built it.
    pub size: u32,
    /// [`UMI_MODULE_DEFAULT_ON`] or 0.
    pub flags: u32,
    /// Stable key, unique within the mod (`^[a-z][a-z0-9_]{0,63}$`); the
    /// player's switch is saved under it.
    pub key: UmiStr,
    /// Name of the card.
    pub label: UmiStr,
    /// One-line description of the card; may be empty.
    pub description: UmiStr,
    /// Key of one of the mod's categories (`register_category`), or empty for
    /// the "External" tab.
    pub category: UmiStr,
}

/// [`UmiModuleDef::flags`]: the module is on until the player switches it off.
pub const UMI_MODULE_DEFAULT_ON: u32 = 1;

/// `UmiHost::module_attach` kind: a HUD widget, drawn only while the module is
/// on.
pub const UMI_ATTACH_HUD_WIDGET: u32 = 1;
/// `UmiHost::module_attach` kind: a post-process pass, applied only while the
/// module is on.
pub const UMI_ATTACH_POST_PASS: u32 = 2;
/// `UmiHost::module_attach` kind: a world layer, drawn only while the module is
/// on. Added in API 1.4.
pub const UMI_ATTACH_WORLD_LAYER: u32 = 3;

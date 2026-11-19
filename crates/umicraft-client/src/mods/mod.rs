// SPDX-License-Identifier: MIT OR Apache-2.0

//! The client side of the built-in mods (ADR-0065): the runtime, the mods menu,
//! the HUD widgets and the HUD editor, plus the loader of external native mods
//! (ADR-0086).

pub(crate) mod external;
pub(crate) mod facts;
pub(crate) mod hud;
pub(crate) mod hud_editor;
pub(crate) mod icons;
pub(crate) mod menu;
pub(crate) mod runtime;
pub(crate) mod zoom;

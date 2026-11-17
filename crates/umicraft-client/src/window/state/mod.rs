// SPDX-License-Identifier: MIT OR Apache-2.0

//! The groups of fields of [`App`](super::App), one file each.

mod assets;
mod chat;
mod clock;
mod frame;
mod input;
mod options_menu;
mod presses;

pub(super) use assets::Assets;
pub(super) use chat::ChatUi;
pub(super) use clock::FrameClock;
pub(super) use frame::FrameScratch;
pub(super) use input::InputTracking;
pub(super) use options_menu::OptionsMenu;
pub(super) use presses::PendingPresses;

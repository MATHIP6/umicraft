// SPDX-License-Identifier: MIT OR Apache-2.0

//! The key-binding event, emitted on the render thread.

use umicraft_modapi_sys::{UmiEventHeader, UmiEventId, UmiKeyPressed};

use super::{header_for, umi_str, EventThread, HostEvent};
use crate::manifest::ModId;

/// One of a mod's own key bindings was pressed while no screen was open.
///
/// Mods never see the player's other keystrokes (ADR-0086, F3.4 amendment), so the
/// event is aimed at one mod: [`HostEvent::target`] returns its id, which is host
/// data outside the ABI payload, and both dispatch adapters skip every other mod.
#[repr(C)]
#[derive(Debug)]
pub struct KeyPressed {
    pub(crate) raw: UmiKeyPressed,
    key: String,
    target: ModId,
}

impl KeyPressed {
    /// Builds the event for binding `key` (the id the mod gave to `register_key`) of
    /// mod `target`.
    pub fn new(target: ModId, key: &str) -> Self {
        let key = key.to_owned();
        Self {
            raw: UmiKeyPressed {
                header: header_for::<UmiKeyPressed>(UmiEventId::KEY_PRESSED),
                key: umi_str(&key),
            },
            key,
            target,
        }
    }

    /// The key id of the binding that was pressed.
    pub fn key(&self) -> &str {
        &self.key
    }
}

// Not derived: a derived clone would keep pointing at the original's buffer.
impl Clone for KeyPressed {
    fn clone(&self) -> Self {
        Self::new(self.target.clone(), &self.key)
    }
}

impl HostEvent for KeyPressed {
    const ID: UmiEventId = UmiEventId::KEY_PRESSED;
    const THREAD: EventThread = EventThread::Render;
    const CANCELLABLE: bool = false;
    const MUTABLE: bool = false;

    fn header(&self) -> &UmiEventHeader {
        &self.raw.header
    }

    fn header_mut(&mut self) -> &mut UmiEventHeader {
        &mut self.raw.header
    }

    fn target(&self) -> Option<&ModId> {
        Some(&self.target)
    }
}

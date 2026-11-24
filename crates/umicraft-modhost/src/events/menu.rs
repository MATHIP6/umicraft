// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mods-menu events of API 1.2 (ADR-0095), emitted on the render thread and
//! aimed at the mod that declared the module or the button, like `KEY_PRESSED`.

use umicraft_modapi_sys::{UmiButtonPressed, UmiEventHeader, UmiEventId, UmiModuleToggled};

use super::{header_for, umi_str, EventThread, HostEvent};
use crate::manifest::ModId;

/// The player switched module `module` of mod `target` on or off.
#[repr(C)]
#[derive(Debug)]
pub struct ModuleToggled {
    pub(crate) raw: UmiModuleToggled,
    module: String,
    target: ModId,
}

impl ModuleToggled {
    /// Builds the event for module `module` of mod `target`, now `enabled`.
    pub fn new(target: ModId, module: &str, enabled: bool) -> Self {
        let module = module.to_owned();
        Self {
            raw: UmiModuleToggled {
                header: header_for::<UmiModuleToggled>(UmiEventId::MODULE_TOGGLED),
                module: umi_str(&module),
                enabled: u8::from(enabled),
            },
            module,
            target,
        }
    }

    /// The module's key.
    pub fn module(&self) -> &str {
        &self.module
    }

    /// The new state of the switch.
    pub fn enabled(&self) -> bool {
        self.raw.enabled != 0
    }
}

// Not derived: a derived clone would keep pointing at the original's buffer.
impl Clone for ModuleToggled {
    fn clone(&self) -> Self {
        Self::new(self.target.clone(), &self.module, self.enabled())
    }
}

impl HostEvent for ModuleToggled {
    const ID: UmiEventId = UmiEventId::MODULE_TOGGLED;
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

/// The player clicked button `key` of mod `target`.
#[repr(C)]
#[derive(Debug)]
pub struct ButtonPressed {
    pub(crate) raw: UmiButtonPressed,
    module: String,
    key: String,
    target: ModId,
}

impl ButtonPressed {
    /// Builds the event for button `key` on the card of `module` (empty: the mod's
    /// own card) of mod `target`.
    pub fn new(target: ModId, module: &str, key: &str) -> Self {
        let (module, key) = (module.to_owned(), key.to_owned());
        Self {
            raw: UmiButtonPressed {
                header: header_for::<UmiButtonPressed>(UmiEventId::BUTTON_PRESSED),
                module: umi_str(&module),
                key: umi_str(&key),
            },
            module,
            key,
            target,
        }
    }

    /// The module whose card holds the button, empty for the mod's own card.
    pub fn module(&self) -> &str {
        &self.module
    }

    /// The button's key.
    pub fn key(&self) -> &str {
        &self.key
    }
}

// Not derived: a derived clone would keep pointing at the original's buffers.
impl Clone for ButtonPressed {
    fn clone(&self) -> Self {
        Self::new(self.target.clone(), &self.module, &self.key)
    }
}

impl HostEvent for ButtonPressed {
    const ID: UmiEventId = UmiEventId::BUTTON_PRESSED;
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

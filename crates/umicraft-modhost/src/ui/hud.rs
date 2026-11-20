// SPDX-License-Identifier: MIT OR Apache-2.0

//! HUD widgets a mod declares, and how they are drawn and exposed to the client.
//!
//! **Drawing** happens on the render thread: [`ModHost::draw_widgets`] clears each
//! enabled widget's [`HudContent`] and calls the mod's draw function, which fills it
//! through the `hud_*` host functions, under the 0.5 ms [`HUD_BUDGET`]. **Reading**
//! is a visitor, [`ModHost::visit_widgets`], that lends each widget as a
//! [`DrawnWidget`] borrowing the content: the buffers are reused from frame to
//! frame (as the built-in widgets do), so a frame with foreign widgets allocates
//! nothing once the buffers have grown, which a returned `Vec` could not promise.
//!
//! [`ModHost::draw_widgets`]: crate::ModHost::draw_widgets
//! [`ModHost::visit_widgets`]: crate::ModHost::visit_widgets

use std::sync::{Mutex, MutexGuard, PoisonError};

use umicraft_builtin_mods::{Anchor, HudContent, HudPlacement};
use umicraft_modapi_sys::{UmiHudDrawFn, UmiHudPlacement};

use super::validate::{check_key, check_text};
use super::MAX_LABEL_BYTES;
use crate::budget::HUD_BUDGET;
use crate::context::ModContext;
use crate::error::DeclareError;
use crate::ffi::{with_hud_cx, UserPtr};
use crate::hook::run_hook;
use crate::manifest::ModId;

/// A widget of a foreign mod, drawn this frame.
#[derive(Debug, Clone, Copy)]
pub struct DrawnWidget<'a> {
    /// The mod that owns the widget.
    pub mod_id: &'a ModId,
    /// The widget's key, unique within the mod.
    pub key: &'a str,
    /// Label for the HUD editor and the mods menu.
    pub label: &'a str,
    /// Where the player put it (or its default).
    pub placement: HudPlacement,
    /// What the draw function wrote this frame.
    pub content: &'a HudContent,
}

/// Converts the ABI placement: a non-finite number or an anchor above 8 is refused,
/// the scale is clamped by the same rule as the built-in widgets.
pub(crate) fn placement_of(raw: UmiHudPlacement) -> Result<HudPlacement, DeclareError> {
    let anchor = Anchor::from_index(raw.anchor).ok_or(DeclareError::Invalid {
        what: "widget placement",
        why: "anchor must be 0 to 8",
    })?;
    let finite = [raw.offset_x, raw.offset_y, raw.scale]
        .iter()
        .all(|v| v.is_finite());
    if !finite {
        return Err(DeclareError::Invalid {
            what: "widget placement",
            why: "offsets and scale must be finite",
        });
    }
    Ok(HudPlacement {
        anchor,
        offset_x: raw.offset_x,
        offset_y: raw.offset_y,
        scale: raw.scale,
    }
    .sanitized())
}

/// A declared widget: its draw function, its placement and its last content.
#[derive(Debug)]
pub(crate) struct WidgetSlot {
    key: String,
    label: String,
    default_placement: HudPlacement,
    placement: Mutex<HudPlacement>,
    draw: UmiHudDrawFn,
    user: UserPtr,
    content: Mutex<HudContent>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic while holding the lock cannot leave these plain values inconsistent.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl WidgetSlot {
    /// Validates and builds a widget.
    pub(crate) fn new(
        key: &str,
        label: &str,
        placement: HudPlacement,
        draw: Option<UmiHudDrawFn>,
        user: UserPtr,
    ) -> Result<Self, DeclareError> {
        check_key("widget key", key)?;
        check_text("widget label", label, MAX_LABEL_BYTES)?;
        let draw = draw.ok_or(DeclareError::Invalid {
            what: "widget",
            why: "the draw function must not be null",
        })?;
        Ok(Self {
            key: key.to_owned(),
            label: label.to_owned(),
            default_placement: placement,
            placement: Mutex::new(placement),
            draw,
            user,
            content: Mutex::new(HudContent::default()),
        })
    }

    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    pub(crate) fn default_placement(&self) -> HudPlacement {
        self.default_placement
    }

    pub(crate) fn placement(&self) -> HudPlacement {
        *lock(&self.placement)
    }

    /// Moves the widget; the placement is sanitised first.
    pub(crate) fn set_placement(&self, placement: HudPlacement) {
        *lock(&self.placement) = placement.sanitized();
    }

    /// Clears the content and calls the draw function under [`HUD_BUDGET`]; a failure
    /// (status or budget) has already failed the mod when this returns `false`.
    pub(crate) fn draw(&self, owner: &ModContext) -> bool {
        let mut content = lock(&self.content);
        content.clear();
        run_hook(
            owner,
            owner.hud_counters(),
            HUD_BUDGET,
            format_args!("its widget `{}`", self.key),
            || with_hud_cx(&mut content, |cx| (self.draw)(cx, self.user.as_ptr())),
        )
    }

    /// Lends the widget with its last content to `visit`.
    pub(crate) fn visit<'a>(&'a self, owner: &'a ModId, visit: &mut impl FnMut(DrawnWidget<'_>)) {
        let content = lock(&self.content);
        visit(DrawnWidget {
            mod_id: owner,
            key: &self.key,
            label: &self.label,
            placement: self.placement(),
            content: &content,
        });
    }
}

impl ModContext {
    /// Draws every widget of this mod, unless it is not enabled; stops at the first
    /// failure, since a failed mod is skipped from then on.
    pub fn draw_widgets(&self) {
        for widget in self.ui.declared().widgets() {
            if !self.is_enabled() {
                return;
            }
            widget.draw(self);
        }
    }

    /// Lends each widget of this mod, as last drawn, to `visit`; nothing for a mod
    /// that is not enabled, whose content may be stale or half written.
    pub fn visit_widgets(&self, mut visit: impl FnMut(DrawnWidget<'_>)) {
        if !self.is_enabled() {
            return;
        }
        for widget in self.ui.declared().widgets() {
            widget.visit(self.id(), &mut visit);
        }
    }
}

#[cfg(test)]
mod tests;

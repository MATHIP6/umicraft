// SPDX-License-Identifier: MIT OR Apache-2.0

//! The event fired right before a frame is drawn (API 1.1, ADR-0094).

use umicraft_modapi_sys::{UmiEventHeader, UmiEventId, UmiPreRender, UmiRenderCamera};

use super::{header_for, host_event, EventThread, HostEvent};

/// Fired once per drawn frame in a world, after the `render_camera` layers, with the
/// final camera of the frame; read-only and not cancellable.
///
/// A plain `Copy` payload with no pointer, so a derived `Clone` is correct and the
/// type is `Send` without any `unsafe`.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct PreRender {
    pub(crate) raw: UmiPreRender,
}

impl PreRender {
    /// Builds the event for the frame drawn with `camera`.
    pub fn new(camera: UmiRenderCamera) -> Self {
        Self {
            raw: UmiPreRender {
                header: header_for::<UmiPreRender>(UmiEventId::PRE_RENDER),
                camera,
            },
        }
    }

    /// The camera of the frame.
    pub fn camera(&self) -> UmiRenderCamera {
        self.raw.camera
    }
}

host_event!(PreRender, PRE_RENDER, Render, cancellable: false, mutable: false);

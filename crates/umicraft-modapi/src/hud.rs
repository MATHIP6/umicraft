// SPDX-License-Identifier: MIT OR Apache-2.0

//! HUD widgets: where they sit and what they draw.

use core::marker::PhantomData;

use umicraft_modapi_sys::{UmiHost, UmiHudCx, UmiHudPlacement, UmiStatus};

use crate::host::utf8;

/// A point on the screen a widget hangs from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// Top left corner.
    TopLeft = 0,
    /// Top edge, centred.
    TopCenter = 1,
    /// Top right corner.
    TopRight = 2,
    /// Left edge, centred.
    MiddleLeft = 3,
    /// Screen centre.
    Center = 4,
    /// Right edge, centred.
    MiddleRight = 5,
    /// Bottom left corner.
    BottomLeft = 6,
    /// Bottom edge, centred.
    BottomCenter = 7,
    /// Bottom right corner.
    BottomRight = 8,
    /// The top-left corner of the screen, with the offset in absolute GUI
    /// pixels, a scale of 1 and no panel or padding; the player cannot move it.
    /// For annotations placed with
    /// [`Frame::world_to_screen`](crate::Frame::world_to_screen). Added in API 1.1.
    Screen = 9,
}

/// The default position of a widget; the player can move and scale it.
///
/// ```
/// use umicraft_modapi::{Anchor, Placement};
/// let placement = Placement::new(Anchor::TopLeft).offset(4.0, 4.0).scale(1.5);
/// assert_eq!(placement.offset, [4.0, 4.0]);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// The screen point the widget hangs from.
    pub anchor: Anchor,
    /// Offset from the anchor in GUI pixels.
    pub offset: [f32; 2],
    /// Scale factor; the host clamps it to the built-in widgets' range.
    pub scale: f32,
}

impl Placement {
    /// At `anchor`, no offset, scale 1.
    pub fn new(anchor: Anchor) -> Self {
        Self {
            anchor,
            offset: [0.0, 0.0],
            scale: 1.0,
        }
    }

    /// Moves the widget by `(x, y)` GUI pixels from its anchor.
    #[must_use]
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = [x, y];
        self
    }

    /// Sets the scale factor.
    #[must_use]
    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    pub(crate) fn to_raw(self) -> UmiHudPlacement {
        UmiHudPlacement {
            anchor: self.anchor as u32,
            offset_x: self.offset[0],
            offset_y: self.offset[1],
            scale: self.scale,
        }
    }
}

/// The drawing surface of one widget for one frame.
///
/// Only exists inside a draw closure (see [`InitCx::hud_widget`](crate::InitCx::hud_widget));
/// the borrow stops it from outliving the call. Colours are `0xRRGGBB`,
/// coordinates are widget pixels.
///
/// Drawing is best effort: a call the host refuses (a non-finite number, say)
/// is skipped together with the rest of that frame's calls instead of
/// aborting, because one bad line should not take the widget down, and the
/// next frame starts clean.
#[derive(Debug)]
pub struct Hud<'a> {
    raw: *mut UmiHudCx,
    host: UmiHost,
    failed: bool,
    frame: PhantomData<&'a mut UmiHudCx>,
}

impl Hud<'_> {
    pub(crate) fn new(raw: *mut UmiHudCx, host: UmiHost) -> Self {
        Self {
            raw,
            host,
            failed: false,
            frame: PhantomData,
        }
    }

    fn emit(&mut self, call: impl FnOnce(&UmiHost, *mut UmiHudCx) -> UmiStatus) {
        if !self.failed {
            self.failed = !call(&self.host, self.raw).is_ok();
        }
    }

    /// Adds a text line below the previous ones.
    pub fn line(&mut self, text: &str, color: u32) {
        self.emit(|host, raw| (host.hud_line)(raw, utf8(text), color));
    }

    /// Adds free text at `(x, y)`.
    pub fn label(&mut self, x: f32, y: f32, text: &str, color: u32, alpha: f32, scale: f32) {
        self.emit(|host, raw| (host.hud_label)(raw, x, y, utf8(text), color, alpha, scale));
    }

    /// Adds a boxed cell, drawn as pressed or not (a key of a keystrokes
    /// display, for instance).
    #[allow(clippy::too_many_arguments)] // mirrors `hud_cell` of the C ABI
    pub fn cell(&mut self, x: f32, y: f32, w: f32, h: f32, pressed: bool, text: &str, color: u32) {
        self.emit(|host, raw| {
            (host.hud_cell)(raw, x, y, w, h, u8::from(pressed), utf8(text), color)
        });
    }

    /// Adds a filled rectangle.
    pub fn tick(&mut self, x: f32, y: f32, w: f32, h: f32, color: u32, alpha: f32) {
        self.emit(|host, raw| (host.hud_tick)(raw, x, y, w, h, color, alpha));
    }

    /// Reserves at least `w` x `h` pixels: labels, cells and ticks do not
    /// size the widget by themselves.
    pub fn reserve(&mut self, w: f32, h: f32) {
        self.emit(|host, raw| (host.hud_reserve)(raw, w, h));
    }

    /// Adds the one-pixel outline of a `w` x `h` rectangle at `(x, y)`.
    pub fn outline(&mut self, x: f32, y: f32, w: f32, h: f32, color: u32, alpha: f32) {
        self.emit(|host, raw| (host.hud_outline)(raw, x, y, w, h, color, alpha));
    }

    /// Adds a line from `(x1, y1)` to `(x2, y2)`, `thickness` pixels wide (the
    /// host keeps it within 0.05..=8 since API 1.4, 0.5..=8 before; under one
    /// screen pixel it is drawn one pixel wide and fainter).
    #[allow(clippy::too_many_arguments)] // mirrors `hud_segment` of the C ABI
    pub fn segment(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        color: u32,
        alpha: f32,
        thickness: f32,
    ) {
        self.emit(|host, raw| (host.hud_segment)(raw, x1, y1, x2, y2, color, alpha, thickness));
    }

    /// Adds a filled disc of `radius` pixels centred on `(x, y)`. API 1.4.
    pub fn circle(&mut self, x: f32, y: f32, radius: f32, color: u32, alpha: f32) {
        self.emit(|host, raw| (host.hud_circle)(raw, x, y, radius, color, alpha, 0.0));
    }

    /// Adds a ring of `radius` pixels centred on `(x, y)`, `thickness` pixels
    /// wide inside its edge (at most 8). API 1.4.
    #[allow(clippy::too_many_arguments)] // mirrors `hud_circle` of the C ABI
    pub fn ring(&mut self, x: f32, y: f32, radius: f32, color: u32, alpha: f32, thickness: f32) {
        self.emit(|host, raw| (host.hud_circle)(raw, x, y, radius, color, alpha, thickness));
    }

    /// Adds the filled convex polygon through `points` (3 to 64 of them). API
    /// 1.4.
    pub fn polygon(&mut self, points: &[[f32; 2]], color: u32, alpha: f32) {
        self.emit(|host, raw| {
            (host.hud_polygon)(raw, points.as_ptr().cast(), points.len(), color, alpha)
        });
    }

    /// Runs `draw` with the free items it adds (cells, labels, rectangles,
    /// segments, circles, polygons; not [`line`](Self::line)s) cut to the
    /// `w` x `h` rectangle at `(x, y)`, within any enclosing clip. At most 8
    /// clips nest: a ninth is refused, and like any refused call it skips the
    /// rest of the frame. API 1.4.
    ///
    /// ```ignore
    /// hud.clip(0.0, 0.0, 60.0, 10.0, |hud| hud.label(30.0, 0.0, long_text, WHITE, 1.0, 1.0));
    /// ```
    pub fn clip(&mut self, x: f32, y: f32, w: f32, h: f32, draw: impl FnOnce(&mut Self)) {
        let mut pushed = false;
        self.emit(|host, raw| {
            let status = (host.hud_clip_push)(raw, x, y, w, h);
            pushed = status.is_ok();
            status
        });
        draw(self);
        if pushed {
            self.emit(|host, raw| (host.hud_clip_pop)(raw));
        }
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The opening animation of the modern title screen: when each part comes in
//! and how it moves. The dark lifts, the logo grows into place, the wordmark's
//! letters come in, the two buttons rise in turn, then the cross and the footer
//! fade in. [`Stage`], [`lowered`] and [`scaled`] are shared with the mods menu.

use crate::geom::Rect;

/// Ticks the opening lasts (2.6 s at 20 TPS): the end of the last stage.
pub(super) const INTRO_TICKS: u8 = 52;
/// How far the logo rises while it grows into place, in GUI units.
pub(super) const LOGO_RISE: f32 = 16.0;
/// Size of the logo when it starts to grow, as a fraction of its final size.
pub(super) const LOGO_START_SCALE: f32 = 0.8;
/// How far a main button rises while it fades in, in GUI units.
pub(super) const BUTTON_RISE: f32 = 8.0;

/// One stage of the opening: it starts `delay` ticks in and takes `span`
/// ticks, easing out.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Stage {
    delay: f32,
    span: f32,
}

/// The black sheet over the screen lifts.
pub(super) const VEIL: Stage = Stage::new(0.0, 24.0);
/// The logo grows and rises into place.
pub(super) const LOGO: Stage = Stage::new(6.0, 26.0);
/// The letters of the wordmark come in.
pub(super) const WORD: Stage = Stage::new(14.0, 24.0);
/// The Play button rises.
pub(super) const PLAY: Stage = Stage::new(28.0, 18.0);
/// The Options button rises, a little after Play.
pub(super) const OPTIONS: Stage = Stage::new(33.0, 18.0);
/// The quit cross and the footer fade in last.
pub(super) const CHROME: Stage = Stage::new(40.0, 12.0);

impl Stage {
    pub(crate) const fn new(delay: f32, span: f32) -> Self {
        Self { delay, span }
    }

    /// `0.0` before the stage, `1.0` after it, in between a cubic ease-out of
    /// the way through it, `ticks` being the time since the opening began.
    pub(crate) fn at(self, ticks: f32) -> f32 {
        let linear = ((ticks - self.delay) / self.span).clamp(0.0, 1.0);
        1.0 - (1.0 - linear).powi(3)
    }
}

/// `rect` moved down by what is left of `rise` GUI units, `progress` of the
/// way through its stage.
pub(crate) fn lowered(rect: Rect, rise: f32, scale: f32, progress: f32) -> Rect {
    Rect::new(
        rect.x,
        rect.y + (1.0 - progress) * rise * scale,
        rect.width,
        rect.height,
    )
}

/// `rect` shrunk by `factor` about its centre.
pub(crate) fn scaled(rect: Rect, factor: f32) -> Rect {
    let (width, height) = (rect.width * factor, rect.height * factor);
    Rect::new(
        rect.x + (rect.width - width) * 0.5,
        rect.y + (rect.height - height) * 0.5,
        width,
        height,
    )
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Design tokens of the modern widgets.
//!
//! Lengths and text sizes are in GUI units; colours are straight-alpha RGBA.
//! The surfaces stay translucent so the panorama shows through.

use crate::draw::Color;

/// Pixel size the brand face is rasterized at.
///
/// Must equal `crate::brand_font::BRAND_PIXEL_SIZE`; text scales are
/// `wanted_px / BRAND_PIXEL_SIZE`.
pub const BRAND_PIXEL_SIZE: f32 = 48.0;

/// Fill of buttons and panels at rest.
pub const SURFACE: Color = [0.07, 0.08, 0.10, 0.55];
/// Fill of a hovered button.
pub const SURFACE_HOVER: Color = [0.20, 0.23, 0.30, 0.78];
/// Fill of a disabled button.
pub const SURFACE_DISABLED: Color = [0.07, 0.08, 0.10, 0.30];
/// The highlight colour of the brand.
pub const ACCENT: Color = [0.36, 0.55, 1.00, 1.0];
/// Primary text.
pub const TEXT: Color = [0.96, 0.97, 0.98, 1.0];
/// Secondary text.
pub const TEXT_MUTED: Color = [0.70, 0.73, 0.80, 1.0];
/// The darkest point of the screen vignette (alpha is the strength).
pub const VIGNETTE: Color = [0.0, 0.0, 0.02, 0.60];
/// Factor applied to the alpha of text and icons on a disabled widget.
pub const DISABLED_OPACITY: f32 = 0.4;

/// Corner radius of buttons and panels.
pub const RADIUS: f32 = 3.0;
/// The gap between neighbouring widgets and between an icon and its label.
pub const SPACING: f32 = 4.0;
/// Height of a standard button.
pub const BUTTON_HEIGHT: f32 = 16.0;
/// Edge of a small icon-only button.
pub const ICON_BUTTON_SIZE: f32 = 14.0;

/// Size of button labels.
pub const TEXT_SIZE_BODY: f32 = 7.0;
/// Size of small captions.
pub const TEXT_SIZE_SMALL: f32 = 5.5;
/// Size of headings.
pub const TEXT_SIZE_TITLE: f32 = 16.0;

/// Fraction of the hover fade gained or lost per 20 TPS tick (about 200 ms).
pub const HOVER_FADE_STEP: f32 = 0.25;

/// Linear blend of two colours, `t` clamped to `0.0..=1.0`.
#[must_use]
pub fn mix(from: Color, to: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    std::array::from_fn(|i| from[i] * (1.0 - t) + to[i] * t)
}

/// `color` with its alpha multiplied by `opacity`.
#[must_use]
pub fn with_opacity(color: Color, opacity: f32) -> Color {
    [color[0], color[1], color[2], color[3] * opacity]
}

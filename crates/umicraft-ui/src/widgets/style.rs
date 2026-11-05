// SPDX-License-Identifier: MIT OR Apache-2.0

//! The crate-private palette and metrics shared by the M4 widgets.
//!
//! The M4 contract freezes the widget *behaviour*, not a theming API, so the
//! values stay crate-private. A later theme pass can lift them into a public
//! style type without changing any widget signature. Colours are straight
//! alpha RGBA; the palette is deliberately dark so the debug screen reads over
//! both the sky and the terrain.

use crate::draw::Color;

/// Multiplier applied to the font atlas metrics by every widget label.
///
/// The M3 overlay drew at scale 2 and the 1.8 font is designed for GUI scale
/// 2 at the usual window sizes. Keeping one constant here makes the button,
/// the text field and the list agree on a line height.
pub(crate) const TEXT_SCALE: f32 = 2.0;

/// Primary label colour.
pub(crate) const TEXT: Color = [0.94, 0.95, 0.98, 1.0];
/// Colour of placeholder and other secondary text.
pub(crate) const TEXT_DIM: Color = [0.55, 0.58, 0.66, 1.0];
/// Colour of a debug-screen title.
pub(crate) const TITLE: Color = [0.98, 0.98, 1.0, 1.0];

/// Panel fill behind a debug screen.
pub(crate) const PANEL: Color = [0.05, 0.06, 0.09, 0.94];

/// Button fill in its idle state.
pub(crate) const BUTTON: Color = [0.18, 0.20, 0.26, 1.0];
/// Button fill when it cannot be pressed.
pub(crate) const BUTTON_DISABLED: Color = [0.12, 0.13, 0.17, 1.0];
/// Button fill while the pointer is over it.
pub(crate) const BUTTON_HOVER: Color = [0.27, 0.30, 0.38, 1.0];
/// Button fill on the frame of the press.
pub(crate) const BUTTON_PRESSED: Color = [0.12, 0.38, 0.62, 1.0];

/// Text-field outline, focused or not (`0xFFA0A0A0`).
pub(crate) const FIELD_BORDER: Color = [0.627, 0.627, 0.627, 1.0];
/// Text-field fill (`0xFF000000`).
pub(crate) const FIELD_BACKGROUND: Color = [0.0, 0.0, 0.0, 1.0];
/// Text-field text (`0xE0E0E0`, `GuiTextField.enabledColor`).
pub(crate) const TEXT_FIELD: Color = [0.878, 0.878, 0.878, 1.0];
/// The shadow of [`TEXT_FIELD`] (`(0xE0E0E0 & 0xFCFCFC) >> 2`).
pub(crate) const TEXT_FIELD_SHADOW: Color = [0.220, 0.220, 0.220, 1.0];
/// Disabled text-field text (`0x707070`, `GuiTextField.disabledColor`).
pub(crate) const TEXT_FIELD_DISABLED: Color = [0.439, 0.439, 0.439, 1.0];
/// The shadow of [`TEXT_FIELD_DISABLED`] (`(0x707070 & 0xFCFCFC) >> 2`).
pub(crate) const TEXT_FIELD_DISABLED_SHADOW: Color = [0.110, 0.110, 0.110, 1.0];
/// The bar caret drawn inside the text (`0xFFD0D0D0`).
pub(crate) const CARET_BAR: Color = [0.816, 0.816, 0.816, 1.0];

/// List fill.
pub(crate) const LIST_BACKGROUND: Color = [0.07, 0.08, 0.11, 1.0];
/// Row fill while the pointer is over it.
pub(crate) const LIST_ROW_HOVER: Color = [0.15, 0.18, 0.24, 1.0];
/// Row fill of the selected row.
pub(crate) const LIST_ROW_SELECTED: Color = [0.14, 0.38, 0.60, 1.0];
/// Scrollbar groove.
pub(crate) const SCROLLBAR_TRACK: Color = [0.12, 0.13, 0.17, 1.0];
/// Scrollbar thumb.
pub(crate) const SCROLLBAR_THUMB: Color = [0.38, 0.42, 0.52, 1.0];

/// Backdrop drawn behind the whole debug screen.
pub(crate) const SCREEN_BACKDROP: Color = [0.0, 0.0, 0.0, 0.55];

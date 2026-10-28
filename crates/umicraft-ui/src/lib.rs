// SPDX-License-Identifier: MIT OR Apache-2.0

//! # umicraft-ui
//!
//! The backend-agnostic immediate-mode UI of Umicraft (spec M4).
//!
//! This crate owns the widget layer, the layout maths, the focus and hit
//! testing, the font atlas and the draw list. It deliberately owns **no**
//! window, GPU, network or filesystem code: [`umicraft-render`] uploads the
//! [`DrawList`] and `umicraft-client` feeds the [`UiInput`]. The same code is
//! therefore usable from the windowed client, from the offscreen screenshot
//! path and from the headless acceptance driver.
//!
//! The public surface is frozen by `docs/decisions/0006-m4-ui-foundation.md`.
//! The widget modules are filled by the M4 workstreams; the types in
//! [`geom`], [`id`], [`input`], [`draw`], [`text`] and [`context`] are the
//! contract they compile against. The layout helpers in [`geom`] are pure
//! functions over [`Rect`]: a [`Layout`] carves rows or columns out of a panel
//! and a [`ScrollArea`] clamps a viewport over taller content.
//!
//! [`umicraft-render`]: https://docs.rs/umicraft-render
//!
//! # Example
//!
//! ```
//! use umicraft_ui::{Rect, Ui, UiInput};
//!
//! let viewport = Rect::new(0.0, 0.0, 1280.0, 720.0);
//! let mut ui = Ui::new(viewport);
//! ui.begin_frame(viewport, UiInput::default());
//!
//! let output = ui.end_frame();
//! assert!(output.draw_list.is_empty());
//! assert!(output.focus.is_none());
//! ```

pub mod context;
pub mod debug_screen;
pub mod draw;
pub mod geom;
pub mod id;
pub mod input;
pub mod text;
pub mod widgets;

pub use context::{Ui, UiOutput, UiResponse};
pub use draw::{Color, DrawCommand, DrawList, TextStyle};
pub use geom::{Axis, Insets, Layout, Rect, ScrollArea};
pub use id::WidgetId;
pub use input::{InputEvent, Key, Modifiers, PointerButton, UiInput};
pub use text::{FontAtlas, FontError, Glyph};
pub use widgets::{draw_item, SlotItem};

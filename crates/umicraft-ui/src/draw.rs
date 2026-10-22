// SPDX-License-Identifier: MIT OR Apache-2.0

//! The backend-agnostic draw list produced by one UI frame.
//!
//! The list is deliberately small: rectangles, text runs and clip scopes.
//! `umicraft-render` converts it into overlay vertices and never learns what a
//! widget is. Keeping the boundary this narrow is what lets the same debug
//! screen run in the window and in the headless path.

use crate::geom::Rect;

/// Straight-alpha RGBA colour, components in `0.0..=1.0`.
pub type Color = [f32; 4];

/// The visual parameters of a text run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    /// Multiplier applied to the font atlas metrics.
    pub scale: f32,
    /// Straight-alpha RGBA colour.
    pub color: Color,
}

impl TextStyle {
    /// Creates a text style.
    #[must_use]
    pub const fn new(scale: f32, color: Color) -> Self {
        Self { scale, color }
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            scale: 1.0,
            color: [1.0; 4],
        }
    }
}

/// One draw command in screen-space order.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    /// Fill a rectangle with a solid colour.
    Rect {
        /// Rectangle to fill, in physical pixels.
        rect: Rect,
        /// Straight-alpha RGBA colour.
        color: Color,
    },
    /// Draw one line of text at `position` (top-left of the first glyph).
    Text {
        /// Top-left position of the text run.
        position: [f32; 2],
        /// UTF-8 text; the renderer maps unknown characters to the fallback.
        text: String,
        /// Scale and colour of the run.
        style: TextStyle,
    },
    /// Clip every following command to `rect` until the matching
    /// [`DrawCommand::PopClip`].
    PushClip {
        /// Clip rectangle in physical pixels.
        rect: Rect,
    },
    /// End the most recent clip scope.
    PopClip,
}

/// The ordered commands of one UI frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DrawList {
    commands: Vec<DrawCommand>,
}

impl DrawList {
    /// Creates an empty list.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Drops every command, keeping the allocation for the next frame.
    pub fn clear(&mut self) {
        self.commands.clear();
    }

    /// True when the frame would draw nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Number of commands in the list.
    #[must_use]
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Borrows the commands in draw order.
    #[must_use]
    pub fn commands(&self) -> &[DrawCommand] {
        &self.commands
    }

    /// Appends a solid rectangle.
    pub fn rect(&mut self, rect: Rect, color: Color) {
        self.commands.push(DrawCommand::Rect { rect, color });
    }

    /// Appends a text run.
    pub fn text(&mut self, position: [f32; 2], text: impl Into<String>, style: TextStyle) {
        self.commands.push(DrawCommand::Text {
            position,
            text: text.into(),
            style,
        });
    }

    /// Starts a clip scope.
    pub fn push_clip(&mut self, rect: Rect) {
        self.commands.push(DrawCommand::PushClip { rect });
    }

    /// Ends the most recent clip scope.
    pub fn pop_clip(&mut self) {
        self.commands.push(DrawCommand::PopClip);
    }
}

#[cfg(test)]
mod tests {
    use super::{DrawCommand, DrawList, TextStyle};
    use crate::geom::Rect;

    #[test]
    fn commands_keep_their_order() {
        let mut list = DrawList::new();
        list.push_clip(Rect::new(0.0, 0.0, 10.0, 10.0));
        list.rect(Rect::new(1.0, 1.0, 2.0, 2.0), [1.0, 0.0, 0.0, 1.0]);
        list.text([2.0, 3.0], "hello", TextStyle::default());
        list.pop_clip();

        assert_eq!(list.len(), 4);
        assert!(matches!(list.commands()[0], DrawCommand::PushClip { .. }));
        assert!(matches!(list.commands()[3], DrawCommand::PopClip));
    }

    #[test]
    fn clear_keeps_the_list_empty() {
        let mut list = DrawList::new();
        list.rect(Rect::ZERO, [1.0; 4]);
        list.clear();
        assert!(list.is_empty());
        assert_eq!(list.len(), 0);
    }
}

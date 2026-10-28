// SPDX-License-Identifier: MIT OR Apache-2.0

//! The backend-agnostic draw list produced by one UI frame.
//!
//! The list is deliberately small: rectangles, text runs and clip scopes.
//! `umicraft-render` converts it into overlay vertices and never learns what a
//! widget is. Keeping the boundary this narrow is what lets the same debug
//! screen run in the window and in the headless path.
//!
//! Clip scopes are tracked as they are appended. [`DrawList::pop_clip`]
//! ignores a second pop at depth zero, and [`DrawList::close_clips`] closes
//! whatever is still open when the frame ends. A renderer can therefore
//! assume that every [`DrawCommand::PushClip`] has a matching
//! [`DrawCommand::PopClip`].

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
    /// Draw a sub-region of the GUI texture atlas onto `dst`.
    ///
    /// `src` is `[u_min, v_min, u_max, v_max]` in normalised atlas
    /// coordinates. The renderer samples the RGBA GUI atlas (not the font
    /// atlas) and multiplies by `color` for tinting.
    Image {
        /// Source UV rectangle in the GUI atlas.
        src: [f32; 4],
        /// Destination rectangle in physical pixels.
        dst: Rect,
        /// Tint colour multiplied with the texel (white = unmodified).
        color: Color,
    },
    /// Draw the item icon registered under `key` into `dst`.
    ///
    /// `key` is an opaque, client-chosen identifier that the renderer maps to
    /// a slot of its icon atlas; the UI never interprets it. `glint` asks for
    /// the enchantment glint overlay.
    Item {
        /// Opaque icon key chosen by the client.
        key: u64,
        /// Destination rectangle in physical pixels.
        dst: Rect,
        /// Whether to draw the enchantment glint over the icon.
        glint: bool,
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
    /// Effective clip rectangles, each already intersected with its parent.
    clips: Vec<Rect>,
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
        self.clips.clear();
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

    /// Number of commands the list can hold without reallocating.
    ///
    /// This is exposed so tests and benchmarks can verify that [`Self::clear`]
    /// reuses the frame allocation instead of dropping it.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.commands.capacity()
    }

    /// Number of clip scopes that are currently open.
    #[must_use]
    pub fn clip_depth(&self) -> usize {
        self.clips.len()
    }

    /// The effective clip of the innermost scope, already intersected with
    /// every enclosing clip, or `None` when no scope is open.
    ///
    /// The UI uses this for hit testing, so a widget can push a clip directly
    /// on the draw list and the pointer test follows it.
    #[must_use]
    pub fn clip_rect(&self) -> Option<Rect> {
        self.clips.last().copied()
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

    /// Appends a textured rectangle from the GUI atlas.
    ///
    /// `src` is `[u_min, v_min, u_max, v_max]` in normalised atlas
    /// coordinates. `color` tints the texel (`[1.0; 4]` = unmodified).
    pub fn image(&mut self, src: [f32; 4], dst: Rect, color: Color) {
        self.commands.push(DrawCommand::Image { src, dst, color });
    }

    /// Appends an item icon registered under `key`, drawn into `dst`.
    ///
    /// `glint` adds the enchantment glint. The key is opaque to the UI.
    pub fn item(&mut self, key: u64, dst: Rect, glint: bool) {
        self.commands.push(DrawCommand::Item { key, dst, glint });
    }

    /// Starts a clip scope.
    ///
    /// The command records the intersection with the enclosing scopes, so the
    /// renderer never has to combine them and [`Self::clip_rect`] can answer
    /// hit tests with one rectangle.
    pub fn push_clip(&mut self, rect: Rect) {
        let effective = match self.clips.last() {
            Some(parent) => parent
                .intersect(rect)
                .unwrap_or(Rect::new(parent.x, parent.y, 0.0, 0.0)),
            None => rect,
        };
        self.clips.push(effective);
        self.commands
            .push(DrawCommand::PushClip { rect: effective });
    }

    /// Ends the most recent clip scope.
    ///
    /// A pop without a matching push is ignored instead of emitting an
    /// unbalanced command; the renderer never sees a negative clip depth.
    pub fn pop_clip(&mut self) {
        if self.clips.pop().is_none() {
            return;
        }
        self.commands.push(DrawCommand::PopClip);
    }

    /// Closes every clip scope that is still open.
    ///
    /// This is the frame-end safety net: a widget that forgot a
    /// [`Self::pop_clip`] cannot leak its clip into the next frame, and the
    /// list stays balanced for the renderer.
    pub fn close_clips(&mut self) {
        while !self.clips.is_empty() {
            self.pop_clip();
        }
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
    fn item_appends_an_item_command() {
        let mut list = DrawList::new();
        let dst = Rect::new(1.0, 2.0, 16.0, 16.0);
        list.item(7, dst, true);
        assert_eq!(
            list.commands(),
            [DrawCommand::Item {
                key: 7,
                dst,
                glint: true
            }]
        );
    }

    #[test]
    fn clear_keeps_the_list_empty() {
        let mut list = DrawList::new();
        list.rect(Rect::ZERO, [1.0; 4]);
        list.clear();
        assert!(list.is_empty());
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn clear_keeps_the_allocation() {
        let mut list = DrawList::new();
        list.rect(Rect::new(1.0, 2.0, 3.0, 4.0), [1.0; 4]);
        let capacity = list.capacity();
        assert!(capacity > 0);
        list.clear();
        assert_eq!(list.capacity(), capacity);
    }

    #[test]
    fn clips_are_balanced_and_stray_pops_are_ignored() {
        let mut list = DrawList::new();
        list.pop_clip();
        assert!(list.is_empty());
        assert_eq!(list.clip_depth(), 0);

        list.push_clip(Rect::new(0.0, 0.0, 10.0, 10.0));
        list.push_clip(Rect::new(2.0, 2.0, 4.0, 4.0));
        list.pop_clip();
        assert_eq!(list.clip_depth(), 1);
        list.close_clips();
        assert_eq!(list.clip_depth(), 0);
        assert_eq!(list.len(), 4);
        assert_eq!(
            list.commands()
                .iter()
                .filter(|command| matches!(command, DrawCommand::PushClip { .. }))
                .count(),
            list.commands()
                .iter()
                .filter(|command| matches!(command, DrawCommand::PopClip))
                .count()
        );
    }

    #[test]
    fn clear_resets_the_clip_depth() {
        let mut list = DrawList::new();
        list.push_clip(Rect::new(0.0, 0.0, 10.0, 10.0));
        list.clear();
        assert_eq!(list.clip_depth(), 0);
        list.pop_clip();
        assert!(list.is_empty());
    }

    #[test]
    fn nested_clips_record_the_intersection() {
        let mut list = DrawList::new();
        list.push_clip(Rect::new(0.0, 0.0, 100.0, 100.0));
        assert_eq!(list.clip_rect(), Some(Rect::new(0.0, 0.0, 100.0, 100.0)));
        list.push_clip(Rect::new(50.0, 40.0, 100.0, 100.0));
        assert_eq!(list.clip_rect(), Some(Rect::new(50.0, 40.0, 50.0, 60.0)));
        assert!(matches!(
            list.commands()[1],
            DrawCommand::PushClip { rect } if rect == Rect::new(50.0, 40.0, 50.0, 60.0)
        ));
        list.pop_clip();
        assert_eq!(list.clip_rect(), Some(Rect::new(0.0, 0.0, 100.0, 100.0)));
        list.close_clips();
        assert_eq!(list.clip_rect(), None);
    }
}

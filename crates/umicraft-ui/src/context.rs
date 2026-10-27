// SPDX-License-Identifier: MIT OR Apache-2.0

//! The immediate-mode UI context: one frame in, one draw list out.
//!
//! [`Ui`] is owned by the render thread (or by the headless driver). Each
//! frame starts with [`Ui::begin_frame`], widgets append to the draw list and
//! return their [`UiResponse`], and [`Ui::end_frame`] moves the list out for
//! the renderer. No state is shared with the game, network or mesh threads.
//!
//! # Focus and hit testing
//!
//! Every widget that calls [`Ui::response`] is registered in the keyboard focus
//! order for that frame. `Tab` moves to the next widget and `Shift+Tab` to the
//! previous one, wrapping at both ends. The order from the previous frame is
//! kept so the traversal can happen at the start of the next frame, where the
//! focused widget can already see its new state. Clicking a widget focuses it;
//! clicking the empty part of the viewport clears the focus. State left by
//! [`Ui::push_clip`] is respected by hover and click tests, so a row clipped
//! out of a scroll list cannot be hovered or clicked.
//!
//! Between frames the context keeps the focus, the pointer position and its
//! allocations. Widgets are not retained: an ID is stable only as long as the
//! ID stack and the widget label are stable.

use crate::draw::{DrawList, SpriteSheet};
use crate::geom::Rect;
use crate::id::WidgetId;
use crate::input::{InputEvent, Key, PointerButton, UiInput};

/// What one widget reports after it has been drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiResponse {
    /// The widget that produced the response.
    pub id: WidgetId,
    /// The pointer is over the widget.
    pub hovered: bool,
    /// The primary button was pressed over the widget in this frame.
    pub clicked: bool,
    /// The widget's value changed in this frame (typed text, selection, …).
    ///
    /// A widget that changes its state calls [`Ui::mark_changed`] so that the
    /// response stored in [`UiOutput::responses`] agrees with the value the
    /// widget returned.
    pub changed: bool,
    /// The widget owns the keyboard focus.
    pub focused: bool,
}

/// The result of one completed UI frame.
#[derive(Debug, Clone, Default)]
pub struct UiOutput {
    /// The commands the renderer must draw, in order.
    pub draw_list: DrawList,
    /// One response per widget that was visited, in visit order.
    pub responses: Vec<UiResponse>,
    /// The widget that owns the focus after the frame.
    pub focus: Option<WidgetId>,
}

/// The direction of a keyboard focus traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Traversal {
    /// Move to the next focusable widget.
    Forward,
    /// Move to the previous focusable widget.
    Backward,
}

/// The per-frame immediate-mode state.
#[derive(Debug, Clone)]
pub struct Ui {
    viewport: Rect,
    input: UiInput,
    draw: DrawList,
    responses: Vec<UiResponse>,
    focus: Option<WidgetId>,
    /// Focus order from the last completed frame, used by `Tab` at frame start.
    focus_order: Vec<WidgetId>,
    /// Focus order collected by the current frame.
    frame_order: Vec<WidgetId>,
    /// A traversal that had no previous frame order to work with.
    pending_traversal: Option<Traversal>,
    id_stack: Vec<WidgetId>,
    /// Last pointer position, kept across frames until another event moves it.
    pointer: Option<[f32; 2]>,
    /// Primary press of the current frame, if any.
    primary_pressed: Option<[f32; 2]>,
    /// The press was claimed by a widget, so it no longer clears the focus.
    click_consumed: bool,
    /// The optional widgets sprite sheet used for skinning buttons and sliders.
    widgets_sheet: Option<SpriteSheet>,
}

impl Ui {
    /// Creates a UI context for `viewport` in physical pixels.
    #[must_use]
    pub fn new(viewport: Rect) -> Self {
        Self {
            viewport,
            input: UiInput::default(),
            draw: DrawList::new(),
            responses: Vec::new(),
            focus: None,
            focus_order: Vec::new(),
            frame_order: Vec::new(),
            pending_traversal: None,
            id_stack: Vec::new(),
            pointer: None,
            primary_pressed: None,
            click_consumed: false,
            widgets_sheet: None,
        }
    }

    /// Starts a frame, replacing the viewport and the input of the old one.
    ///
    /// The draw list, the per-frame responses, the ID stack and the clip stack
    /// are cleared in place; the focus, the pointer and the input allocation
    /// survive across frames. `Tab` and `Shift+Tab` are applied against the
    /// focus order of the previous frame so the widgets built by this frame
    /// already report their new focus.
    pub fn begin_frame(&mut self, viewport: Rect, input: UiInput) {
        self.viewport = viewport;
        self.input.modifiers = input.modifiers;
        self.input.events.clear();
        self.input.events.extend(input.events);
        self.draw.clear();
        self.responses.clear();
        self.id_stack.clear();
        self.frame_order.clear();
        self.primary_pressed = None;
        self.click_consumed = false;
        self.pending_traversal = None;

        let mut pending = None;
        for event in &self.input.events {
            match event {
                InputEvent::PointerMoved { position }
                | InputEvent::PointerReleased { position, .. } => {
                    self.pointer = Some(*position);
                }
                InputEvent::PointerPressed { position, button } => {
                    self.pointer = Some(*position);
                    if *button == PointerButton::Left {
                        self.primary_pressed = Some(*position);
                    }
                }
                InputEvent::KeyPressed(Key::Tab) => {
                    let direction = if self.input.modifiers.shift {
                        Traversal::Backward
                    } else {
                        Traversal::Forward
                    };
                    if self.focus_order.is_empty() {
                        // The first frame has no order yet; apply it in
                        // `end_frame` once the widgets have registered.
                        pending = Some(direction);
                    } else {
                        self.focus = next_focus(self.focus, &self.focus_order, direction);
                    }
                }
                InputEvent::Wheel { .. }
                | InputEvent::KeyPressed(_)
                | InputEvent::KeyReleased(_)
                | InputEvent::Text(_) => {}
            }
        }
        self.pending_traversal = pending;
    }

    /// Ends the frame and hands the draw list and the responses to the caller.
    ///
    /// Any clip left open by a widget is closed, a primary press that no widget
    /// claimed clears the focus, a focus target that was not visited in this
    /// frame is dropped, and every stored response is synchronised with
    /// [`UiOutput::focus`]. The draw list and the response vector move into the
    /// output; their allocations belong to the caller until the next frame.
    /// Pass the output back through [`Self::recycle_output`] after rendering
    /// to reuse those allocations.
    pub fn end_frame(&mut self) -> UiOutput {
        self.draw.close_clips();

        if self
            .primary_pressed
            .is_some_and(|position| self.viewport.contains(position))
            && !self.click_consumed
        {
            self.focus = None;
        }
        if let Some(direction) = self.pending_traversal.take() {
            self.focus = next_focus(self.focus, &self.frame_order, direction);
        }
        // A focus target that was not visited in this frame is gone from the
        // tree; keeping it would let a hidden screen reopen with a stale field
        // focused.
        if self.focus.is_some_and(|id| !self.frame_order.contains(&id)) {
            self.focus = None;
        }
        for response in &mut self.responses {
            response.focused = self.focus == Some(response.id);
        }

        std::mem::swap(&mut self.focus_order, &mut self.frame_order);
        self.frame_order.clear();

        let output = UiOutput {
            draw_list: std::mem::take(&mut self.draw),
            responses: std::mem::take(&mut self.responses),
            focus: self.focus,
        };
        self.primary_pressed = None;
        self.click_consumed = false;
        output
    }

    /// Returns the allocations of a completed frame to this context.
    ///
    /// [`Self::end_frame`] transfers ownership of the draw list and the
    /// responses to the caller. When the renderer is done with a
    /// [`UiOutput`], passing it back here lets the next frame reuse those
    /// vectors instead of allocating them again. Call it between frames, not
    /// while a frame is being built; the focus stored in the output is
    /// ignored because the context already owns the current focus.
    pub fn recycle_output(&mut self, output: UiOutput) {
        self.draw = output.draw_list;
        self.draw.clear();
        self.responses = output.responses;
        self.responses.clear();
    }

    /// The current viewport in physical pixels.
    #[must_use]
    pub fn viewport(&self) -> Rect {
        self.viewport
    }

    /// The input events of the current frame.
    #[must_use]
    pub fn input(&self) -> &UiInput {
        &self.input
    }

    /// Mutable access to the draw list, for widgets that need a primitive
    /// instead of a helper.
    ///
    /// Clips pushed here participate in hit testing through
    /// [`DrawList::clip_rect`]; [`Self::push_clip`] is a convenience that also
    /// intersects the viewport.
    pub fn draw(&mut self) -> &mut DrawList {
        &mut self.draw
    }

    /// Sets the widgets sprite sheet for textured UI widgets (buttons, sliders).
    pub fn set_widgets_sheet(&mut self, sheet: Option<SpriteSheet>) {
        self.widgets_sheet = sheet;
    }

    /// The active widgets sprite sheet, if any.
    #[must_use]
    pub fn widgets_sheet(&self) -> Option<SpriteSheet> {
        self.widgets_sheet
    }

    /// Pushes a child scope onto the ID stack.
    ///
    /// The first scope of a frame is derived from its label alone; a nested
    /// scope also depends on its parent's ID. Scopes are cleared at the start
    /// of every frame, so the same calls produce the same IDs across frames.
    pub fn push_id(&mut self, label: &str) {
        let id = match self.id_stack.last() {
            Some(parent) => parent.child(label),
            None => WidgetId::from_label(label),
        };
        self.id_stack.push(id);
    }

    /// Pops the most recent ID scope.
    ///
    /// Popping the root scope (an empty stack) is a no-op.
    pub fn pop_id(&mut self) {
        self.id_stack.pop();
    }

    /// Derives a widget ID from the current ID scope.
    ///
    /// Outside a [`Self::push_id`] scope this is
    /// [`WidgetId::from_label`]; inside a scope it is the parent ID's
    /// [`WidgetId::child`]. Both are deterministic, so the same label at the
    /// same stack position keeps its identity across frames.
    #[must_use]
    pub fn id(&self, label: &str) -> WidgetId {
        match self.id_stack.last() {
            Some(parent) => parent.child(label),
            None => WidgetId::from_label(label),
        }
    }

    /// The widget that owns the keyboard focus.
    #[must_use]
    pub fn focus(&self) -> Option<WidgetId> {
        self.focus
    }

    /// Gives the keyboard focus to `focus`, or clears it.
    ///
    /// This is the programmatic entry point for opening a screen on a text
    /// field or for returning the focus after a modal closes. The target does
    /// not have to be visited yet: it keeps the focus until a later frame
    /// moves or clears it.
    pub fn set_focus(&mut self, focus: Option<WidgetId>) {
        self.focus = focus;
    }

    /// True when the pointer is over `rect` in the current frame.
    ///
    /// The pointer must be inside the viewport and inside the active clip.
    /// An empty rectangle is never hovered.
    #[must_use]
    pub fn is_hovered(&self, rect: Rect) -> bool {
        self.pointer
            .is_some_and(|position| self.point_is_visible(position) && rect.contains(position))
    }

    /// Reports the state of one widget and consumes a click if it is inside.
    ///
    /// The first widget whose rectangle contains the press wins, which is the
    /// usual immediate-mode rule for overlapping panels. A click also moves
    /// the keyboard focus to the widget. The widget is registered in this
    /// frame's focus order even when it is not clicked; `Tab` and `Shift+Tab`
    /// use that order on the next frame.
    ///
    /// Call this before reading the frame input in a widget that edits state:
    /// the returned response already reflects a click that focused the widget.
    pub fn response(&mut self, id: WidgetId, rect: Rect) -> UiResponse {
        let hovered = self.is_hovered(rect);
        let clicked = !self.click_consumed
            && self
                .primary_pressed
                .is_some_and(|position| self.point_is_visible(position) && rect.contains(position));
        if clicked {
            self.click_consumed = true;
            self.focus = Some(id);
        }
        if !self.frame_order.contains(&id) {
            self.frame_order.push(id);
        }
        let response = UiResponse {
            id,
            hovered,
            clicked,
            changed: false,
            focused: self.focus == Some(id),
        };
        self.responses.push(response);
        response
    }

    /// Marks the response stored for `id` in this frame as changed.
    ///
    /// Widgets call this after [`Self::response`] when they edited their state.
    /// The value returned by `response` is a copy, so the widget also sets
    /// [`UiResponse::changed`] on its own copy when it needs the flag right
    /// away; the next frame's [`UiOutput`] is the authoritative report.
    pub fn mark_changed(&mut self, id: WidgetId) {
        if let Some(response) = self
            .responses
            .iter_mut()
            .rev()
            .find(|response| response.id == id)
        {
            response.changed = true;
        }
    }

    /// Starts a clip scope and pushes the same region to the draw list.
    ///
    /// The stored clip is the intersection of `rect`, the viewport and every
    /// enclosing clip. This is the clip that [`Self::is_hovered`] and
    /// [`Self::response`] test, so a widget cannot be interacted with outside
    /// the region it draws. Close the scope with [`Self::pop_clip`].
    pub fn push_clip(&mut self, rect: Rect) {
        let parent = self.clip_rect();
        let clipped = parent
            .intersect(rect)
            .unwrap_or(Rect::new(parent.x, parent.y, 0.0, 0.0));
        self.draw.push_clip(clipped);
    }

    /// Ends the most recent clip scope.
    ///
    /// Popping without a matching [`Self::push_clip`] or
    /// [`DrawList::push_clip`] is a no-op; a scope left open when the frame
    /// ends is closed by [`Self::end_frame`].
    pub fn pop_clip(&mut self) {
        if self.draw.clip_depth() > 0 {
            self.draw.pop_clip();
        }
    }

    /// The effective clip of the current scope: the viewport when no clip is
    /// active, otherwise the innermost intersection.
    #[must_use]
    pub fn clip_rect(&self) -> Rect {
        self.draw.clip_rect().unwrap_or(self.viewport)
    }

    /// True when `position` is inside the viewport and the active clip.
    fn point_is_visible(&self, position: [f32; 2]) -> bool {
        self.viewport.contains(position)
            && self
                .draw
                .clip_rect()
                .is_none_or(|clip| clip.contains(position))
    }
}

/// The next focusable ID, wrapping at both ends.
///
/// A missing or stale current ID starts again at the first (forward) or the
/// last (backward) widget. An empty order clears the focus.
fn next_focus(
    current: Option<WidgetId>,
    order: &[WidgetId],
    direction: Traversal,
) -> Option<WidgetId> {
    if order.is_empty() {
        return None;
    }
    let position = current.and_then(|id| order.iter().position(|candidate| *candidate == id));
    let index = match (position, direction) {
        (Some(index), Traversal::Forward) => (index + 1) % order.len(),
        (Some(index), Traversal::Backward) => (index + order.len() - 1) % order.len(),
        (None, Traversal::Forward) => 0,
        (None, Traversal::Backward) => order.len() - 1,
    };
    order.get(index).copied()
}

#[cfg(test)]
mod tests {
    use super::Ui;
    use crate::geom::Rect;
    use crate::id::WidgetId;
    use crate::input::{InputEvent, Key, Modifiers, PointerButton, UiInput};

    fn click_input(position: [f32; 2]) -> UiInput {
        UiInput::new(vec![
            InputEvent::PointerMoved { position },
            InputEvent::PointerPressed {
                position,
                button: PointerButton::Left,
            },
        ])
    }

    fn tab_input(shift: bool) -> UiInput {
        UiInput {
            events: vec![InputEvent::KeyPressed(Key::Tab)],
            modifiers: Modifiers {
                shift,
                ..Modifiers::default()
            },
        }
    }

    #[test]
    fn a_click_inside_a_widget_is_reported_and_takes_focus() {
        let viewport = Rect::new(0.0, 0.0, 100.0, 100.0);
        let mut ui = Ui::new(viewport);
        ui.begin_frame(viewport, click_input([10.0, 10.0]));
        let id = ui.id("button");
        let response = ui.response(id, Rect::new(0.0, 0.0, 20.0, 20.0));
        assert!(response.hovered);
        assert!(response.clicked);
        assert!(response.focused);
        assert_eq!(ui.focus(), Some(id));
    }

    #[test]
    fn only_the_first_overlapping_widget_gets_the_click() {
        let viewport = Rect::new(0.0, 0.0, 100.0, 100.0);
        let mut ui = Ui::new(viewport);
        ui.begin_frame(viewport, click_input([10.0, 10.0]));
        let first = ui.id("first");
        let second = ui.id("second");
        let rect = Rect::new(0.0, 0.0, 20.0, 20.0);
        assert!(ui.response(first, rect).clicked);
        assert!(!ui.response(second, rect).clicked);
    }

    #[test]
    fn a_click_outside_does_not_take_focus() {
        let viewport = Rect::new(0.0, 0.0, 100.0, 100.0);
        let mut ui = Ui::new(viewport);
        ui.begin_frame(viewport, click_input([50.0, 50.0]));
        let id = ui.id("button");
        let response = ui.response(id, Rect::new(0.0, 0.0, 20.0, 20.0));
        assert!(!response.clicked);
        assert_eq!(ui.focus(), None);
    }

    #[test]
    fn id_scopes_are_stable_and_distinct() {
        let viewport = Rect::new(0.0, 0.0, 100.0, 100.0);
        let mut ui = Ui::new(viewport);
        ui.begin_frame(viewport, UiInput::default());
        ui.push_id("panel");
        let item = ui.id("item");
        ui.pop_id();
        assert_eq!(item, ui.id("panel").child("item"));
        assert_ne!(item, ui.id("item"));
    }

    #[test]
    fn a_root_id_is_the_label_id() {
        let viewport = Rect::new(0.0, 0.0, 100.0, 100.0);
        let ui = Ui::new(viewport);
        assert_eq!(ui.id("button"), WidgetId::from_label("button"));
    }

    #[test]
    fn the_frame_moves_the_draw_list_out() {
        let viewport = Rect::new(0.0, 0.0, 100.0, 100.0);
        let mut ui = Ui::new(viewport);
        ui.begin_frame(viewport, UiInput::default());
        ui.draw().rect(viewport, [1.0; 4]);
        let output = ui.end_frame();
        assert_eq!(output.draw_list.len(), 1);
        assert_eq!(output.responses.len(), 0);
        assert!(ui.draw().is_empty());
    }

    #[test]
    fn tab_uses_the_previous_frame_order() {
        let viewport = Rect::new(0.0, 0.0, 100.0, 100.0);
        let mut ui = Ui::new(viewport);
        let first = ui.id("first");
        let second = ui.id("second");
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);

        ui.begin_frame(viewport, UiInput::default());
        ui.response(first, rect);
        ui.response(second, rect);
        ui.end_frame();

        ui.begin_frame(viewport, tab_input(false));
        assert!(ui.response(first, rect).focused);
        assert!(!ui.response(second, rect).focused);
        ui.end_frame();

        ui.begin_frame(viewport, tab_input(false));
        assert!(!ui.response(first, rect).focused);
        assert!(ui.response(second, rect).focused);
        ui.end_frame();

        ui.begin_frame(viewport, tab_input(true));
        assert!(ui.response(first, rect).focused);
        assert!(!ui.response(second, rect).focused);
        let output = ui.end_frame();
        assert_eq!(output.focus, Some(first));
    }

    #[test]
    fn an_empty_click_clears_the_focus_and_syncs_responses() {
        let viewport = Rect::new(0.0, 0.0, 100.0, 100.0);
        let mut ui = Ui::new(viewport);
        let id = ui.id("button");
        let rect = Rect::new(0.0, 0.0, 20.0, 20.0);

        ui.begin_frame(viewport, click_input([10.0, 10.0]));
        assert!(ui.response(id, rect).clicked);
        ui.end_frame();

        ui.begin_frame(viewport, click_input([80.0, 80.0]));
        assert!(ui.response(id, rect).focused);
        let output = ui.end_frame();
        assert_eq!(output.focus, None);
        assert!(!output.responses[0].focused);
    }

    #[test]
    fn clipped_widgets_cannot_be_hovered() {
        let viewport = Rect::new(0.0, 0.0, 100.0, 100.0);
        let mut ui = Ui::new(viewport);
        let id = ui.id("row");
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);

        ui.begin_frame(viewport, click_input([50.0, 50.0]));
        ui.push_clip(Rect::new(0.0, 0.0, 20.0, 20.0));
        let response = ui.response(id, rect);
        assert!(!response.hovered);
        assert!(!response.clicked);
        ui.pop_clip();
        ui.end_frame();
    }

    #[test]
    fn mark_changed_updates_the_stored_response() {
        let viewport = Rect::new(0.0, 0.0, 100.0, 100.0);
        let mut ui = Ui::new(viewport);
        let id = ui.id("field");
        ui.begin_frame(viewport, UiInput::default());
        assert!(!ui.response(id, Rect::new(0.0, 0.0, 20.0, 20.0)).changed);
        ui.mark_changed(id);
        let output = ui.end_frame();
        assert!(output.responses[0].changed);
    }

    #[test]
    fn focus_is_dropped_when_the_widget_disappears() {
        let viewport = Rect::new(0.0, 0.0, 100.0, 100.0);
        let mut ui = Ui::new(viewport);
        let id = ui.id("field");
        let rect = Rect::new(0.0, 0.0, 20.0, 20.0);

        ui.begin_frame(viewport, click_input([5.0, 5.0]));
        assert!(ui.response(id, rect).clicked);
        let output = ui.end_frame();
        assert_eq!(output.focus, Some(id));

        ui.begin_frame(viewport, UiInput::default());
        let output = ui.end_frame();
        assert_eq!(output.focus, None);
    }
}

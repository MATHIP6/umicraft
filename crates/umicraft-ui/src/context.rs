// SPDX-License-Identifier: MIT OR Apache-2.0

//! The immediate-mode UI context: one frame in, one draw list out.
//!
//! [`Ui`] is owned by the render thread (or by the headless driver). Each
//! frame starts with [`Ui::begin_frame`], widgets append to the draw list and
//! return their [`UiResponse`], and [`Ui::end_frame`] moves the list out for
//! the renderer. No state is shared with the game, network or mesh threads.

use crate::draw::DrawList;
use crate::geom::Rect;
use crate::id::WidgetId;
use crate::input::{InputEvent, PointerButton, UiInput};

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

/// The per-frame immediate-mode state.
#[derive(Debug, Clone)]
pub struct Ui {
    viewport: Rect,
    input: UiInput,
    draw: DrawList,
    responses: Vec<UiResponse>,
    focus: Option<WidgetId>,
    id_stack: Vec<WidgetId>,
    pointer: Option<[f32; 2]>,
    primary_pressed: Option<[f32; 2]>,
    click_consumed: bool,
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
            id_stack: Vec::new(),
            pointer: None,
            primary_pressed: None,
            click_consumed: false,
        }
    }

    /// Starts a frame, replacing the viewport and the input of the old one.
    ///
    /// The draw list and the per-frame responses are cleared; the focus and
    /// the allocations survive across frames.
    pub fn begin_frame(&mut self, viewport: Rect, input: UiInput) {
        self.viewport = viewport;
        self.input = input;
        self.draw.clear();
        self.responses.clear();
        self.id_stack.clear();
        self.pointer = None;
        self.primary_pressed = None;
        self.click_consumed = false;

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
                InputEvent::Wheel { .. }
                | InputEvent::KeyPressed(_)
                | InputEvent::KeyReleased(_)
                | InputEvent::Text(_) => {}
            }
        }
    }

    /// Ends the frame and hands the draw list and the responses to the caller.
    pub fn end_frame(&mut self) -> UiOutput {
        let output = UiOutput {
            draw_list: std::mem::take(&mut self.draw),
            responses: std::mem::take(&mut self.responses),
            focus: self.focus,
        };
        self.pointer = None;
        self.primary_pressed = None;
        self.click_consumed = false;
        output
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
    pub fn draw(&mut self) -> &mut DrawList {
        &mut self.draw
    }

    /// Pushes a child scope onto the ID stack.
    pub fn push_id(&mut self, label: &str) {
        let parent = self.id_stack.last().copied().unwrap_or_default();
        self.id_stack.push(parent.child(label));
    }

    /// Pops the most recent ID scope.
    pub fn pop_id(&mut self) {
        self.id_stack.pop();
    }

    /// Derives a widget ID from the current ID scope.
    #[must_use]
    pub fn id(&self, label: &str) -> WidgetId {
        self.id_stack
            .last()
            .copied()
            .unwrap_or_default()
            .child(label)
    }

    /// The widget that owns the keyboard focus.
    #[must_use]
    pub fn focus(&self) -> Option<WidgetId> {
        self.focus
    }

    /// Gives the keyboard focus to `focus`, or clears it.
    pub fn set_focus(&mut self, focus: Option<WidgetId>) {
        self.focus = focus;
    }

    /// True when the pointer is over `rect` in the current frame.
    #[must_use]
    pub fn is_hovered(&self, rect: Rect) -> bool {
        self.pointer
            .is_some_and(|position| self.viewport.contains(position) && rect.contains(position))
    }

    /// Reports the state of one widget and consumes a click if it is inside.
    ///
    /// The first widget whose rectangle contains the press wins, which is the
    /// usual immediate-mode rule for overlapping panels. A click also moves
    /// the keyboard focus to the widget.
    pub fn response(&mut self, id: WidgetId, rect: Rect) -> UiResponse {
        let hovered = self.is_hovered(rect);
        let clicked = hovered
            && !self.click_consumed
            && self
                .primary_pressed
                .is_some_and(|position| rect.contains(position));
        if clicked {
            self.click_consumed = true;
            self.focus = Some(id);
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
}

#[cfg(test)]
mod tests {
    use super::Ui;
    use crate::geom::Rect;
    use crate::input::{InputEvent, PointerButton, UiInput};

    fn click_input(position: [f32; 2]) -> UiInput {
        UiInput::new(vec![
            InputEvent::PointerMoved { position },
            InputEvent::PointerPressed {
                position,
                button: PointerButton::Left,
            },
        ])
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
}

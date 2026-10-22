// SPDX-License-Identifier: MIT OR Apache-2.0

//! The scrollable list widget.
//!
//! One list is a single focusable widget: it registers one response, owns one
//! selection and clips its rows to one viewport. Rows are painted cells rather
//! than separate widgets, so pressing Tab does not walk every item and a list
//! of a hundred servers still has a three-stop focus chain per screen.
//!
//! Rows are addressed by their index in the `items` slice. The caller must
//! keep that slice order stable between frames for the selection to follow a
//! row; the index is the row identity in the M4 state model. The state is
//! owned by the caller, so two lists never share a selection.
//!
//! The list handles:
//!
//! - the wheel while the pointer is over the viewport;
//! - Up and Down while the list owns the focus: they move the selection and
//!   scroll the selected row into view;
//! - selection by a primary click on a visible row;
//! - clipping, row culling and a proportional scrollbar.
//!
//! The scroll offset is clamped after a resize or an item-count change, and a
//! selection that no longer exists is dropped.

use crate::context::{Ui, UiResponse};
use crate::draw::TextStyle;
use crate::geom::{Rect, ScrollArea};
use crate::id::WidgetId;
use crate::input::{InputEvent, Key};

use super::style;
use super::util;

/// Height of one list row in physical pixels.
///
/// The value is fixed rather than derived from the font so the scroll maths
/// and the row culling are pure geometry. It fits both the 16-pixel fallback
/// line height at the widget text scale and the 18-pixel real 1.8 font.
pub const ROW_HEIGHT: f32 = 24.0;

/// Width of the scrollbar in physical pixels.
pub const SCROLLBAR_WIDTH: f32 = 4.0;

/// Gap between the row area and the scrollbar, in pixels.
const SCROLLBAR_GAP: f32 = 2.0;
/// Smallest visible scrollbar thumb, in pixels.
const MIN_THUMB_HEIGHT: f32 = 12.0;
/// Horizontal padding between a row edge and its label, in pixels.
const ROW_PADDING: f32 = 6.0;
/// Vertical offset of the row label, chosen to centre the widget text scale in
/// a [`ROW_HEIGHT`] row.
const ROW_TEXT_OFFSET: f32 = 4.0;

/// The scroll and selection state of one list.
///
/// The offset and the selection survive across frames. Both are validated
/// against the item slice on the next [`list`] call, which makes it safe for
/// the caller to replace the items, resize the viewport or set a stale index.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ListState {
    offset: f32,
    selected: Option<usize>,
}

impl ListState {
    /// Creates a list scrolled to the top with no selection.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The vertical scroll offset in pixels; zero is the first row.
    #[must_use]
    pub fn offset(&self) -> f32 {
        self.offset
    }

    /// Sets the scroll offset, clamped to a finite non-negative value.
    ///
    /// The upper bound depends on the item count and the viewport, so it is
    /// applied by the next [`list`] call.
    pub fn set_offset(&mut self, offset: f32) {
        self.offset = if offset.is_finite() {
            offset.max(0.0)
        } else {
            0.0
        };
    }

    /// The selected row index, if any.
    #[must_use]
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Replaces the selection.
    ///
    /// An index outside the item slice is dropped by the next [`list`] call.
    pub fn set_selected(&mut self, selection: Option<usize>) {
        self.selected = selection;
    }

    /// Selects `index`; the next [`list`] call drops it when it is out of
    /// range.
    pub fn select(&mut self, index: usize) {
        self.selected = Some(index);
    }
}

/// Draws a scrollable list and applies this frame's scrolling and selection.
///
/// `id` must stay stable across frames. The function calls [`Ui::response`]
/// exactly once, before it reads the input events. Wheel input scrolls only
/// when the pointer is over the list; keyboard input is read only while the
/// list owns the focus. A click that the list wins selects the row under the
/// pointer (the scrollbar is a passive affordance in M4).
///
/// [`UiResponse::changed`] is set when the selection or the scroll offset
/// changed in this frame, including an offset clamped after a resize or an
/// item-count change. The same flag is stored with [`Ui::mark_changed`] so
/// [`crate::UiOutput::responses`] reports it as well.
///
/// Rows outside the viewport are culled before they are drawn, and the
/// visible rows are clipped to the row area, which excludes the scrollbar.
///
/// # Example
///
/// ```
/// use umicraft_ui::widgets::{list, ListState};
/// use umicraft_ui::{Rect, Ui, UiInput};
///
/// let viewport = Rect::new(0.0, 0.0, 320.0, 180.0);
/// let mut ui = Ui::new(viewport);
/// let mut state = ListState::new();
/// let items = vec!["FIRST".to_owned(), "SECOND".to_owned()];
///
/// ui.begin_frame(viewport, UiInput::default());
/// let id = ui.id("items");
/// let response = list(
///     &mut ui,
///     id,
///     Rect::new(10.0, 10.0, 160.0, 60.0),
///     &items,
///     &mut state,
/// );
/// assert!(!response.changed);
/// ui.end_frame();
/// ```
pub fn list(
    ui: &mut Ui,
    id: WidgetId,
    rect: Rect,
    items: &[String],
    state: &mut ListState,
) -> UiResponse {
    let mut response = ui.response(id, rect);
    let mut changed = false;

    // Item-count changes can invalidate a selection and, through the content
    // height, the scroll offset. Drop the stale selection before laying out.
    if state
        .selected
        .is_some_and(|selected| selected >= items.len())
    {
        state.selected = None;
        changed = true;
    }

    let content_height = items.len() as f32 * ROW_HEIGHT;
    let scrollbar = content_height > rect.height;
    let reserved = if scrollbar {
        SCROLLBAR_WIDTH + SCROLLBAR_GAP
    } else {
        0.0
    };
    let row_area = Rect::new(
        rect.x,
        rect.y,
        (rect.width - reserved).max(0.0),
        rect.height,
    );

    let mut scroll = ScrollArea::new(row_area);
    scroll.set_content_height(content_height);
    scroll.set_offset(state.offset);

    if response.hovered {
        let lines: f32 = ui
            .input()
            .events
            .iter()
            .filter_map(|event| match event {
                InputEvent::Wheel { delta } => Some(*delta),
                _ => None,
            })
            .sum();
        if lines != 0.0 {
            scroll.scroll_by(lines * ROW_HEIGHT);
        }
    }

    if response.focused && !items.is_empty() {
        for event in &ui.input().events {
            if let InputEvent::KeyPressed(Key::ArrowDown) = event {
                let next = state
                    .selected
                    .map_or(0, |selected| (selected + 1).min(items.len() - 1));
                if state.selected != Some(next) {
                    state.selected = Some(next);
                    changed = true;
                }
                scroll_into_view(&mut scroll, next);
            } else if let InputEvent::KeyPressed(Key::ArrowUp) = event {
                let previous = state
                    .selected
                    .map_or(items.len() - 1, |selected| selected.saturating_sub(1));
                if state.selected != Some(previous) {
                    state.selected = Some(previous);
                    changed = true;
                }
                scroll_into_view(&mut scroll, previous);
            }
        }
    }

    if response.clicked {
        if let Some(position) = util::last_primary_press(ui) {
            if row_area.contains(position) {
                let local_y = position[1] - row_area.y + scroll.offset();
                let index = (local_y / ROW_HEIGHT).floor() as usize;
                if index < items.len() && state.selected != Some(index) {
                    state.selected = Some(index);
                    changed = true;
                }
            }
        }
    }

    if (scroll.offset() - state.offset).abs() > f32::EPSILON {
        changed = true;
    }
    state.offset = scroll.offset();

    if !rect.is_empty() {
        ui.draw().rect(rect, style::LIST_BACKGROUND);
        if !row_area.is_empty() {
            ui.push_clip(row_area);
            draw_rows(ui, row_area, items, state.selected, scroll.offset());
            ui.pop_clip();
            draw_scrollbar(ui, rect, row_area, content_height, scroll.offset());
        }
    }

    if changed {
        response.changed = true;
        ui.mark_changed(id);
    }
    response
}

/// Draws the rows that overlap `row_area`, selecting and highlighting cells.
///
/// The first and last visible indices are derived from the offset so that a
/// long list costs one rectangle and one text run per visible row, not per
/// item. Rows partially above or below the viewport are still drawn; the clip
/// pushed by [`list`] cuts them at the edge.
fn draw_rows(ui: &mut Ui, row_area: Rect, items: &[String], selected: Option<usize>, offset: f32) {
    let first = (offset / ROW_HEIGHT).floor().max(0.0) as usize;
    let last =
        (((offset + row_area.height) / ROW_HEIGHT).ceil().max(0.0) as usize).min(items.len());

    for index in first.min(last)..last {
        let row = Rect::new(
            row_area.x,
            row_area.y + index as f32 * ROW_HEIGHT - offset,
            row_area.width,
            ROW_HEIGHT,
        );
        if selected == Some(index) {
            ui.draw().rect(row, style::LIST_ROW_SELECTED);
        } else if ui.is_hovered(row) {
            ui.draw().rect(row, style::LIST_ROW_HOVER);
        }
        if let Some(item) = items.get(index) {
            ui.draw().text(
                [row.x + ROW_PADDING, row.y + ROW_TEXT_OFFSET],
                item.as_str(),
                TextStyle::new(style::TEXT_SCALE, style::TEXT),
            );
        }
    }
}

/// Draws the scrollbar groove and thumb when the content is taller than the
/// viewport.
fn draw_scrollbar(ui: &mut Ui, rect: Rect, row_area: Rect, content_height: f32, offset: f32) {
    if content_height <= row_area.height || row_area.is_empty() {
        return;
    }
    let track = Rect::new(
        rect.right() - SCROLLBAR_WIDTH,
        rect.y,
        SCROLLBAR_WIDTH.min(rect.width),
        rect.height,
    );
    ui.draw().rect(track, style::SCROLLBAR_TRACK);

    let visible_ratio = row_area.height / content_height;
    let thumb_height =
        (track.height * visible_ratio).clamp(MIN_THUMB_HEIGHT.min(track.height), track.height);
    let max_offset = (content_height - row_area.height).max(0.0);
    let progress = if max_offset > 0.0 {
        (offset / max_offset).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let thumb = Rect::new(
        track.x,
        track.y + progress * (track.height - thumb_height),
        track.width,
        thumb_height,
    );
    ui.draw().rect(thumb, style::SCROLLBAR_THUMB);
}

/// Scrolls `scroll` just enough to show the row at `index` completely.
fn scroll_into_view(scroll: &mut ScrollArea, index: usize) {
    let top = index as f32 * ROW_HEIGHT;
    let bottom = top + ROW_HEIGHT;
    let viewport = scroll.viewport();
    let offset = scroll.offset();
    if top < offset {
        scroll.set_offset(top);
    } else if bottom > offset + viewport.height {
        scroll.set_offset(bottom - viewport.height);
    }
}

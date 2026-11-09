// SPDX-License-Identifier: MIT OR Apache-2.0

//! The scrollbar of the lists of the mods menu: a slim thumb in the margin right
//! of the list, which the pointer can grab and drag, or click on its track to jump.

use crate::context::Ui;
use crate::geom::Rect;
use crate::input::{InputEvent, PointerButton};
use crate::menus::common::Gui;

use super::motion::Motion;

/// Width of the thumb at rest and under the pointer, in GUI units.
const THIN: f32 = 1.5;
const THICK: f32 = 2.5;
/// The zone that catches the pointer: where it starts right of the list and how
/// wide it is, in GUI units.
const GRAB_FROM: f32 = 1.0;
const GRAB_WIDTH: f32 = 7.0;
/// The shortest thumb, in GUI units.
const MIN_THUMB: f32 = 8.0;

/// A drag of the thumb under way: how far below the top of the thumb the pointer
/// holds it, in physical pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct BarGrab(Option<f32>);

impl BarGrab {
    /// Whether the thumb is being dragged.
    pub(super) fn is_active(self) -> bool {
        self.0.is_some()
    }
}

/// Where the scrollbar of a list shows, in physical pixels.
struct Geometry {
    /// The zone that catches the pointer.
    track: Rect,
    /// The height of the thumb.
    thumb: f32,
    /// The largest scroll offset, in GUI units.
    max: f32,
}

/// The scrollbar of `area` for content `height` GUI units tall, or `None` when the
/// content fits.
fn geometry(area: Rect, height: f32, scale: f32) -> Option<Geometry> {
    let view = area.height / scale;
    if height <= view {
        return None;
    }
    Some(Geometry {
        track: Rect::new(
            area.right() + GRAB_FROM * scale,
            area.y,
            GRAB_WIDTH * scale,
            area.height,
        ),
        thumb: (area.height * view / height).max(MIN_THUMB * scale),
        max: height - view,
    })
}

/// Moves the scroll `offset` (GUI units) with the pointer over the scrollbar of
/// `area`: a press on the thumb holds it where it was caught, a press on the track
/// centres the thumb on the pointer, and the drag follows the pointer until the
/// release. Returns the new offset.
pub(super) fn scroll_input(
    ui: &Ui,
    area: Rect,
    height: f32,
    offset: f32,
    grab: &mut BarGrab,
) -> f32 {
    let scale = Gui::new(ui.viewport()).scale;
    let Some(bar) = geometry(area, height, scale) else {
        grab.0 = None;
        return offset;
    };
    let travel = (area.height - bar.thumb).max(1.0);
    let thumb_top = |offset: f32| area.y + travel * (offset / bar.max).clamp(0.0, 1.0);
    let to_offset = |y: f32, held: f32| ((y - area.y - held) / travel).clamp(0.0, 1.0) * bar.max;
    let mut offset = offset;
    for event in &ui.input().events {
        match event {
            InputEvent::PointerPressed {
                position,
                button: PointerButton::Left,
            } if bar.track.contains(*position) => {
                let top = thumb_top(offset);
                let held = if (top..top + bar.thumb).contains(&position[1]) {
                    position[1] - top
                } else {
                    bar.thumb * 0.5
                };
                grab.0 = Some(held);
                offset = to_offset(position[1], held);
            }
            InputEvent::PointerMoved { position } => {
                if let Some(held) = grab.0 {
                    offset = to_offset(position[1], held);
                }
            }
            InputEvent::PointerReleased {
                button: PointerButton::Left,
                ..
            } => grab.0 = None,
            _ => {}
        }
    }
    offset
}

/// Draws the thumb at the scroll `offset` (GUI units) shown. It widens and
/// brightens under the pointer and while it is dragged.
pub(super) fn draw_scroll_bar(
    ui: &mut Ui,
    area: Rect,
    height: f32,
    offset: f32,
    (grab, motion): (BarGrab, &mut Motion),
) {
    let scale = Gui::new(ui.viewport()).scale;
    let Some(bar) = geometry(area, height, scale) else {
        return;
    };
    let over = ui.pointer().is_some_and(|p| bar.track.contains(p));
    let level = motion.hover("scroll_bar", over || grab.is_active());
    let progress = (offset / bar.max).clamp(0.0, 1.0);
    let width = (THIN + (THICK - THIN) * level) * scale;
    let rect = Rect::new(
        area.right() + 3.0 * scale + (THIN * scale - width) * 0.5,
        area.y + (area.height - bar.thumb) * progress,
        width,
        bar.thumb,
    );
    let alpha = 0.22 + 0.33 * level;
    ui.draw()
        .rounded_rect(rect, width * 0.5, [1.0, 1.0, 1.0, alpha]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::UiInput;
    use crate::menus::common::testing::frame;

    /// A list area of 100 x 60 GUI units at scale 3, in physical pixels. The
    /// viewport of the tests is 427 x 240 units, which is also a scale of 3.
    fn area() -> Rect {
        Rect::new(30.0, 30.0, 300.0, 180.0)
    }

    fn scrolled(events: Vec<InputEvent>, offset: f32, grab: &mut BarGrab) -> f32 {
        frame(UiInput::new(events), |ui| {
            scroll_input(ui, area(), 200.0, offset, grab)
        })
    }

    fn press(at: [f32; 2]) -> InputEvent {
        InputEvent::PointerPressed {
            position: at,
            button: PointerButton::Left,
        }
    }

    fn release(at: [f32; 2]) -> InputEvent {
        InputEvent::PointerReleased {
            position: at,
            button: PointerButton::Left,
        }
    }

    #[test]
    fn content_that_fits_has_no_bar() {
        assert!(geometry(area(), 50.0, 3.0).is_none());
        assert!(geometry(area(), 200.0, 3.0).is_some());
    }

    #[test]
    fn a_press_on_the_track_jumps_there_and_a_drag_follows_the_pointer() {
        let bar = geometry(area(), 200.0, 3.0).expect("a bar");
        let x = bar.track.x + 2.0;
        let mut grab = BarGrab::default();
        // The middle of the track puts the thumb in the middle: half of the offset.
        let middle = area().y + area().height * 0.5;
        let offset = scrolled(vec![press([x, middle])], 0.0, &mut grab);
        assert!((offset - bar.max * 0.5).abs() < 1.0, "{offset}");
        assert!(grab.is_active());
        // Dragging below the bottom reaches the end, and the release lets go.
        let offset = scrolled(
            vec![
                InputEvent::PointerMoved {
                    position: [x, area().bottom() + 40.0],
                },
                release([x, area().bottom()]),
            ],
            offset,
            &mut grab,
        );
        assert_eq!(offset, bar.max, "the end of the list");
        assert!(!grab.is_active());
    }

    #[test]
    fn a_press_beside_the_track_does_not_scroll() {
        let mut grab = BarGrab::default();
        let at = [area().x + 10.0, area().y + 10.0];
        assert_eq!(scrolled(vec![press(at)], 12.0, &mut grab), 12.0);
        assert!(!grab.is_active());
    }

    #[test]
    fn the_thumb_keeps_where_it_was_caught() {
        let bar = geometry(area(), 200.0, 3.0).expect("a bar");
        let x = bar.track.x + 2.0;
        let mut grab = BarGrab::default();
        // The thumb is at the top; it is caught 10 pixels below its top edge.
        let top = area().y;
        let offset = scrolled(vec![press([x, top + 10.0])], 0.0, &mut grab);
        assert_eq!(offset, 0.0, "catching it does not move it");
        let moved = scrolled(
            vec![InputEvent::PointerMoved {
                position: [x, top + 10.0 + 20.0],
            }],
            offset,
            &mut grab,
        );
        let travel = area().height - bar.thumb;
        assert!((moved - 20.0 / travel * bar.max).abs() < 1e-3, "{moved}");
    }
}

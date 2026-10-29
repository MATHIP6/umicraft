// SPDX-License-Identifier: MIT OR Apache-2.0

//! A horizontal slider (M10, UI-3): a bar with a handle that the pointer
//! drags and the arrow keys nudge.
//!
//! The slider keeps no value: the caller owns it, passes it in as a fraction
//! in `0.0..=1.0` and receives the new one. The only state the widget needs
//! is whether a drag is under way, kept in a [`SliderState`] that the caller
//! stores next to the value. That is what lets a drag continue while the
//! pointer leaves the bar, and end on the release even outside of it.

use crate::context::Ui;
use crate::draw::TextStyle;
use crate::geom::Rect;
use crate::id::WidgetId;
use crate::input::{InputEvent, Key, PointerButton};
use crate::text::FontAtlas;

use super::style;

/// Width of the handle, in pixels.
const HANDLE_WIDTH: f32 = 8.0;
/// How much an arrow key moves the value.
const KEY_STEP: f32 = 0.05;

/// Whether a drag of the slider is under way.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SliderState {
    dragging: bool,
}

impl SliderState {
    /// Whether the pointer is dragging the handle.
    #[must_use]
    pub fn is_dragging(self) -> bool {
        self.dragging
    }
}

/// Draws a slider with `label` on it and returns the new value and whether
/// it changed in this frame.
///
/// `value` is a fraction in `0.0..=1.0`; anything else is clamped, and a
/// value that is not a number counts as `0.0`.
///
/// ```
/// use umicraft_ui::widgets::{slider, SliderState};
/// use umicraft_ui::{FontAtlas, Rect, Ui, UiInput};
///
/// let viewport = Rect::new(0.0, 0.0, 320.0, 180.0);
/// let mut ui = Ui::new(viewport);
/// ui.begin_frame(viewport, UiInput::default());
/// let id = ui.id("volume");
/// let mut state = SliderState::default();
/// let (value, changed) = slider(
///     &mut ui,
///     id,
///     Rect::new(10.0, 10.0, 200.0, 24.0),
///     &FontAtlas::fallback(),
///     "VOLUME",
///     0.5,
///     &mut state,
/// );
/// assert_eq!(value, 0.5);
/// assert!(!changed);
/// ui.end_frame();
/// ```
pub fn slider(
    ui: &mut Ui,
    id: WidgetId,
    rect: Rect,
    atlas: &FontAtlas,
    label: &str,
    value: f32,
    state: &mut SliderState,
) -> (f32, bool) {
    let response = ui.response(id, rect);
    // With the real texture the handle and text follow the GUI scale, as in vanilla.
    let skin = if ui.widgets_sheet().is_some() {
        (rect.height / 20.0).round().max(1.0)
    } else {
        1.0
    };
    let handle_width = HANDLE_WIDTH * skin;
    let travel = (rect.width - handle_width).max(1.0);
    let to_value = |x: f32| ((x - rect.x - handle_width * 0.5) / travel).clamp(0.0, 1.0);

    let start = if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mut value = start;
    // The events are copied so the value can be edited while reading them.
    let events: Vec<InputEvent> = ui.input().events.clone();
    for event in &events {
        match event {
            InputEvent::PointerPressed {
                position,
                button: PointerButton::Left,
            } if rect.contains(*position) => {
                state.dragging = true;
                value = to_value(position[0]);
            }
            InputEvent::PointerMoved { position } if state.dragging => {
                value = to_value(position[0]);
            }
            InputEvent::PointerReleased {
                position,
                button: PointerButton::Left,
            } => {
                if state.dragging {
                    value = to_value(position[0]);
                }
                state.dragging = false;
            }
            InputEvent::KeyPressed(Key::ArrowLeft) if response.focused => {
                value = (value - KEY_STEP).max(0.0);
            }
            InputEvent::KeyPressed(Key::ArrowRight) if response.focused => {
                value = (value + KEY_STEP).min(1.0);
            }
            _ => {}
        }
    }
    let changed = (value - start).abs() > f32::EPSILON;
    if changed {
        ui.mark_changed(id);
    }

    if let Some(sheet) = ui.widgets_sheet() {
        // The texture is cut in texels (GUI units), then scaled, as vanilla does.
        let texels = (rect.width / skin).round().min(400.0);
        let half = (texels / 2.0).floor();
        let other = texels - half;
        let src_left = sheet.sub(0, 46, half as u32, 20);
        let src_right = sheet.sub(200 - other as u32, 46, other as u32, 20);
        let dst_left = Rect::new(rect.x, rect.y, half * skin, rect.height);
        let dst_right = Rect::new(
            rect.x + half * skin,
            rect.y,
            rect.width - half * skin,
            rect.height,
        );
        ui.draw().image(src_left, dst_left, [1.0; 4]);
        ui.draw().image(src_right, dst_right, [1.0; 4]);

        let handle_v_y = if response.hovered || state.dragging {
            86
        } else {
            66
        };
        let handle_x = rect.x + value * travel;
        let handle_left = Rect::new(handle_x, rect.y, 4.0 * skin, rect.height);
        let handle_right = Rect::new(handle_x + 4.0 * skin, rect.y, 4.0 * skin, rect.height);
        let src_h_left = sheet.sub(0, handle_v_y, 4, 20);
        let src_h_right = sheet.sub(196, handle_v_y, 4, 20);
        ui.draw().image(src_h_left, handle_left, [1.0; 4]);
        ui.draw().image(src_h_right, handle_right, [1.0; 4]);
    } else {
        let fill = if response.hovered || state.dragging {
            style::BUTTON_HOVER
        } else {
            style::BUTTON
        };
        ui.draw().rect(rect, fill);
        let handle = Rect::new(rect.x + value * travel, rect.y, handle_width, rect.height);
        ui.draw().rect(handle, style::BUTTON_PRESSED);
    }

    if !label.is_empty() && !rect.is_empty() {
        let pad = 6.0;
        let available = (rect.width - pad).max(0.0);
        let mut scale = if skin > 1.0 || ui.widgets_sheet().is_some() {
            skin
        } else {
            style::TEXT_SCALE
        };
        let mut width = atlas.measure(label, scale);
        if width > available && available > 0.0 {
            scale = (scale * available / width).max(1.0);
            width = atlas.measure(label, scale);
        }
        let line_height = if ui.widgets_sheet().is_some() {
            8.0 * scale
        } else {
            atlas.line_height(scale)
        };
        let pos = [
            (rect.x + (rect.width - width) * 0.5).round(),
            (rect.y + (rect.height - line_height) * 0.5).round(),
        ];
        if ui.widgets_sheet().is_some() {
            ui.draw().text(
                [pos[0] + scale, pos[1] + scale],
                label,
                TextStyle::new(scale, [0.22, 0.22, 0.22, 1.0]),
            );
            ui.draw().text(
                pos,
                label,
                TextStyle::new(scale, [0.878, 0.878, 0.878, 1.0]),
            );
        } else {
            ui.draw()
                .text(pos, label, TextStyle::new(scale, style::TEXT));
        }
    }
    (value, changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::UiInput;

    const BAR: Rect = Rect {
        x: 10.0,
        y: 10.0,
        width: 108.0,
        height: 20.0,
    };

    fn frame(events: Vec<InputEvent>, value: f32, state: &mut SliderState) -> (f32, bool) {
        let viewport = Rect::new(0.0, 0.0, 400.0, 200.0);
        let mut ui = Ui::new(viewport);
        ui.begin_frame(viewport, UiInput::new(events));
        let id = ui.id("slider");
        let result = slider(&mut ui, id, BAR, &FontAtlas::fallback(), "X", value, state);
        ui.end_frame();
        result
    }

    fn press(x: f32) -> InputEvent {
        InputEvent::PointerPressed {
            position: [x, 20.0],
            button: PointerButton::Left,
        }
    }

    fn moved(x: f32) -> InputEvent {
        InputEvent::PointerMoved {
            position: [x, 20.0],
        }
    }

    fn release(x: f32) -> InputEvent {
        InputEvent::PointerReleased {
            position: [x, 20.0],
            button: PointerButton::Left,
        }
    }

    /// The x of the centre of the handle for a value, on `BAR`.
    fn x_of(value: f32) -> f32 {
        BAR.x + HANDLE_WIDTH * 0.5 + value * (BAR.width - HANDLE_WIDTH)
    }

    #[test]
    fn an_idle_slider_keeps_its_value() {
        let mut state = SliderState::default();
        assert_eq!(frame(vec![], 0.3, &mut state), (0.3, false));
    }

    #[test]
    fn a_press_sets_the_value_under_the_pointer() {
        let mut state = SliderState::default();
        let (value, changed) = frame(vec![press(x_of(0.75))], 0.0, &mut state);
        assert!((value - 0.75).abs() < 1e-4, "{value}");
        assert!(changed);
        assert!(state.is_dragging());
    }

    #[test]
    fn a_drag_follows_the_pointer_even_outside_the_bar_and_ends_on_release() {
        let mut state = SliderState::default();
        frame(vec![press(x_of(0.5))], 0.0, &mut state);
        // The pointer leaves the bar to the right and above: the value pins to 1.
        let (value, _) = frame(vec![moved(900.0)], 0.5, &mut state);
        assert_eq!(value, 1.0);
        let (value, _) = frame(vec![moved(-50.0)], 1.0, &mut state);
        assert_eq!(value, 0.0);
        let (value, _) = frame(vec![release(x_of(0.25))], 0.0, &mut state);
        assert!((value - 0.25).abs() < 1e-4);
        assert!(!state.is_dragging());
        // After the release the pointer no longer moves the value.
        assert_eq!(
            frame(vec![moved(x_of(0.9))], 0.25, &mut state),
            (0.25, false)
        );
    }

    #[test]
    fn a_press_outside_the_bar_does_not_start_a_drag() {
        let mut state = SliderState::default();
        let result = frame(vec![press(300.0), moved(x_of(0.5))], 0.2, &mut state);
        assert_eq!(result, (0.2, false));
        assert!(!state.is_dragging());
    }

    #[test]
    fn the_arrow_keys_nudge_a_focused_slider() {
        let mut state = SliderState::default();
        let viewport = Rect::new(0.0, 0.0, 400.0, 200.0);
        let mut ui = Ui::new(viewport);
        // A click focuses the slider; the key comes in the next frame.
        ui.begin_frame(
            viewport,
            UiInput::new(vec![press(x_of(0.5)), release(x_of(0.5))]),
        );
        let id = ui.id("slider");
        slider(
            &mut ui,
            id,
            BAR,
            &FontAtlas::fallback(),
            "X",
            0.5,
            &mut state,
        );
        ui.end_frame();
        ui.begin_frame(
            viewport,
            UiInput::new(vec![InputEvent::KeyPressed(Key::ArrowRight)]),
        );
        let id = ui.id("slider");
        let (value, changed) = slider(
            &mut ui,
            id,
            BAR,
            &FontAtlas::fallback(),
            "X",
            0.5,
            &mut state,
        );
        assert!(changed);
        assert!((value - 0.55).abs() < 1e-4, "{value}");
    }

    #[test]
    fn an_unfocused_slider_ignores_the_arrow_keys() {
        let mut state = SliderState::default();
        let events = vec![InputEvent::KeyPressed(Key::ArrowRight)];
        assert_eq!(frame(events, 0.5, &mut state), (0.5, false));
    }

    #[test]
    fn out_of_range_and_non_numeric_values_are_tamed() {
        let mut state = SliderState::default();
        assert_eq!(frame(vec![], 7.0, &mut state).0, 1.0);
        assert_eq!(frame(vec![], -3.0, &mut state).0, 0.0);
        assert_eq!(frame(vec![], f32::NAN, &mut state).0, 0.0);
    }

    #[test]
    fn slider_with_skin_draws_images() {
        use crate::draw::SpriteSheet;
        let mut state = SliderState::default();
        let viewport = Rect::new(0.0, 0.0, 400.0, 200.0);
        let mut ui = Ui::new(viewport);
        let sheet = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 256, 256);
        ui.set_widgets_sheet(Some(sheet));
        ui.begin_frame(viewport, UiInput::default());

        let id = ui.id("slider");
        slider(
            &mut ui,
            id,
            BAR,
            &FontAtlas::fallback(),
            "SLIDER",
            0.5,
            &mut state,
        );
        let output = ui.end_frame();
        let images = output
            .draw_list
            .commands()
            .iter()
            .filter(|cmd| matches!(cmd, crate::draw::DrawCommand::Image { .. }))
            .count();
        // 2 images for track, 2 images for handle = 4 images
        assert_eq!(images, 4);
    }
}

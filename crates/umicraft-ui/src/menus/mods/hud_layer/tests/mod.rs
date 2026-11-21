// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the HUD layer: the mouse, the wheel and the blocked zones. The pure
//! drag logic is in `logic`.

mod gear;
mod logic;
mod wheel;

use super::*;
use crate::draw::DrawCommand;
use crate::input::UiInput;
use crate::menus::common::testing::frame;
use crate::mod_widget::ModLine;

/// The viewport of the tests is 427 x 240 GUI units at scale 3.
const SCREEN: [f32; 2] = [427.0, 240.0];
const SCALE: f32 = 3.0;

/// Pulls a rectangle to the top-left screen edges within 4 units, like the
/// mods crate does (which also knows the right edge and the centre).
fn snap(rect: Rect, _screen: [f32; 2]) -> Rect {
    let near = |v: f32| if v.abs() <= 4.0 { 0.0 } else { v };
    Rect::new(near(rect.x), near(rect.y), rect.width, rect.height)
}

fn widget(id: &str, position: [f32; 2], scale: f32) -> HudWidgetView {
    HudWidgetView {
        id: id.into(),
        name: format!("Name {id}"),
        widget: ModWidgetView {
            lines: vec![ModLine::new("60 FPS", 0xFFFFFF)],
            position,
            scale,
            ..ModWidgetView::default()
        },
    }
}

fn view() -> HudEditorView {
    HudEditorView {
        widgets: vec![
            widget("fps", [100.0, 100.0], 1.0),
            widget("cps", [200.0, 100.0], 1.0),
        ],
        screen: SCREEN,
        scale_range: (0.5, 3.0),
        ..HudEditorView::default()
    }
}

fn rect_of(view: &HudEditorView, index: usize) -> Rect {
    widget_rect(&FontAtlas::fallback(), &view.widgets[index].widget)
}

fn px(point: [f32; 2]) -> [f32; 2] {
    [point[0] * SCALE, point[1] * SCALE]
}

/// One frame of the layer with nothing blocked.
fn show(screen: &mut HudLayer, view: &HudEditorView, input: UiInput) -> Option<HudEditorOutcome> {
    show_blocking(screen, view, input, &[])
}

/// One frame of the layer with `blocked` protected.
fn show_blocking(
    screen: &mut HudLayer,
    view: &HudEditorView,
    input: UiInput,
    blocked: &[Rect],
) -> Option<HudEditorOutcome> {
    let font = FontAtlas::fallback();
    let zones = HudZones {
        snap: &snap,
        blocked,
        grid: None,
    };
    frame(input, |ui| screen.show(ui, (&font, &font), view, zones))
}

fn pressed(at: [f32; 2]) -> InputEvent {
    InputEvent::PointerPressed {
        position: px(at),
        button: PointerButton::Left,
    }
}

fn moved(to: [f32; 2]) -> InputEvent {
    InputEvent::PointerMoved { position: px(to) }
}

fn released(at: [f32; 2]) -> InputEvent {
    InputEvent::PointerReleased {
        position: px(at),
        button: PointerButton::Left,
    }
}

fn middle(rect: Rect) -> [f32; 2] {
    [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5]
}

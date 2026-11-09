// SPDX-License-Identifier: MIT OR Apache-2.0

//! The HUD layer of the mods menu (ADR-0065): the widgets of the enabled HUD
//! mods drawn over the dark sheet of the menu, as the HUD draws them, and
//! movable with the mouse wherever the pointer is not on a button or on the
//! open panel.
//!
//! A widget is dragged (snapped to the screen edges and centre, with guides),
//! resized with the wheel or by its corner handle, and a small bar on the
//! landing page resets them. The layer keeps only what the player is doing (the
//! selection and the drag under way); every change leaves as a
//! [`HudEditorOutcome`] for the client to apply, and the placement maths is in
//! the mods crate, handed in as the [`Snap`] function. The sheet and the blur
//! are the menu's own.

mod drag;
mod frame;
mod gear;
mod guides;
mod paint;
mod shield;
#[cfg(test)]
mod tests;
mod toolbar;
mod view;

pub use view::{HudEditorOutcome, HudEditorView, HudWidgetView, Snap};

/// The gear of the widget whose text covers `rect`, for the tests of the menu.
#[cfg(test)]
pub(super) fn gear_rect_for_tests(rect: crate::geom::Rect) -> crate::geom::Rect {
    gear::gear_rect(rect, [427.0, 240.0])
}

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::geom::Rect;
use crate::input::{InputEvent, PointerButton};
use crate::menus::common::Gui;
use crate::mod_widget::{measure, ModWidgetView};
use crate::text::FontAtlas;

use drag::{hit_widget, on_handle, wheel_resize, Bounds, Drag, Grab};
use frame::frame_rect;
use gear::gear_rect;
use shield::Shield;
use toolbar::ToolbarAction;

/// Where the layer must leave the pointer to the rest of the menu.
#[derive(Clone, Copy)]
pub struct HudZones<'a> {
    /// The snapping applied while a widget moves.
    pub snap: Snap<'a>,
    /// Rectangles (physical pixels) a press must not grab a widget through:
    /// the buttons of the page and its panel.
    pub blocked: &'a [Rect],
    /// Whether the reset bar is shown; it blocks the pointer too.
    pub bar: bool,
}

/// The HUD layer state. The menu makes a new one each time it opens.
#[derive(Debug, Clone, Default)]
pub struct HudLayer {
    /// The id of the mod whose widget the bar acts on.
    selected: Option<String>,
    drag: Option<Drag>,
    /// The drag that ended in this frame, drawn once more where it was dropped
    /// because the client has not applied the new placement yet.
    dropped: Option<Drag>,
}

impl HudLayer {
    /// Draws one frame of the layer over `view` and returns what the player
    /// did. `font` is the font of the HUD (the widgets are drawn with it),
    /// `brand` the font of the menus.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        (font, brand): (&FontAtlas, &FontAtlas),
        lang: &Lang,
        view: &HudEditorView,
        zones: HudZones<'_>,
    ) -> Option<HudEditorOutcome> {
        let viewport = ui.viewport();
        let gui = Gui::new(viewport);
        let bounds = Bounds {
            screen: view.screen,
            scale_range: view.scale_range,
            snap: zones.snap,
        };
        let bar = toolbar::layout(viewport);
        let shield = Shield::new(zones.blocked, zones.bar.then_some(bar.bar));
        let rects: Vec<Rect> = view
            .widgets
            .iter()
            .map(|w| widget_rect(font, &w.widget))
            .collect();
        self.dropped = None;
        let placed = self.read_input(ui, &gui, (view, &rects), (&shield, &bounds));

        self.draw_widgets(ui, &gui, font, view);
        self.draw_overlay(ui, &gui, (view, &rects), &shield);
        self.draw_gears(ui, brand, &gui, (view, &rects), &shield);
        if !zones.bar {
            return placed;
        }
        let selected = self.selected_index(view);
        self.draw_readout(ui, &gui, brand, view);
        let action = toolbar::show(
            ui,
            brand,
            lang,
            &bar,
            (selected.is_some(), !view.widgets.is_empty()),
        );
        placed.or_else(|| Self::toolbar_outcome(action, selected.map(|i| &view.widgets[i])))
    }

    /// Applies this frame's input events; returns the placement a drop or a
    /// wheel step produced.
    fn read_input(
        &mut self,
        ui: &Ui,
        gui: &Gui,
        (view, rects): (&HudEditorView, &[Rect]),
        (shield, bounds): (&Shield<'_>, &Bounds<'_>),
    ) -> Option<HudEditorOutcome> {
        let mut placed = None;
        let mut notches = 0.0;
        for event in &ui.input().events {
            match event {
                InputEvent::PointerMoved { position } => {
                    if let Some(drag) = self.drag.as_mut() {
                        drag.follow(gui.to_gui(*position), bounds);
                    }
                }
                InputEvent::PointerPressed {
                    position,
                    button: PointerButton::Left,
                } if !shield.covers(*position) => {
                    let point = gui.to_gui(*position);
                    if let Some(id) = self.gear_at(point, (view, rects)) {
                        placed = Some(HudEditorOutcome::OpenSettings { id });
                    } else {
                        self.press(point, (view, rects));
                    }
                }
                InputEvent::PointerReleased {
                    button: PointerButton::Left,
                    ..
                } => placed = self.release(view.screen).or(placed),
                InputEvent::Wheel { delta } => notches += delta,
                _ => {}
            }
        }
        if placed.is_none() && notches != 0.0 && self.drag.is_none() {
            placed = self.wheel(ui, gui, notches, (view, rects), (shield, bounds));
        }
        placed
    }

    /// The widgets that show a gear with the pointer at `point`: the one the
    /// pointer is over and the selected one. Nothing while a
    /// widget is dragged.
    pub(super) fn gear_targets(
        &self,
        point: Option<[f32; 2]>,
        (view, rects): (&HudEditorView, &[Rect]),
    ) -> Vec<usize> {
        let mut targets = Vec::new();
        if self.drag.is_some() {
            return targets;
        }
        let under = point.and_then(|p| hit_widget(rects, p, view.screen));
        targets.extend(under);
        if let Some(selected) = self.selected_index(view) {
            if !targets.contains(&selected) {
                targets.push(selected);
            }
        }
        targets
    }

    /// The id of the mod whose gear is at `point`, if one is shown there.
    fn gear_at(&self, point: [f32; 2], (view, rects): (&HudEditorView, &[Rect])) -> Option<String> {
        self.gear_targets(Some(point), (view, rects))
            .into_iter()
            .find(|&index| gear_rect(rects[index], view.screen).contains(point))
            .map(|index| view.widgets[index].id.clone())
    }

    /// A press at `point`: selects the widget under it and starts dragging it
    /// by its body or, on the corner handle, resizing it.
    fn press(&mut self, point: [f32; 2], (view, rects): (&HudEditorView, &[Rect])) {
        let Some(index) = hit_widget(rects, point, view.screen) else {
            self.selected = None;
            return;
        };
        let (rect, widget) = (rects[index], &view.widgets[index]);
        let grab = if on_handle(frame_rect(rect, view.screen), point) {
            Grab::Resize(point[0] - rect.right())
        } else {
            Grab::Move([point[0] - rect.x, point[1] - rect.y])
        };
        self.selected = Some(widget.id.clone());
        self.drag = Some(Drag::begin(
            widget.id.clone(),
            grab,
            rect,
            widget.widget.scale,
        ));
    }

    /// The end of a drag: the placement, when the widget went anywhere.
    fn release(&mut self, screen: [f32; 2]) -> Option<HudEditorOutcome> {
        let drag = self.drag.take()?;
        let outcome = drag.changed().then(|| HudEditorOutcome::Placed {
            id: drag.id.clone(),
            rect: drag.live,
            scale: drag.scale,
            screen,
        });
        self.dropped = Some(drag);
        outcome
    }

    /// The wheel over a widget changes its scale by a step per notch.
    fn wheel(
        &mut self,
        ui: &Ui,
        gui: &Gui,
        notches: f32,
        (view, rects): (&HudEditorView, &[Rect]),
        (shield, bounds): (&Shield<'_>, &Bounds<'_>),
    ) -> Option<HudEditorOutcome> {
        let pointer = ui.pointer().filter(|p| !shield.covers(*p))?;
        let point = gui.to_gui(pointer);
        let index = hit_widget(rects, point, view.screen)?;
        let widget = &view.widgets[index];
        self.selected = Some(widget.id.clone());
        let (rect, scale) = wheel_resize((rects[index], widget.widget.scale), notches, bounds)?;
        Some(HudEditorOutcome::Placed {
            id: widget.id.clone(),
            rect,
            scale,
            screen: view.screen,
        })
    }

    /// The outcome of a bar button: the reset of the selected widget, or the
    /// reset of all.
    fn toolbar_outcome(
        action: Option<ToolbarAction>,
        selected: Option<&HudWidgetView>,
    ) -> Option<HudEditorOutcome> {
        match action? {
            ToolbarAction::Reset => selected.map(|w| HudEditorOutcome::Reset { id: w.id.clone() }),
            ToolbarAction::ResetAll => Some(HudEditorOutcome::ResetAll),
        }
    }
}

/// The rectangle a widget covers, in GUI units.
fn widget_rect(font: &FontAtlas, widget: &ModWidgetView) -> Rect {
    let (width, height) = measure(font, &widget.lines, widget.scale, &widget.style);
    Rect::new(widget.position[0], widget.position[1], width, height)
}

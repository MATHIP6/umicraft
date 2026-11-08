// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the HUD layer draws each frame: the previews, the selection frames,
//! the guides and the readout of the selected widget.

use std::slice;

use crate::context::Ui;
use crate::geom::Rect;
use crate::menus::common::Gui;
use crate::mod_widget::draw_mod_widgets;
use crate::text::FontAtlas;

use super::drag::{hit_widget, Drag};
use super::frame::{draw_frame, draw_label, FrameKind};
use super::guides::draw_guides;
use super::shield::Shield;
use super::toolbar;
use super::{HudEditorView, HudLayer};

impl HudLayer {
    /// The widget of `drag`, where the drag has put it.
    pub(super) fn dragged(&self) -> Option<&Drag> {
        self.drag.as_ref().or(self.dropped.as_ref())
    }

    /// The index in `view` of the selected widget, if it is still there.
    pub(super) fn selected_index(&self, view: &HudEditorView) -> Option<usize> {
        let id = self.selected.as_deref()?;
        view.widgets.iter().position(|w| w.id == id)
    }

    /// Draws the previews, the dragged one where the pointer holds it.
    pub(super) fn draw_widgets(
        &self,
        ui: &mut Ui,
        gui: &Gui,
        font: &FontAtlas,
        view: &HudEditorView,
    ) {
        for widget in &view.widgets {
            let mut shown = widget.widget.clone();
            if let Some(drag) = self.dragged().filter(|d| d.id == widget.id) {
                shown.position = [drag.live.x, drag.live.y];
                shown.scale = drag.scale;
            }
            draw_mod_widgets(ui.draw(), slice::from_ref(&shown), gui.scale, font);
        }
    }

    /// Draws the frames and the guides.
    pub(super) fn draw_overlay(
        &self,
        ui: &mut Ui,
        gui: &Gui,
        (view, rects): (&HudEditorView, &[Rect]),
        shield: &Shield<'_>,
    ) {
        let hovered = match self.drag {
            Some(_) => None,
            None => ui
                .pointer()
                .filter(|p| !shield.covers(*p))
                .and_then(|p| hit_widget(rects, gui.to_gui(p))),
        };
        for (index, widget) in view.widgets.iter().enumerate() {
            let selected = self.selected.as_deref() == Some(widget.id.as_str());
            let kind = if selected {
                FrameKind::Selected
            } else if hovered == Some(index) {
                FrameKind::Hovered
            } else {
                continue;
            };
            let rect = self
                .dragged()
                .filter(|d| d.id == widget.id)
                .map_or(rects[index], |d| d.live);
            draw_frame(ui, gui, rect, kind);
        }
        if let Some(drag) = &self.drag {
            draw_guides(ui, gui, &drag.guides, view.screen);
        }
    }

    /// Draws the name and scale of the selected widget above the bar.
    pub(super) fn draw_readout(
        &self,
        ui: &mut Ui,
        gui: &Gui,
        brand: &FontAtlas,
        view: &HudEditorView,
    ) {
        if let Some(index) = self.selected_index(view) {
            let widget = &view.widgets[index];
            let scale = self.dragged().map_or(widget.widget.scale, |d| d.scale);
            let readout = format!("{} - x{scale:.1}", widget.name);
            let above_bar = gui.rect(gui.cx(), toolbar::top(gui) - 9.0, 0.0, 0.0);
            draw_label(ui, brand, gui, &readout, [above_bar.x, above_bar.y]);
        }
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The HUD editor: a page of the mods menu that leaves the screen to the game
//! and to the HUD widgets, with one panel docked on a side.
//!
//! The widgets are the ones of the [HUD layer](super::hud_layer): dragged,
//! snapped and resized by the same code as on the other pages. The panel (the
//! "dock") lists every mod that has a widget, edits the selected one (place,
//! size, look, behaviour) and holds the actions: undo, redo, the grid, the
//! presets, reset, cancel and save. It moves to the side opposite the selected
//! widget, so it never covers what the player is working on.
//!
//! Like the rest of the menu the page keeps only what the player is doing;
//! every change leaves as a [`ModsOutcome`] for the client to apply.

mod dock;
mod inspect;

pub(super) use dock::chip;
#[cfg(test)]
pub(super) use dock::{action_row, list_row, row_toggle, split};

use std::borrow::Cow;

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::geom::Rect;
use crate::menus::common::{escape_pressed, Gui};
use crate::modern::theme;
use crate::modern::{Align, ModernButton, SliderDrag};
use crate::text::FontAtlas;

use super::hud_layer::{widget_rect, HudEditorOutcome, HudZones, Snap};
use super::style::{hover_level, Paint, PANEL, PANEL_RIM};
use super::view::{ModsOutcome, ModsView};
use super::{ModsScreen, Page};

/// The step of the magnetic grid, in GUI units.
pub(super) const GRID_STEP: f32 = 8.0;
/// Width of the dock, in GUI units.
const DOCK_WIDTH: f32 = 140.0;
/// Space between the dock and the screen edge.
const MARGIN: f32 = 6.0;
/// Space between the dock edge and its content.
pub(super) const PAD: f32 = 6.0;
/// Ticks the page takes to come in.
const ENTER_TICKS: u8 = 5;
/// How dark the sheet over the game is while "dim" is on.
const DIM: f32 = 0.22;
/// The lines of the grid.
const GRID_LINE: [f32; 4] = [1.0, 1.0, 1.0, 0.06];

/// The tab that lists the widgets, and the one that edits the selected one.
pub(super) const TAB_WIDGETS: usize = 0;
pub(super) const TAB_SETTINGS: usize = 1;

/// What the player is doing on the HUD editor.
#[derive(Debug, Clone)]
pub(super) struct EditorState {
    /// [`TAB_WIDGETS`] or [`TAB_SETTINGS`].
    pub tab: usize,
    /// The search of the widget list.
    pub query: String,
    /// Whether a moved widget sticks to the grid.
    pub grid: bool,
    /// Whether the game behind is darkened a little.
    pub dim: bool,
    /// The ids of the widgets hidden while editing; nothing is saved of it.
    pub hidden: Vec<String>,
    /// Whether the "reset everything?" question is up.
    pub confirm: bool,
    /// Scroll offset of the settings tab, in GUI units.
    pub scroll: f32,
    /// Whether the dock is on the left side.
    pub left: bool,
    /// Ticks since the page opened, up to [`ENTER_TICKS`].
    pub age: u8,
    /// The slider being dragged in the settings tab.
    pub drag: SliderDrag,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            tab: TAB_WIDGETS,
            query: String::new(),
            grid: false,
            dim: true,
            hidden: Vec::new(),
            confirm: false,
            scroll: 0.0,
            left: false,
            age: 0,
            drag: SliderDrag::default(),
        }
    }
}

/// Where the parts of the dock sit, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct DockLayout {
    /// The whole panel.
    pub dock: Rect,
    /// The title band.
    pub header: Rect,
    /// The cross that goes back to the list of mods.
    pub close: Rect,
    /// The two tabs.
    pub tabs: Rect,
    /// The area of the tab shown.
    pub body: Rect,
    /// The area of the actions, at the bottom.
    pub footer: Rect,
    /// Physical pixels per GUI unit.
    pub scale: f32,
}

/// Height of the title band, the tab row and one row of actions.
const HEADER: f32 = 10.0;
const TABS: f32 = 10.0;
pub(super) const ACTION: f32 = 12.0;
/// Space between two parts, and between two actions.
pub(super) const GAP: f32 = 3.0;
/// Rows of actions in the footer.
const ACTION_ROWS: f32 = 4.0;

/// The dock of a screen of `gui`, its left edge at `x` GUI units.
pub(super) fn dock_layout(gui: &Gui, x: f32) -> DockLayout {
    let (top, height) = (MARGIN, gui.height - 2.0 * MARGIN);
    let inner = DOCK_WIDTH - 2.0 * PAD;
    let left = x + PAD;
    let header_y = top + PAD;
    let tabs_y = header_y + HEADER + GAP;
    let body_y = tabs_y + TABS + GAP;
    let footer_height = ACTION_ROWS * ACTION + (ACTION_ROWS - 1.0) * GAP;
    let footer_y = top + height - PAD - footer_height;
    DockLayout {
        dock: gui.rect(x, top, DOCK_WIDTH, height),
        header: gui.rect(left, header_y, inner, HEADER),
        close: gui.rect(left + inner - HEADER, header_y, HEADER, HEADER),
        tabs: gui.rect(left, tabs_y, inner, TABS),
        body: gui.rect(left, body_y, inner, (footer_y - GAP - body_y).max(0.0)),
        footer: gui.rect(left, footer_y, inner, footer_height),
        scale: gui.scale,
    }
}

/// The left edge of the dock on a screen `width` GUI units wide.
pub(super) fn dock_x(width: f32, left: bool) -> f32 {
    if left {
        MARGIN
    } else {
        width - MARGIN - DOCK_WIDTH
    }
}

impl ModsScreen {
    /// Opens the HUD editor, its page coming in.
    pub(super) fn open_editor(&mut self) {
        self.page = Page::HudEditor;
        self.editor.age = 0;
        self.editor.confirm = false;
        self.motion.settle("dock_x");
    }

    /// Opens the HUD editor with its entrance over, on the settings of the
    /// widget of `selected` or, without one, on the list of widgets: what a
    /// capture of a single frame needs.
    pub fn open_hud_editor(&mut self, selected: Option<&str>) {
        self.open_editor();
        self.entered = Page::HudEditor;
        self.editor.age = ENTER_TICKS;
        self.hud.select(selected.map(str::to_owned));
        if selected.is_some() {
            self.show_settings_tab();
        }
    }

    /// Leaves the HUD editor for the list of mods.
    pub(super) fn close_editor(&mut self) {
        self.page = Page::List;
        self.editor.confirm = false;
        self.editor.drag = SliderDrag::default();
    }

    /// How far the HUD editor has come in, `0.0` to `1.0`; `0.0` on the other
    /// pages.
    pub(super) fn editor_level(&self, partial_tick: f32) -> f32 {
        if self.page != Page::HudEditor {
            return 0.0;
        }
        let partial = if self.editor.age < ENTER_TICKS {
            partial_tick.clamp(0.0, 1.0)
        } else {
            0.0
        };
        ((f32::from(self.editor.age) + partial) / f32::from(ENTER_TICKS)).clamp(0.0, 1.0)
    }

    /// Advances the entrance of the HUD editor by one tick.
    pub(super) fn tick_editor(&mut self) {
        if self.page == Page::HudEditor {
            self.editor.age = (self.editor.age + 1).min(ENTER_TICKS);
        }
    }

    /// One frame of the HUD editor.
    pub(super) fn editor_page(
        &mut self,
        ui: &mut Ui,
        fonts: (&FontAtlas, &FontAtlas),
        lang: &Lang,
        view: &ModsView,
        (snap, partial_tick): (Snap<'_>, f32),
    ) -> Option<ModsOutcome> {
        let viewport = ui.viewport();
        let gui = Gui::new(viewport);
        let (font, brand) = fonts;
        let paint = Paint {
            brand,
            scale: gui.scale,
        };
        let level = self.editor_level(partial_tick);
        // Read before anything else, so one press closes one thing only.
        let escape = escape_pressed(ui);

        if self.editor.dim {
            ui.draw().rect(viewport, [0.0, 0.0, 0.0, DIM * level]);
        }
        if self.editor.grid {
            draw_grid(ui, &gui);
        }

        // The widgets hidden while editing leave the layer; the copy is only
        // made while there is one.
        let shown: Cow<'_, _> = if self.editor.hidden.is_empty() {
            Cow::Borrowed(&view.hud)
        } else {
            let mut hud = view.hud.clone();
            hud.widgets.retain(|w| !self.editor.hidden.contains(&w.id));
            Cow::Owned(hud)
        };

        // The dock goes to the side the selected widget is not on, and waits
        // for the end of a drag to move.
        if !self.hud.is_dragging() {
            let selected = self
                .hud
                .selected()
                .and_then(|id| shown.widgets.iter().find(|w| w.id == id))
                .map(|w| widget_rect(font, &w.widget));
            if let Some(rect) = selected {
                self.editor.left = rect.x + rect.width * 0.5 > shown.screen[0] * 0.5;
            } else {
                // Nothing selected: the side that covers fewer widgets, and
                // the one it is on when both cover as many.
                let covered = |left: bool| {
                    let x = dock_x(gui.width, left);
                    shown
                        .widgets
                        .iter()
                        .map(|w| widget_rect(font, &w.widget))
                        .filter(|rect| rect.right() > x && rect.x < x + DOCK_WIDTH)
                        .count()
                };
                let (on_left, on_right) = (covered(true), covered(false));
                if on_left != on_right {
                    self.editor.left = on_left < on_right;
                }
            }
        }
        let target = dock_x(gui.width, self.editor.left);
        let layout = dock_layout(&gui, self.motion.glide("dock_x", target, false));

        let everything = [viewport];
        let dock_only = [layout.dock];
        let zones = HudZones {
            snap,
            blocked: if self.editor.confirm {
                &everything
            } else {
                &dock_only
            },
            bar: false,
            grid: self.editor.grid.then_some(GRID_STEP),
        };
        let before = self.hud.selected().map(str::to_owned);
        let mut placed = self.hud.show(ui, fonts, lang, &shown, zones);
        // The gear of a widget, or a click on another widget, opens its
        // settings in the dock.
        if let Some(HudEditorOutcome::OpenSettings { id }) = &placed {
            self.hud.select(Some(id.clone()));
            placed = None;
            self.show_settings_tab();
        } else if self.hud.selected().is_some() && self.hud.selected() != before.as_deref() {
            self.show_settings_tab();
        }

        ui.push_id("hud_editor");
        ui.draw().set_opacity(level);
        let acted = if self.editor.confirm {
            self.confirm_dialog(ui, paint, lang, &gui)
        } else {
            self.dock(ui, (paint, font), lang, view, &layout)
        };
        ui.draw().set_opacity(1.0);
        ui.pop_id();

        if escape {
            if self.editor.confirm {
                self.editor.confirm = false;
            } else {
                self.close_editor();
            }
        }
        acted.or(placed.map(ModsOutcome::Hud))
    }

    /// Shows the settings of the selected widget, from their top.
    fn show_settings_tab(&mut self) {
        self.editor.tab = TAB_SETTINGS;
        self.editor.scroll = 0.0;
    }

    /// The question asked before every widget is reset: a small panel in the
    /// middle of the screen, the rest of the page waiting behind it.
    fn confirm_dialog(
        &mut self,
        ui: &mut Ui,
        paint: Paint<'_>,
        lang: &Lang,
        gui: &Gui,
    ) -> Option<ModsOutcome> {
        const SIZE: (f32, f32) = (176.0, 46.0);
        const BUTTON: (f32, f32) = (70.0, 14.0);
        let left = ((gui.width - SIZE.0) * 0.5).floor();
        let top = ((gui.height - SIZE.1) * 0.5).floor();
        let panel = gui.rect(left, top, SIZE.0, SIZE.1);
        draw_panel(ui, panel, paint.scale);
        let question = lang.tr("umicraft.mods.editor.confirm");
        let at = gui.rect(left + SIZE.0 * 0.5, top + 13.0, 0.0, 0.0);
        let style = (theme::TEXT_SIZE_BODY, theme::TEXT, Align::Center);
        paint.text(ui, question, (at.x, at.y), style);

        let gap = 6.0;
        let first = left + (SIZE.0 - 2.0 * BUTTON.0 - gap) * 0.5;
        let y = top + SIZE.1 - BUTTON.1 - 8.0;
        let mut answer = None;
        for (index, key) in ["umicraft.mods.editor.cancel", "umicraft.mods.editor.reset_all"]
            .into_iter()
            .enumerate()
        {
            let x = first + index as f32 * (BUTTON.0 + gap);
            let rect = gui.rect(x, y, BUTTON.0, BUTTON.1);
            let id = ui.id(key);
            let fade = hover_level(ui, rect);
            if ModernButton::new(id, rect, lang.tr(key))
                .show(ui, paint.brand, paint.scale, fade)
                .clicked
            {
                answer = Some(index == 1);
            }
        }
        let reset = answer?;
        self.editor.confirm = false;
        reset.then_some(ModsOutcome::Hud(HudEditorOutcome::ResetAll))
    }
}

/// Draws the body of a panel: a faint rim and the dark fill of the mods menu.
pub(super) fn draw_panel(ui: &mut Ui, rect: Rect, scale: f32) {
    let radius = theme::RADIUS * scale * 1.5;
    let rim = scale.max(1.0);
    ui.draw().rounded_rect(rect, radius, PANEL_RIM);
    let inner = Rect::new(
        rect.x + rim,
        rect.y + rim,
        rect.width - 2.0 * rim,
        rect.height - 2.0 * rim,
    );
    ui.draw().rounded_rect(inner, radius - rim, PANEL);
}

/// Draws the lines of the magnetic grid, one physical pixel thick.
fn draw_grid(ui: &mut Ui, gui: &Gui) {
    let thickness = 1.0 / gui.scale;
    let mut x = GRID_STEP;
    while x < gui.width {
        let line = gui.rect(x, 0.0, thickness, gui.height);
        ui.draw().rect(line, GRID_LINE);
        x += GRID_STEP;
    }
    let mut y = GRID_STEP;
    while y < gui.height {
        let line = gui.rect(0.0, y, gui.width, thickness);
        ui.draw().rect(line, GRID_LINE);
        y += GRID_STEP;
    }
}

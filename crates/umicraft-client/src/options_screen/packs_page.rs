// SPDX-License-Identifier: MIT OR Apache-2.0

//! The resource packs page (`GuiScreenResourcePacks`).
//!
//! Two lists, the available packs on the left and the selected ones on the
//! right (highest priority on top, the built-in "Default" last). A click on
//! an available pack selects it on top; a click on a selected pack gives it
//! back; the small arrows on a selected pack change its priority. The
//! choice is a draft until "Done": only then the options change and
//! [`Outcome::packs_changed`] tells the window to reload the resources, as
//! vanilla does in `actionPerformed(1)`.
//!
//! Not drawn: the `pack.png` icons (the slot is left empty). A pack whose
//! `pack_format` is not 1 is tinted red like vanilla but selects without the
//! confirmation screen. See `docs/parity.md`.

use umicraft_assets::pack_discovery::DiscoveredPack;
use umicraft_core::lang::Lang;
use umicraft_ui::{Color, FontAtlas, InputEvent, Rect, Ui};

use super::layout::{fit_text, strip_formatting, Gui, BUTTON_H, GREY, HALF_W, LIST_BAR};
use super::{OptionsScreen, Outcome, Page};
use crate::options::Options;

/// Width of one list.
const LIST_W: f32 = 200.0;
/// Height of one entry (`GuiResourcePackList`).
const ROW: f32 = 36.0;
/// Height of the list header (`FONT_HEIGHT * 1.5`).
const HEADER: f32 = 13.0;
/// Size of the icon slot of an entry.
const ICON: f32 = 32.0;
/// Side of a priority arrow button.
const ARROW: f32 = 11.0;
/// The background of an incompatible pack (`0xFF770000`).
/// White text.
const WHITE: Color = [1.0; 4];
const INCOMPATIBLE: Color = [0.467, 0.0, 0.0, 1.0];

/// Which list a click came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    Available,
    Selected,
}

/// What the player did to an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Action {
    /// Moved it to the other list.
    Move(String),
    /// Gave it a higher priority.
    Up(String),
    /// Gave it a lower priority.
    Down(String),
}

/// One entry to draw.
struct Row {
    /// The pack file name; `None` for the built-in "Default".
    pack: Option<String>,
    title: String,
    description: String,
    compatible: bool,
}

impl Row {
    fn of(pack: &DiscoveredPack, lang: &Lang) -> Self {
        let (description, compatible) = match &pack.metadata {
            Some(metadata) => (
                strip_formatting(&metadata.description),
                metadata.pack_format == 1,
            ),
            None => (
                lang.tr("umicraft.resourcePack.missingMetadata").to_string(),
                true,
            ),
        };
        Self {
            pack: Some(pack.name.clone()),
            title: pack.name.clone(),
            description,
            compatible,
        }
    }

    fn default_pack(lang: &Lang) -> Self {
        Self {
            pack: None,
            title: lang.tr("umicraft.resourcePack.default").to_string(),
            description: lang
                .tr("umicraft.resourcePack.defaultDescription")
                .to_string(),
            compatible: true,
        }
    }
}

impl OptionsScreen {
    /// Starts a draft from the enabled packs that still exist in the folder,
    /// highest priority first.
    pub(super) fn begin_pack_draft(&mut self, options: &Options) {
        self.pack_draft = options
            .resource_packs
            .enabled
            .iter()
            .rev()
            .filter(|name| self.catalog.packs().iter().any(|pack| &pack.name == *name))
            .cloned()
            .collect();
        self.pack_scroll = [0.0; 2];
    }

    /// The names of the packs the draft selects, highest priority first.
    #[must_use]
    pub fn pack_draft(&self) -> &[String] {
        &self.pack_draft
    }

    /// `GuiScreenResourcePacks`.
    pub(super) fn show_packs(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        gui: Gui,
        lang: &Lang,
        outcome: &mut Outcome,
    ) {
        let (top, bottom) = (32.0, gui.height - 51.0);
        let cx = gui.cx();
        let available: Vec<Row> = self
            .catalog
            .packs()
            .iter()
            .filter(|pack| !self.pack_draft.contains(&pack.name))
            .map(|pack| Row::of(pack, lang))
            .collect();
        let selected: Vec<Row> = self
            .pack_draft
            .iter()
            .filter_map(|name| self.catalog.packs().iter().find(|pack| &pack.name == name))
            .map(|pack| Row::of(pack, lang))
            .chain(std::iter::once(Row::default_pack(lang)))
            .collect();

        self.scroll_columns(ui, gui, cx, [available.len(), selected.len()], bottom - top);
        let left = cx - 4.0 - LIST_W;
        let right = cx + 4.0;
        let mut action = self.pack_column(
            ui,
            atlas,
            gui,
            (Column::Available, left, (top, bottom)),
            &available,
            lang,
        );
        let selected_action = self.pack_column(
            ui,
            atlas,
            gui,
            (Column::Selected, right, (top, bottom)),
            &selected,
            lang,
        );
        action = action.or(selected_action);
        if let Some(action) = action {
            self.apply_pack_action(action);
        }

        Self::title(ui, atlas, gui, lang.tr("resourcePack.title"), 16.0);
        let open = gui.rect(cx - 154.0, gui.height - 48.0, HALF_W, BUTTON_H);
        if self.button(
            ui,
            atlas,
            "open_folder",
            open,
            lang.tr("resourcePack.openFolder"),
            true,
            None,
        ) {
            outcome.open_packs_folder = true;
        }
        let done = gui.rect(cx + 4.0, gui.height - 48.0, HALF_W, BUTTON_H);
        if self.button(ui, atlas, "done", done, lang.tr("gui.done"), true, None) {
            self.finish_packs(options, outcome);
        }
    }

    /// Applies the draft to the options when it differs, and leaves the page.
    fn finish_packs(&mut self, options: &mut Options, outcome: &mut Outcome) {
        let chosen: Vec<String> = self.pack_draft.iter().rev().cloned().collect();
        if chosen != options.resource_packs.enabled {
            options.resource_packs.enabled = chosen;
            outcome.changed = true;
            outcome.packs_changed = true;
        }
        self.page = Page::Main;
        self.scroll = 0.0;
    }

    /// Moves, raises or lowers a pack of the draft.
    fn apply_pack_action(&mut self, action: Action) {
        let draft = &mut self.pack_draft;
        match action {
            Action::Move(name) => {
                if let Some(index) = draft.iter().position(|item| *item == name) {
                    draft.remove(index);
                } else {
                    draft.insert(0, name);
                }
            }
            Action::Up(name) => {
                if let Some(index) = draft.iter().position(|item| *item == name) {
                    if index > 0 {
                        draft.swap(index, index - 1);
                    }
                }
            }
            Action::Down(name) => {
                if let Some(index) = draft.iter().position(|item| *item == name) {
                    if index + 1 < draft.len() {
                        draft.swap(index, index + 1);
                    }
                }
            }
        }
    }

    /// Applies the wheel to the list under the pointer.
    fn scroll_columns(&mut self, ui: &Ui, gui: Gui, cx: f32, rows: [usize; 2], visible: f32) {
        let wheel: f32 = ui
            .input()
            .events
            .iter()
            .filter_map(|event| match event {
                InputEvent::Wheel { delta } => Some(*delta),
                _ => None,
            })
            .sum();
        let over_right = ui
            .pointer()
            .is_some_and(|position| position[0] >= cx * gui.scale);
        for (column, count) in rows.into_iter().enumerate() {
            let content = HEADER + 4.0 + count as f32 * ROW;
            let max = (content - (visible - 4.0)).max(0.0);
            let scroll = &mut self.pack_scroll[column];
            if (column == 1) == over_right {
                *scroll += wheel * ROW / 2.0;
            }
            *scroll = scroll.clamp(0.0, max);
        }
    }

    /// Draws one list and reports what was clicked in it.
    fn pack_column(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        gui: Gui,
        (column, x, (top, bottom)): (Column, f32, (f32, f32)),
        rows: &[Row],
        lang: &Lang,
    ) -> Option<Action> {
        let slot = usize::from(column == Column::Selected);
        let scroll = self.pack_scroll[slot];
        let header = lang.tr(match column {
            Column::Available => "resourcePack.available.title",
            Column::Selected => "resourcePack.selected.title",
        });
        let mut action = None;
        Self::list_container(ui, gui, (x, LIST_W), (top, bottom), scroll);
        ui.push_clip(gui.rect(x, top, LIST_W, bottom - top));
        for (index, row) in rows.iter().enumerate() {
            let y = top + 4.0 + HEADER - scroll + index as f32 * ROW;
            if y + ROW < top || y > bottom {
                continue;
            }
            let tag = format!("{column:?}:{index}");
            action = action.or(self.pack_row(ui, atlas, gui, (x, y), row, &tag));
            if column == Column::Selected {
                action = action.or(self.arrows(ui, atlas, gui, (x, y), row, &tag));
            }
        }
        ui.pop_clip();
        Self::list_overlay(ui, gui, (x, LIST_W), (top, bottom));
        Self::list_header(ui, atlas, gui, (x, top), header);
        Self::scrollbar(
            ui,
            gui,
            x + LIST_W - 6.0,
            (top, bottom),
            HEADER + 4.0 + rows.len() as f32 * ROW,
            scroll,
        );
        action
    }

    /// The header of a list, drawn over the clipped rows.
    fn list_header(ui: &mut Ui, atlas: &FontAtlas, gui: Gui, (x, top): (f32, f32), text: &str) {
        let at = (x + LIST_W / 2.0, top + 3.0);
        Self::centered_text(ui, atlas, gui, at, text, [1.0; 4]);
    }

    /// One entry: tinted when incompatible, the icon slot, the name and
    /// the description. Returns a move action when the entry was clicked.
    fn pack_row(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        gui: Gui,
        (x, y): (f32, f32),
        row: &Row,
        tag: &str,
    ) -> Option<Action> {
        if !row.compatible {
            ui.draw().rect(
                gui.rect(x - 1.0, y - 1.0, LIST_W - 8.0, ROW + 2.0),
                INCOMPATIBLE,
            );
        }
        ui.draw().rect(gui.rect(x, y, ICON, ICON), LIST_BAR);
        let text_width = LIST_W - ICON - 10.0 - ARROW * 2.0;
        let title = fit_text(atlas, &row.title, text_width);
        Self::text_at(ui, gui, (x + ICON + 3.0, y + 1.0), &title, [1.0; 4]);
        let description = fit_text(atlas, &row.description, text_width);
        Self::text_at(ui, gui, (x + ICON + 3.0, y + 12.0), &description, GREY);
        let pack = row.pack.clone()?;
        let id = format!("pack:{tag}");
        let rect = gui.rect(x, y, LIST_W - 8.0 - ARROW * 2.0, ROW);
        self.rects.insert(id.clone(), rect);
        let widget = ui.id(&id);
        ui.response(widget, rect)
            .clicked
            .then_some(Action::Move(pack))
    }

    /// The up and down arrows of a selected pack.
    fn arrows(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        gui: Gui,
        (x, y): (f32, f32),
        row: &Row,
        tag: &str,
    ) -> Option<Action> {
        let pack = row.pack.clone()?;
        let mut action = None;
        for (symbol, offset, id, make) in [
            ("^", 0.0, "up", Action::Up as fn(String) -> Action),
            ("v", ARROW, "down", Action::Down as fn(String) -> Action),
        ] {
            let at = (x + LIST_W - 8.0 - ARROW * 2.0 + offset, y + 2.0);
            let rect: Rect = gui.rect(at.0, at.1, ARROW, ARROW);
            ui.draw().rect(rect, LIST_BAR);
            Self::centered_text(
                ui,
                atlas,
                gui,
                (at.0 + ARROW / 2.0, at.1 + 2.0),
                symbol,
                WHITE,
            );
            let id = format!("pack:{id}:{tag}");
            self.rects.insert(id.clone(), rect);
            let widget = ui.id(&id);
            if ui.response(widget, rect).clicked {
                action = Some(make(pack.clone()));
            }
        }
        action
    }
}

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
//! A pack whose `pack_format` is not 1 is tinted red and asks "made for an
//! older/newer version" before it is enabled (`GuiYesNo`).
//!
//! An entry is drawn as `ResourcePackListEntry.drawEntry` does: the icon
//! (`pack.png`, else `unknown_pack.png`; the built-in "Default" has the
//! `pack.png` of the jar), the name cut at 157 units with "...", and the
//! description wrapped at 157 units into two lines, with its `§` colours. The
//! `pack.png` images go through the GUI atlas like the server favicons: the
//! window asks for them with [`OptionsScreen::pack_icon_images`] and hands the
//! regions back with [`OptionsScreen::assign_pack_icon_sheets`].

use umicraft_assets::pack_discovery::DiscoveredPack;
use umicraft_assets::{DynamicImage, GuiAtlas, DYNAMIC_PREFIX};
use umicraft_core::lang::Lang;
use umicraft_ui::legacy_text::{parse_legacy, wrap_legacy};
use umicraft_ui::styled_text::draw_styled;
use umicraft_ui::{Color, FontAtlas, InputEvent, Rect, SpriteSheet, Ui};

use umicraft_ui::menus::{ConfirmOutcome, ConfirmScreen};

use super::layout::{fit_text, Gui, BUTTON_H, GREY, HALF_W, LIST_BAR};
use super::{OptionsScreen, Outcome, Page};
use crate::official::sprite;
use crate::options::Options;
use crate::servers::icon::decode_image;

/// Width of one list.
const LIST_W: f32 = 200.0;
/// Height of one entry (`GuiResourcePackList`).
const ROW: f32 = 36.0;
/// Height of the list header (`FONT_HEIGHT * 1.5`).
const HEADER: f32 = 13.0;
/// Size of the icon slot of an entry.
const ICON: f32 = 32.0;
/// The width the name and the description of an entry are cut or wrapped at
/// (`ResourcePackListEntry.drawEntry`).
const TEXT_W: f32 = 157.0;
/// The space between two lines of the description.
const LINE: f32 = 10.0;
/// The largest side of a `pack.png` that is read; a larger image keeps the
/// `unknown_pack.png` icon instead of being unpacked in memory.
const MAX_ICON_SIDE: u32 = 1024;
/// The side the icons take in the atlas: the 32 unit slot at GUI scale 2.
const ICON_TEXTURE: u32 = 64;
/// The shade over the icon of the entry under the pointer (`0xA0909090`).
const ICON_SHADE: Color = [0.565, 0.565, 0.565, 0.627];
/// The `pack_format` of 1.8 packs (`ResourcePackRepository`).
const PACK_FORMAT: u32 = 1;
/// The background of an incompatible pack (`0xFF770000`).
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
    /// Clicked an incompatible pack in the available list: ask first.
    /// `newer` is whether it was made for a newer `pack_format`.
    Ask { name: String, newer: bool },
}

/// One entry to draw.
struct Row {
    /// The pack file name; `None` for the built-in "Default".
    pack: Option<String>,
    title: String,
    /// The description with its `§` codes.
    description: String,
    compatible: bool,
    /// The `pack_format` of the metadata, when there is one.
    format: Option<u32>,
    /// The `pack.png` region of the atlas, when the pack has an icon.
    icon: Option<SpriteSheet>,
}

impl Row {
    fn of(pack: &DiscoveredPack, icon: Option<SpriteSheet>, lang: &Lang) -> Self {
        let (description, compatible, format) = match &pack.metadata {
            Some(metadata) => (
                metadata.description.clone(),
                metadata.pack_format == PACK_FORMAT,
                Some(metadata.pack_format),
            ),
            None => (
                lang.tr("umicraft.resourcePack.missingMetadata").to_string(),
                true,
                None,
            ),
        };
        Self {
            pack: Some(pack.name.clone()),
            title: pack.name.clone(),
            description,
            compatible,
            format,
            icon,
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
            format: None,
            icon: None,
        }
    }
}

/// `rgba` scaled down to [`ICON_TEXTURE`] square by averaging the source
/// pixels each new pixel covers (vanilla draws a `pack.png` of any size into
/// its 32x32 slot). An image that is small enough is returned as it is.
fn shrink(width: u32, height: u32, rgba: Vec<u8>) -> (u32, u32, Vec<u8>) {
    if width <= ICON_TEXTURE && height <= ICON_TEXTURE {
        return (width, height, rgba);
    }
    let side = ICON_TEXTURE as usize;
    let (w, h) = (width as usize, height as usize);
    let mut out = Vec::with_capacity(side * side * 4);
    for y in 0..side {
        let (y0, y1) = (y * h / side, ((y + 1) * h / side).max(y * h / side + 1));
        for x in 0..side {
            let (x0, x1) = (x * w / side, ((x + 1) * w / side).max(x * w / side + 1));
            let mut sum = [0_u32; 4];
            for sy in y0..y1.min(h) {
                for sx in x0..x1.min(w) {
                    let at = (sy * w + sx) * 4;
                    for (channel, total) in sum.iter_mut().enumerate() {
                        *total += u32::from(rgba[at + channel]);
                    }
                }
            }
            let count = ((y1.min(h) - y0) * (x1.min(w) - x0)).max(1) as u32;
            out.extend(sum.map(|total| (total / count) as u8));
        }
    }
    (ICON_TEXTURE, ICON_TEXTURE, out)
}

/// `text` cut to `max_width` with a trailing "..." when it is wider
/// (`ResourcePackListEntry.drawEntry`).
fn ellipsize(atlas: &FontAtlas, text: &str, max_width: f32) -> String {
    if atlas.measure(text, 1.0) <= max_width {
        return text.to_string();
    }
    let dots = "...";
    format!(
        "{}{dots}",
        fit_text(atlas, text, max_width - atlas.measure(dots, 1.0))
    )
}

impl OptionsScreen {
    /// The `pack.png` of every pack of the catalog as an atlas image named
    /// `pack<index>`, for the packs that have one the atlas can take.
    #[must_use]
    pub fn pack_icon_images(&self) -> Vec<DynamicImage> {
        self.catalog
            .packs()
            .iter()
            .enumerate()
            .filter_map(|(index, pack)| {
                let (width, height, pixels) = decode_image(pack.icon.as_deref()?, MAX_ICON_SIDE)?;
                let (width, height, pixels) = shrink(width, height, pixels);
                Some(DynamicImage {
                    name: format!("pack{index}"),
                    width,
                    height,
                    pixels,
                })
            })
            .collect()
    }

    /// Points each pack at its icon region in `atlas`, which the window built
    /// from [`Self::pack_icon_images`]; a pack without a region keeps
    /// `unknown_pack.png`.
    pub fn assign_pack_icon_sheets(&mut self, atlas: &GuiAtlas) {
        self.pack_icons = (0..self.catalog.packs().len())
            .map(|index| {
                atlas
                    .region(&format!("{DYNAMIC_PREFIX}pack{index}"))
                    .map(sprite)
            })
            .collect();
    }

    /// Draws `text` with its `§` codes and the vanilla shadow, left edge at
    /// `x` and top at `y` (GUI units); `base` colours it before any code.
    fn legacy_at(
        ui: &mut Ui,
        atlas: &FontAtlas,
        gui: Gui,
        (x, y): (f32, f32),
        (text, base): (&str, Color),
    ) {
        let runs = parse_legacy(text, base);
        let pixel = [(x * gui.scale).round(), (y * gui.scale).round()];
        draw_styled(ui.draw(), atlas, pixel, &runs, gui.scale, true, 0);
    }

    /// The icon sprite of the pack `name`, when it has one.
    fn icon_of(&self, name: &str) -> Option<SpriteSheet> {
        let index = self
            .catalog
            .packs()
            .iter()
            .position(|pack| pack.name == name)?;
        self.pack_icons.get(index).copied().flatten()
    }

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
        if let Some((name, confirm)) = self.pack_confirm.take() {
            // `GuiYesNo` replaces the screen until answered.
            match confirm.show(ui, atlas, lang) {
                Some(ConfirmOutcome::Yes) => self.apply_pack_action(Action::Move(name), lang),
                Some(ConfirmOutcome::No) => {}
                None => self.pack_confirm = Some((name, confirm)),
            }
            return;
        }
        let (top, bottom) = (32.0, gui.height - 51.0);
        let cx = gui.cx();
        let available: Vec<Row> = self
            .catalog
            .packs()
            .iter()
            .filter(|pack| !self.pack_draft.contains(&pack.name))
            .map(|pack| Row::of(pack, self.icon_of(&pack.name), lang))
            .collect();
        let selected: Vec<Row> = self
            .pack_draft
            .iter()
            .filter_map(|name| self.catalog.packs().iter().find(|pack| &pack.name == name))
            .map(|pack| Row::of(pack, self.icon_of(&pack.name), lang))
            .chain(std::iter::once(Row::default_pack(lang)))
            .collect();

        self.scroll_columns(
            ui,
            gui,
            (cx, top),
            [available.len(), selected.len()],
            bottom - top,
        );
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
            self.apply_pack_action(action, lang);
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
    fn apply_pack_action(&mut self, action: Action, lang: &Lang) {
        let draft = &mut self.pack_draft;
        match action {
            Action::Ask { name, newer } => {
                let message = lang.tr(if newer {
                    "resourcePack.incompatible.confirm.new"
                } else {
                    "resourcePack.incompatible.confirm.old"
                });
                let screen = ConfirmScreen::new(
                    "resourcePack.incompatible.confirm.title",
                    message,
                    "gui.yes",
                    "gui.no",
                );
                self.pack_confirm = Some((name, screen));
            }
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
    fn scroll_columns(
        &mut self,
        ui: &Ui,
        gui: Gui,
        (cx, top): (f32, f32),
        rows: [usize; 2],
        visible: f32,
    ) {
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
            // The scrollbar sits inside the right edge of its list.
            let left = if column == 1 {
                cx + 4.0
            } else {
                cx - 4.0 - LIST_W
            };
            *scroll = self.pack_bars[column].apply(
                ui,
                gui.scale,
                [left + LIST_W - 6.0, top, visible],
                (content, max),
                *scroll,
            );
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
            // The neighbours a pack can swap with: the built-in "Default" at
            // the end of the selected list cannot be passed.
            let moves = (
                index > 0,
                rows.get(index + 1).is_some_and(|next| next.pack.is_some()),
            );
            action = action.or(self.pack_row(ui, atlas, gui, (x, y), (row, column, moves), &tag));
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

    /// One entry: tinted when incompatible, the icon, the name and the
    /// description. Under the pointer the icon is shaded and shows the
    /// arrows of `resource_packs.png`; `moves` tells whether the pack can go
    /// up and down. Returns what a click on the icon did.
    fn pack_row(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        gui: Gui,
        (x, y): (f32, f32),
        (row, column, (can_up, can_down)): (&Row, Column, (bool, bool)),
        tag: &str,
    ) -> Option<Action> {
        if !row.compatible {
            ui.draw().rect(
                gui.rect(x - 1.0, y - 1.0, LIST_W - 8.0, ROW + 2.0),
                INCOMPATIBLE,
            );
        }
        // The pack's `pack.png`, else `unknown_pack.png`; the built-in
        // "Default" has the `pack.png` of the jar.
        let art = ui.menu_art();
        let sheet = match row.pack {
            Some(_) => row.icon.or(art.unknown_pack),
            None => art.default_pack,
        };
        let icon = gui.rect(x, y, ICON, ICON);
        match sheet {
            Some(sheet) => {
                ui.draw()
                    .image(sheet.sub(0, 0, sheet.width, sheet.height), icon, [1.0; 4])
            }
            None => ui.draw().rect(icon, LIST_BAR),
        }
        let text_at = x + ICON + 3.0;
        let title = ellipsize(atlas, &row.title, TEXT_W);
        Self::legacy_at(ui, atlas, gui, (text_at, y + 1.0), (&title, [1.0; 4]));
        for (line, text) in wrap_legacy(atlas, &row.description, TEXT_W)
            .iter()
            .take(2)
            .enumerate()
        {
            let at = (text_at, y + 12.0 + LINE * line as f32);
            Self::legacy_at(ui, atlas, gui, at, (text, GREY));
        }
        // The built-in "Default" cannot be moved.
        let pack = row.pack.clone()?;
        let selected = column == Column::Selected;
        let entry = gui.rect(x, y, LIST_W - 8.0, ROW);
        let hovered = ui.pointer().is_some_and(|point| entry.contains(point));
        // The pointer's place on the icon, in GUI units (`mouseX - x`, `mouseY - y`).
        let over = ui
            .pointer()
            .map(|p| ((p[0] - icon.x) / gui.scale, (p[1] - icon.y) / gui.scale));
        let (px, py) = over.unwrap_or((-1.0, -1.0));
        if hovered {
            ui.draw().rect(icon, ICON_SHADE);
            if let Some(arrows) = art.pack_arrows {
                // `ResourcePackListEntry.drawEntry`: the cell above is the
                // idle sprite, the one below (v = 32) the hovered one.
                let mut cell = |u: u32, lit: bool| {
                    let v = if lit { 32 } else { 0 };
                    ui.draw().image(arrows.sub(u, v, 32, 32), icon, [1.0; 4]);
                };
                if !selected {
                    cell(0, px < 32.0);
                } else {
                    cell(32, px < 16.0);
                    if can_up {
                        cell(96, px < 32.0 && px > 16.0 && py < 16.0);
                    }
                    if can_down {
                        cell(64, px < 32.0 && px > 16.0 && py > 16.0);
                    }
                }
            }
        }
        let zone = |ui: &mut Ui,
                    rects: &mut std::collections::HashMap<String, Rect>,
                    name: &str,
                    rect: Rect| {
            let id = format!("pack:{name}:{tag}");
            rects.insert(id.clone(), rect);
            let widget = ui.id(&id);
            ui.response(widget, rect).clicked
        };
        let half = |left: bool, top: Option<bool>| {
            let (w, h) = (ICON / 2.0, ICON / 2.0);
            let (dx, dy, h) = match top {
                Some(true) => (if left { 0.0 } else { w }, 0.0, h),
                Some(false) => (if left { 0.0 } else { w }, h, h),
                None => (if left { 0.0 } else { w }, 0.0, ICON),
            };
            gui.rect(x + dx, y + dy, w, h)
        };
        if !selected {
            // `mousePressed`: the whole icon enables the pack; an incompatible
            // one asks first.
            let clicked = zone(ui, &mut self.rects, "icon", gui.rect(x, y, ICON, ICON));
            return match row.format {
                Some(format) if clicked && !row.compatible => Some(Action::Ask {
                    name: pack,
                    newer: format > PACK_FORMAT,
                }),
                _ => clicked.then_some(Action::Move(pack)),
            };
        }
        let mut action = None;
        if zone(ui, &mut self.rects, "out", half(true, None)) {
            action = Some(Action::Move(pack.clone()));
        }
        if can_up && zone(ui, &mut self.rects, "up", half(false, Some(true))) {
            action = Some(Action::Up(pack.clone()));
        }
        if can_down && zone(ui, &mut self.rects, "down", half(false, Some(false))) {
            action = Some(Action::Down(pack));
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options_screen::Catalog;
    use crate::servers::icon::test_png;

    fn pack(name: &str, icon: Option<Vec<u8>>) -> DiscoveredPack {
        DiscoveredPack {
            name: name.to_string(),
            metadata: None,
            icon,
        }
    }

    #[test]
    fn only_the_icons_the_atlas_can_take_are_named_by_catalog_index() {
        let mut screen = OptionsScreen::new();
        screen.set_catalog(Catalog::new(
            Vec::new(),
            vec![
                pack("none.zip", None),
                pack("Folder/small.zip", Some(test_png(128, [1, 2, 3, 255]))),
                pack("big.zip", Some(test_png(512, [9, 9, 9, 255]))),
                pack("huge.zip", Some(test_png(2048, [0; 4]))),
                pack("broken.zip", Some(b"not a png".to_vec())),
            ],
        ));
        let images = screen.pack_icon_images();
        let names: Vec<_> = images.iter().map(|image| image.name.as_str()).collect();
        assert_eq!(
            names,
            ["pack1", "pack2"],
            "indices follow the catalog; 2048 px is refused"
        );
        // Both are shrunk to the atlas icon size, keeping their colour.
        for image in &images {
            assert_eq!((image.width, image.height), (ICON_TEXTURE, ICON_TEXTURE));
        }
        assert_eq!(&images[1].pixels[..4], &[9, 9, 9, 255]);
    }

    #[test]
    fn a_long_name_is_cut_with_dots_and_a_short_one_is_kept() {
        let atlas = FontAtlas::fallback();
        assert_eq!(ellipsize(&atlas, "a.zip", TEXT_W), "a.zip");
        let long = "x".repeat(100);
        let cut = ellipsize(&atlas, &long, TEXT_W);
        assert!(cut.ends_with("..."));
        assert!(atlas.measure(&cut, 1.0) <= TEXT_W);
    }
}

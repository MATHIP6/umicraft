// SPDX-License-Identifier: MIT OR Apache-2.0

//! The video settings pages, laid out like OptiFine's (ADR-0089).
//!
//! The root page holds the main options and one button per sub-page
//! (Quality, Details, Performance, Animations, Other). Every page is built
//! from the option table `options::video_spec`:
//! - its options come from that table, in table order, two per row;
//! - its extra buttons follow (the sub-page buttons on the root page, "All ON"
//!   and "All OFF" on Animations, "Reset Video Settings..." on Other);
//! - the tooltip of an option appears once the pointer rests on it
//!   ([`HoverDelay`]). Its lines are the language keys
//!   `umicraft.options.tooltip.<id>.1`, `.2`, and so on.
//!
//! The vanilla page (`GuiVideoSettings`) is replaced: its options are spread
//! over these pages as OptiFine spreads them.

use umicraft_core::lang::Lang;
use umicraft_ui::menus::{ConfirmOutcome, ConfirmScreen};
use umicraft_ui::widgets::tooltip::{tooltip, TooltipRow};
use umicraft_ui::{FontAtlas, Rect, Ui};

use super::layout::{Gui, BUTTON_H, HALF_W, VIDEO_ROW};
use super::{option_label, OptionsScreen, Outcome, Page};
use crate::options::video_spec::{set_all, specs_on, Control, OptionSpec, VideoPage};
use crate::options::{Options, VideoOptions};

/// The colour of a tooltip line (OptiFine's light grey).
const TOOLTIP_RGB: u32 = 0xDC_DC_DC;
/// The colour of a tooltip line ending in `!`, a warning.
const WARNING_RGB: u32 = 0xFF_55_55;
/// The most lines a tooltip reads.
const MAX_TOOLTIP_LINES: usize = 10;

/// One button of a video page.
#[derive(Debug, Clone, Copy)]
enum Item {
    /// An option of the table.
    Option(&'static OptionSpec),
    /// A button to a sub-page.
    Open(VideoPage),
    /// "All ON" / "All OFF" of the Animations page.
    All { on: bool },
    /// "Reset Video Settings..." of the Other page.
    Reset,
}

impl Item {
    /// The widget id, stable across frames and versions.
    fn id(self) -> String {
        match self {
            Self::Option(spec) => format!("video:{}", spec.id),
            Self::Open(page) => format!("video-nav:{}", page.name()),
            Self::All { on: true } => "video:allOn".to_string(),
            Self::All { on: false } => "video:allOff".to_string(),
            Self::Reset => "video:reset".to_string(),
        }
    }
}

/// The buttons of `page`: its options, then its extra buttons.
fn items(page: VideoPage) -> Vec<Item> {
    let mut items: Vec<Item> = specs_on(page).map(Item::Option).collect();
    match page {
        VideoPage::Root => items.extend(VideoPage::SUB_PAGES.map(Item::Open)),
        VideoPage::Animations => {
            items.extend([Item::All { on: true }, Item::All { on: false }]);
        }
        VideoPage::Other => items.push(Item::Reset),
        VideoPage::Details | VideoPage::Quality | VideoPage::Performance => {}
    }
    items
}

/// Where one button is drawn and whether a click on it counts.
#[derive(Clone, Copy)]
struct Slot {
    rect: Rect,
    /// The row lies fully inside the list area, so it can be clicked.
    inside: bool,
}

impl OptionsScreen {
    /// One video page.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn show_video(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        gui: Gui,
        lang: &Lang,
        outcome: &mut Outcome,
        page: VideoPage,
    ) {
        if let Some(confirm) = self.video_reset.take() {
            self.show_reset_confirm(ui, atlas, options, lang, outcome, confirm);
            return;
        }
        let (top, bottom) = (32.0, gui.height - 32.0);
        let cx = gui.cx();
        let items = items(page);
        let content = items.len().div_ceil(2) as f32 * VIDEO_ROW;
        self.scroll_list(ui, gui, (cx + 156.0, top, bottom), (content, VIDEO_ROW));
        Self::list_container(ui, gui, (0.0, gui.width), (top, bottom), self.scroll);

        // The list is clipped to its area; a row is only clickable while it
        // lies inside.
        let mut hovered = None;
        ui.push_clip(gui.rect(0.0, top, gui.width, bottom - top));
        for (index, item) in items.into_iter().enumerate() {
            let x = cx - 155.0 + (index % 2) as f32 * 160.0;
            let y = top + 4.0 - self.scroll + (index / 2) as f32 * VIDEO_ROW;
            let slot = Slot {
                rect: gui.rect(x, y, HALF_W, BUTTON_H),
                inside: y >= top && y + BUTTON_H <= bottom,
            };
            if let Item::Option(spec) = item {
                if slot.inside && ui.is_hovered(slot.rect) {
                    hovered = Some(spec.id);
                }
            }
            self.video_item(ui, atlas, options, lang, item, slot, outcome);
        }
        ui.pop_clip();
        Self::list_overlay(ui, gui, (0.0, gui.width), (top, bottom));
        Self::scrollbar(ui, gui, cx + 156.0, (top, bottom), content, self.scroll);

        let title = match page {
            VideoPage::Root => lang.tr("options.videoTitle").to_string(),
            sub => lang
                .tr(&format!("umicraft.options.video.{}.title", sub.name()))
                .to_string(),
        };
        Self::title(ui, atlas, gui, &title, 5.0);
        let done = gui.rect(cx - 100.0, gui.height - 27.0, 200.0, BUTTON_H);
        self.back(ui, atlas, done, lang);
        self.video_tooltip(ui, atlas, gui, lang, hovered);
    }

    /// Draws one button and applies a click on it.
    #[allow(clippy::too_many_arguments)]
    fn video_item(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        lang: &Lang,
        item: Item,
        slot: Slot,
        outcome: &mut Outcome,
    ) {
        let id = item.id();
        match item {
            Item::Option(spec) => {
                self.video_option(
                    ui,
                    atlas,
                    &mut options.video,
                    lang,
                    spec,
                    &id,
                    slot,
                    outcome,
                );
            }
            Item::Open(page) => {
                let key = format!("umicraft.options.video.{}", page.name());
                let label = lang.tr(&key).to_string();
                if self.live_button(ui, atlas, &id, slot, &label) {
                    self.page = Page::Video(page);
                    self.scroll = 0.0;
                }
            }
            Item::All { on } => {
                let key = if on {
                    "umicraft.options.video.allOn"
                } else {
                    "umicraft.options.video.allOff"
                };
                if self.live_button(ui, atlas, &id, slot, lang.tr(key)) {
                    set_all(&mut options.video, on);
                    outcome.changed = true;
                }
            }
            Item::Reset => {
                let label = lang.tr("umicraft.options.video.reset");
                if self.live_button(ui, atlas, &id, slot, label) {
                    self.video_reset = Some(ConfirmScreen::new(
                        "umicraft.options.video.reset.title",
                        lang.tr("umicraft.options.video.reset.message"),
                        "gui.yes",
                        "gui.no",
                    ));
                }
            }
        }
    }

    /// Draws one option as its button or slider and applies a change.
    #[allow(clippy::too_many_arguments)]
    fn video_option(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        video: &mut VideoOptions,
        lang: &Lang,
        spec: &OptionSpec,
        id: &str,
        slot: Slot,
        outcome: &mut Outcome,
    ) {
        let label = option_label(lang, spec.label, &(spec.value)(lang, video));
        let position = match spec.control {
            Control::Button => self
                .live_button(ui, atlas, id, slot, &label)
                .then(|| spec.next(video)),
            Control::Slider => self
                .live_slider(ui, atlas, id, slot, &label, spec.fraction(video))
                .map(|fraction| spec.position_at(fraction)),
        };
        if let Some(position) = position.filter(|&position| position != (spec.get)(video)) {
            (spec.set)(video, position);
            outcome.changed = true;
        }
    }

    /// The reset question; Yes puts every video option back to its default.
    fn show_reset_confirm(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        options: &mut Options,
        lang: &Lang,
        outcome: &mut Outcome,
        confirm: ConfirmScreen,
    ) {
        match confirm.show(ui, atlas, lang) {
            Some(ConfirmOutcome::Yes) => {
                options.video = VideoOptions::default();
                outcome.changed = true;
            }
            Some(ConfirmOutcome::No) => {}
            None => self.video_reset = Some(confirm),
        }
    }

    /// The tooltip of the option the pointer rests on, if any.
    fn video_tooltip(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        gui: Gui,
        lang: &Lang,
        hovered: Option<&'static str>,
    ) {
        let (Some(now), Some(pointer)) = (self.clock, ui.pointer()) else {
            return;
        };
        let in_gui = [pointer[0] / gui.scale, pointer[1] / gui.scale];
        let Some(&id) = self.video_hover.update(hovered, in_gui, now) else {
            return;
        };
        let rows = tooltip_rows(lang, id);
        tooltip(ui, atlas, ui.viewport(), gui.scale, pointer, &rows);
    }

    /// A button of the list; true when it was clicked while fully visible.
    fn live_button(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        id: &str,
        slot: Slot,
        label: &str,
    ) -> bool {
        self.button(ui, atlas, id, slot.rect, label, true, None) && slot.inside
    }

    /// A slider of the list; its new fraction when it moved. A row that is
    /// only partly visible is not drawn as a slider, only registered.
    fn live_slider(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        id: &str,
        slot: Slot,
        label: &str,
        fraction: f32,
    ) -> Option<f32> {
        if slot.inside {
            self.slider(ui, atlas, id, slot.rect, label, fraction)
        } else {
            self.rects.insert(id.to_string(), slot.rect);
            None
        }
    }
}

/// The lines of the tooltip of option `id`: `umicraft.options.tooltip.<id>.N`
/// from 1 until the first missing key. A line ending in `!` is a warning,
/// drawn red as OptiFine does.
fn tooltip_rows(lang: &Lang, id: &str) -> Vec<TooltipRow> {
    (1..=MAX_TOOLTIP_LINES)
        .map_while(|line| lang.get(&format!("umicraft.options.tooltip.{id}.{line}")))
        .map(|text| TooltipRow {
            rgb: if text.ends_with('!') {
                WARNING_RGB
            } else {
                TOOLTIP_RGB
            },
            text: text.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::video_spec::VIDEO_SPECS;

    #[test]
    fn every_page_lists_its_options_then_its_extra_buttons() {
        let root = items(VideoPage::Root);
        assert!(matches!(root.first(), Some(Item::Option(spec)) if spec.id == "graphics"));
        assert!(matches!(root.last(), Some(Item::Open(VideoPage::Other))));
        assert!(matches!(
            items(VideoPage::Animations).last(),
            Some(Item::All { on: false })
        ));
        assert!(matches!(items(VideoPage::Other).last(), Some(Item::Reset)));
    }

    #[test]
    fn every_option_is_on_exactly_one_page() {
        let pages = [VideoPage::Root].into_iter().chain(VideoPage::SUB_PAGES);
        let listed: usize = pages
            .map(|page| {
                items(page)
                    .iter()
                    .filter(|item| matches!(item, Item::Option(_)))
                    .count()
            })
            .sum();
        assert_eq!(listed, VIDEO_SPECS.len());
    }

    #[test]
    fn every_option_has_a_tooltip_in_both_languages() {
        for code in ["en_US", "fr_FR"] {
            let lang = Lang::from_texts(code, None, None)
                .with_client_layer(None, umicraft_assets::client_pack::lang(code));
            for spec in VIDEO_SPECS {
                let rows = tooltip_rows(&lang, spec.id);
                assert!(!rows.is_empty(), "{code}: no tooltip for {}", spec.id);
            }
        }
    }

    #[test]
    fn a_line_ending_in_a_bang_is_red() {
        let lang = Lang::from_texts(
            "en_US",
            Some(
                "umicraft.options.tooltip.x.1=Plain\numicraft.options.tooltip.x.2=Careful!\n\
                 umicraft.options.tooltip.x.4=Unreachable",
            ),
            None,
        );
        let rows = tooltip_rows(&lang, "x");
        assert_eq!(rows.len(), 2, "the lines stop at the first gap");
        assert_eq!((rows[0].rgb, rows[1].rgb), (TOOLTIP_RGB, WARNING_RGB));
    }
}

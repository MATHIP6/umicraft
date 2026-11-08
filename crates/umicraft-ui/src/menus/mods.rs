// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mods menu, opened with the right Shift key.
//!
//! Laid out like the Lunar Client menu and kept small: over the blurred game,
//! with no panel, the logo, the "UMICRAFT" wordmark under it and one **Mods**
//! button, with a muted hint below. A close cross sits in the top right corner
//! of the screen, as on the title screen. The renderer blurs the game behind
//! it; this screen only adds a light dark sheet. The button only reports
//! [`ModsOutcome::OpenMods`] for now; the mods themselves do not exist yet,
//! this is the interface.

use umicraft_core::lang::Lang;

use crate::context::{Ui, UiResponse};
use crate::draw::Color;
use crate::geom::Rect;
use crate::id::WidgetId;
use crate::menus::common::Gui;
use crate::menus::title::modern::wordmark::{draw_logo, draw_wordmark};
use crate::menus::title::modern::{lowered, scaled, Stage};
use crate::modern::theme::{mix, with_opacity};
use crate::modern::{draw_text, measure_text, theme, Align, HoverFade, ModernButton};
use crate::text::FontAtlas;

/// Edge of the square logo box, in GUI units.
const LOGO_SIZE: f32 = 30.0;
/// Gap between the logo and the wordmark.
const LOGO_GAP: f32 = 2.0;
/// Height of the wordmark band.
const WORDMARK_HEIGHT: f32 = 12.0;
/// The wordmark is drawn at this share of its title-screen size.
const WORDMARK_SCALE: f32 = 0.55;
/// Gap between the wordmark and the button, and between the button and the hint.
const SECTION_GAP: f32 = 8.0;
/// Width of the Mods button.
const BUTTON_WIDTH: f32 = 112.0;
/// Height of the Mods button.
const BUTTON_HEIGHT: f32 = 20.0;
/// Thickness of the thin rim of the Mods button, in GUI units.
const RIM_WIDTH: f32 = 0.5;
/// The rim at rest: a faint white line.
const RIM: Color = [1.0, 1.0, 1.0, 0.14];
/// The rim when the pointer is over the button: the accent colour.
const RIM_HOVER: Color = [0.36, 0.55, 1.0, 0.70];
/// Edge of the module grid icon, in GUI units: two squares and a gap.
const ICON_SIZE: f32 = 7.0;
/// Space between the icon and the label.
const ICON_GAP: f32 = 5.0;
/// Space between the capitals of the label, as a share of their size.
const LABEL_TRACKING: f32 = 0.25;
/// Edge of the square close cross.
const CLOSE_SIZE: f32 = 14.0;
/// Distance of the close cross from the top and right edges.
const CLOSE_MARGIN: f32 = 7.0;
/// The label of the close button, as on the title screen.
const CLOSE_CROSS: &str = "\u{d7}";

/// A light dark sheet over the blurred game, so the menu reads well on bright
/// worlds.
const BACKDROP: Color = [0.0, 0.0, 0.0, 0.25];

/// Ticks the opening animation lasts (0.6 s at 20 TPS): the end of its last
/// stage. Only the logo, the dark sheet and the blur move; every text and
/// button is there from the first frame.
const OPEN_TICKS: u8 = 12;
/// The dark sheet comes in.
const SHEET: Stage = Stage::new(0.0, 8.0);
/// The blur of the game behind fades in, a little slower than the sheet.
const BLUR: Stage = Stage::new(0.0, 12.0);
/// The logo grows, rises and fades in.
const LOGO: Stage = Stage::new(0.0, 10.0);
/// How far the logo rises while it grows, in GUI units.
const LOGO_RISE: f32 = 8.0;
/// Size of the logo when it starts to grow, as a fraction of its final size.
const LOGO_START_SCALE: f32 = 0.8;

/// What the player did on the mods menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModsOutcome {
    /// The close cross was clicked.
    Close,
    /// The Mods button was clicked.
    OpenMods,
}

/// The rectangles of the mods menu, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModsLayout {
    /// The square box the logo is fitted into.
    pub logo: Rect,
    /// The band the wordmark is centred in.
    pub wordmark: Rect,
    /// The Mods button.
    pub mods: Rect,
    /// Where the hint is centred.
    pub hint: [f32; 2],
    /// The close cross in the top right corner of the screen.
    pub close: Rect,
}

/// Computes the layout for `viewport`: the group of logo, wordmark, button and
/// hint is centred on the screen.
#[must_use]
pub fn mods_layout(viewport: Rect) -> ModsLayout {
    let gui = Gui::new(viewport);
    let cx = gui.width * 0.5;
    let block = LOGO_SIZE
        + LOGO_GAP
        + WORDMARK_HEIGHT
        + SECTION_GAP
        + BUTTON_HEIGHT
        + SECTION_GAP
        + theme::TEXT_SIZE_SMALL;
    let top = ((gui.height - block) * 0.5).floor();
    let wordmark = top + LOGO_SIZE + LOGO_GAP;
    let button = wordmark + WORDMARK_HEIGHT + SECTION_GAP;
    let hint = gui.rect(
        cx,
        button + BUTTON_HEIGHT + SECTION_GAP + theme::TEXT_SIZE_SMALL * 0.5,
        0.0,
        0.0,
    );
    ModsLayout {
        logo: gui.rect(cx - LOGO_SIZE * 0.5, top, LOGO_SIZE, LOGO_SIZE),
        wordmark: gui.rect(0.0, wordmark, gui.width, WORDMARK_HEIGHT),
        mods: gui.rect(cx - BUTTON_WIDTH * 0.5, button, BUTTON_WIDTH, BUTTON_HEIGHT),
        hint: [hint.x, hint.y],
        close: gui.rect(
            gui.width - CLOSE_MARGIN - CLOSE_SIZE,
            CLOSE_MARGIN,
            CLOSE_SIZE,
            CLOSE_SIZE,
        ),
    }
}

/// The mods menu state: the hover fades of its two buttons and how far the
/// opening animation has come. The flow makes a new one each time the menu
/// opens, so the animation plays again.
#[derive(Debug, Clone, Default)]
pub struct ModsScreen {
    fades: [HoverFade; 2],
    hovered: [bool; 2],
    /// Ticks since the menu opened, up to [`OPEN_TICKS`].
    ticks: u8,
}

impl ModsScreen {
    /// Creates the menu with both buttons at rest.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Ticks since the menu opened, `partial_tick` of the way into the current
    /// one; it stands still once the animation is over.
    fn elapsed(&self, partial_tick: f32) -> f32 {
        let running = self.ticks < OPEN_TICKS;
        let partial = if running {
            partial_tick.clamp(0.0, 1.0)
        } else {
            0.0
        };
        f32::from(self.ticks) + partial
    }

    /// How blurred the game behind should be, `0.0` to `1.0`: the renderer
    /// fades the blur in with it.
    #[must_use]
    pub fn blur_strength(&self, partial_tick: f32) -> f32 {
        BLUR.at(self.elapsed(partial_tick))
    }

    /// Advances the opening animation and the hover fades by one game tick.
    pub fn tick(&mut self) {
        self.ticks = (self.ticks + 1).min(OPEN_TICKS);
        for (fade, hovered) in self.fades.iter_mut().zip(self.hovered) {
            fade.tick(hovered);
        }
    }

    /// Draws one frame and returns what the player clicked. `brand` is the
    /// brand atlas and `partial_tick` how far the tick has come.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        brand: &FontAtlas,
        lang: &Lang,
        partial_tick: f32,
    ) -> Option<ModsOutcome> {
        let viewport = ui.viewport();
        let gui = Gui::new(viewport);
        let layout = mods_layout(viewport);

        let elapsed = self.elapsed(partial_tick);
        let logo_in = LOGO.at(elapsed);

        let sheet = with_opacity(BACKDROP, SHEET.at(elapsed));
        ui.draw().rect(viewport, sheet);
        let logo_box = lowered(layout.logo, LOGO_RISE, gui.scale, logo_in);
        let logo_box = scaled(
            logo_box,
            LOGO_START_SCALE + (1.0 - LOGO_START_SCALE) * logo_in,
        );
        let logo = ui.menu_art().brand.logo;
        draw_logo(ui, logo, logo_box, logo_in);
        let name_scale = gui.scale * WORDMARK_SCALE;
        draw_wordmark(ui, brand, layout.wordmark, name_scale, 1.0);
        let hint = lang.tr("umicraft.mods.hint");
        let hint_size = theme::TEXT_SIZE_SMALL * gui.scale;
        draw_text(
            ui,
            brand,
            hint,
            (layout.hint[0], layout.hint[1]),
            (hint_size, theme::TEXT_MUTED, Align::Center),
        );

        ui.push_id("mods_menu");
        let levels = self.fades.map(|fade| fade.level(partial_tick));
        let mods_id = ui.id("mods");
        let close = ModernButton::new(ui.id("close"), layout.close, CLOSE_CROSS);
        let label = lang.tr("umicraft.mods.button");
        let responses = [
            mods_button(
                ui,
                brand,
                (mods_id, layout.mods),
                (gui.scale, levels[0]),
                label,
            ),
            close.show(ui, brand, gui.scale, levels[1]),
        ];
        ui.pop_id();

        self.hovered = responses.map(|response| response.hovered);
        if responses[0].clicked {
            Some(ModsOutcome::OpenMods)
        } else if responses[1].clicked {
            Some(ModsOutcome::Close)
        } else {
            None
        }
    }
}

/// Draws the Mods button and reports its interaction state: a dark rounded
/// card with a thin rim that lights up in the accent colour under the
/// pointer, a small grid of modules and the label in tracked capitals.
///
/// `target` is the widget id and the rectangle, `look` the GUI scale and the
/// hover fade of the button.
fn mods_button(
    ui: &mut Ui,
    brand: &FontAtlas,
    target: (WidgetId, Rect),
    look: (f32, f32),
    label: &str,
) -> UiResponse {
    let (id, rect) = target;
    let (scale, fade) = look;
    let response = ui.response(id, rect);
    if response.clicked {
        ui.note_button_press();
    }

    // The rim is the whole card in its colour; the fill goes over it, one rim
    // width in from every edge.
    let radius = theme::RADIUS * scale;
    let rim = mix(RIM, RIM_HOVER, fade);
    ui.draw().rounded_rect(rect, radius, rim);
    let inset = (RIM_WIDTH * scale).max(1.0);
    let inner = Rect::new(
        rect.x + inset,
        rect.y + inset,
        rect.width - 2.0 * inset,
        rect.height - 2.0 * inset,
    );
    let fill = mix(theme::SURFACE, theme::SURFACE_HOVER, fade);
    ui.draw()
        .rounded_rect(inner, (radius - inset).max(0.0), fill);

    // The icon and the label are centred together.
    let label = label.to_uppercase();
    let size = theme::TEXT_SIZE_BODY * scale;
    let gap = size * LABEL_TRACKING;
    let mut buffer = [0u8; 4];
    let glyph_widths: Vec<f32> = label
        .chars()
        .map(|letter| measure_text(brand, letter.encode_utf8(&mut buffer), size))
        .collect();
    let text_width =
        glyph_widths.iter().sum::<f32>() + gap * label.chars().count().saturating_sub(1) as f32;
    let icon = ICON_SIZE * scale;
    let group = icon + ICON_GAP * scale + text_width;
    let left = rect.x + (rect.width - group) * 0.5;
    let middle = rect.y + rect.height * 0.5;

    let cell = icon * 3.0 / 7.0;
    let step = icon - cell;
    let icon_colour = with_opacity(theme::TEXT, 0.9);
    for (column, row) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
        let square = Rect::new(
            left + column * step,
            middle - icon * 0.5 + row * step,
            cell,
            cell,
        );
        ui.draw().rounded_rect(square, cell * 0.25, icon_colour);
    }
    let mut x = left + icon + ICON_GAP * scale;
    for (letter, width) in label.chars().zip(glyph_widths) {
        let glyph = letter.encode_utf8(&mut buffer);
        draw_text(
            ui,
            brand,
            glyph,
            (x, middle),
            (size, theme::TEXT, Align::Left),
        );
        x += width + gap;
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::{DrawCommand, FontFace, SpriteSheet};
    use crate::menus::common::testing::{click_at, frame, VIEWPORT};
    use crate::menus::title::WORDMARK;
    use crate::menus::{BrandArt, MenuArt};

    fn show(screen: &mut ModsScreen, input: crate::input::UiInput) -> Option<ModsOutcome> {
        let atlas = FontAtlas::fallback();
        frame(input, |ui| screen.show(ui, &atlas, &Lang::builtin(), 0.0))
    }

    #[test]
    fn the_group_is_centred_on_the_screen() {
        let layout = mods_layout(VIEWPORT);
        let half_unit = Gui::new(VIEWPORT).scale * 0.5;
        let centre = VIEWPORT.x + VIEWPORT.width * 0.5;
        for rect in [layout.logo, layout.mods] {
            assert!(
                (rect.x + rect.width * 0.5 - centre).abs() <= half_unit,
                "{rect:?}"
            );
        }
        let middle = VIEWPORT.y + VIEWPORT.height * 0.5;
        assert!(layout.logo.y < middle && layout.hint[1] > middle);
    }

    #[test]
    fn the_parts_stack_from_the_logo_down_to_the_hint() {
        let layout = mods_layout(VIEWPORT);
        let bottom = |rect: Rect| rect.y + rect.height;
        assert!(bottom(layout.logo) <= layout.wordmark.y);
        assert!(bottom(layout.wordmark) <= layout.mods.y);
        assert!(bottom(layout.mods) <= layout.hint[1]);
    }

    #[test]
    fn the_menu_is_small() {
        // The group is much smaller than the screen and its button is smaller
        // than a title-screen button (160 x 22 units).
        let layout = mods_layout(VIEWPORT);
        let group = layout.hint[1] - layout.logo.y;
        assert!(group < VIEWPORT.height * 0.5, "{group}");
        assert!(layout.mods.width < 160.0 * Gui::new(VIEWPORT).scale);
    }

    #[test]
    fn the_cross_is_in_the_top_right_corner_of_the_screen() {
        let layout = mods_layout(VIEWPORT);
        assert!(layout.close.x > VIEWPORT.x + VIEWPORT.width * 0.5);
        assert!(layout.close.y < VIEWPORT.y + VIEWPORT.height * 0.2);
    }

    #[test]
    fn the_two_buttons_report_their_outcome() {
        let layout = mods_layout(VIEWPORT);
        let mut screen = ModsScreen::new();
        assert_eq!(
            show(&mut screen, click_at(layout.mods)),
            Some(ModsOutcome::OpenMods)
        );
        assert_eq!(
            show(&mut screen, click_at(layout.close)),
            Some(ModsOutcome::Close)
        );
    }

    #[test]
    fn a_click_beside_the_buttons_does_nothing() {
        let layout = mods_layout(VIEWPORT);
        let mut screen = ModsScreen::new();
        let away = Rect::new(VIEWPORT.x + 10.0, VIEWPORT.y + 10.0, 8.0, 8.0);
        assert_eq!(show(&mut screen, click_at(away)), None);
        let _ = layout;
    }

    /// A menu whose opening animation is over.
    fn opened() -> ModsScreen {
        let mut screen = ModsScreen::new();
        for _ in 0..30 {
            screen.tick();
        }
        screen
    }

    fn drawn_by(mut screen: ModsScreen, art: MenuArt) -> Vec<DrawCommand> {
        let atlas = FontAtlas::fallback();
        frame(crate::input::UiInput::default(), |ui| {
            ui.set_menu_art(art);
            screen.show(ui, &atlas, &Lang::builtin(), 0.0);
            ui.draw().commands().to_vec()
        })
    }

    fn drawn(art: MenuArt) -> Vec<DrawCommand> {
        drawn_by(opened(), art)
    }

    /// The alpha of the dark sheet: the viewport-sized black `Rect`.
    fn sheet_alpha(commands: &[DrawCommand]) -> Option<f32> {
        commands.iter().find_map(|command| match command {
            DrawCommand::Rect { rect, color } if *rect == VIEWPORT && color[..3] == [0.0; 3] => {
                Some(color[3])
            }
            _ => None,
        })
    }

    /// The letters of the name: single brand glyphs above the Mods button (the
    /// label of the button is spelled in glyphs too).
    fn name_letters(commands: &[DrawCommand]) -> usize {
        let button_top = mods_layout(VIEWPORT).mods.y;
        commands
            .iter()
            .filter(|command| {
                matches!(command, DrawCommand::Text { position, text, style }
                    if style.face == FontFace::Brand
                        && position[1] < button_top
                        && text.chars().count() == 1
                        && WORDMARK.contains(text.as_str()))
            })
            .count()
    }

    #[test]
    fn only_the_sheet_the_logo_and_the_blur_animate_the_text_is_always_there() {
        // The text is complete from the first frame, the sheet is not.
        let first = drawn_by(ModsScreen::new(), MenuArt::default());
        assert_eq!(sheet_alpha(&first), Some(0.0), "the sheet starts clear");
        assert_eq!(name_letters(&first), WORDMARK.len(), "the name is there");
        let done = drawn(MenuArt::default());
        assert_eq!(sheet_alpha(&done), Some(BACKDROP[3]));
        assert_eq!(name_letters(&done), WORDMARK.len());

        // In between the sheet only darkens; the name never changes.
        let mut screen = ModsScreen::new();
        let mut sheet = 0.0;
        for _ in 0..OPEN_TICKS {
            screen.tick();
            let frame = drawn_by(screen.clone(), MenuArt::default());
            let now = sheet_alpha(&frame).unwrap();
            assert!(now >= sheet, "{now} >= {sheet}");
            assert_eq!(name_letters(&frame), WORDMARK.len());
            sheet = now;
        }
        assert_eq!(sheet, BACKDROP[3]);
    }

    #[test]
    fn the_buttons_and_the_hint_do_not_fade_or_move() {
        // The same frame at the first tick and at the end, once the sheet and
        // the logo (absent here) are left out.
        let strip = |commands: Vec<DrawCommand>| -> Vec<DrawCommand> {
            commands
                .into_iter()
                .filter(|command| !matches!(command, DrawCommand::Rect { rect, .. } if *rect == VIEWPORT))
                .collect()
        };
        let first = strip(drawn_by(ModsScreen::new(), MenuArt::default()));
        let done = strip(drawn(MenuArt::default()));
        assert_eq!(first, done);
    }

    #[test]
    fn the_blur_fades_in_over_the_opening() {
        let mut screen = ModsScreen::new();
        assert_eq!(screen.blur_strength(0.0), 0.0, "sharp at the first frame");
        let mut previous = 0.0;
        for _ in 0..OPEN_TICKS {
            screen.tick();
            let now = screen.blur_strength(0.0);
            assert!(now >= previous, "{now} >= {previous}");
            previous = now;
        }
        assert_eq!(previous, 1.0, "fully blurred once open");
        assert_eq!(screen.blur_strength(0.7), 1.0, "and it stays so");
    }

    #[test]
    fn the_logo_grows_into_its_box_while_it_opens() {
        let logo = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 359, 354);
        let art = MenuArt {
            brand: BrandArt {
                logo: Some(logo),
                ..BrandArt::default()
            },
            ..MenuArt::default()
        };
        let width = |commands: Vec<DrawCommand>| {
            commands.into_iter().find_map(|command| match command {
                DrawCommand::Image { src, dst, .. } if src == logo.sub(0, 0, 359, 354) => {
                    Some(dst.width)
                }
                _ => None,
            })
        };
        let start = width(drawn_by(ModsScreen::new(), art)).expect("logo drawn");
        let end = width(drawn(art)).expect("logo drawn");
        assert!(start < end, "{start} < {end}");
        assert!((start / end - LOGO_START_SCALE).abs() < 1e-2);
    }

    #[test]
    fn nothing_but_a_light_sheet_is_drawn_behind_the_buttons() {
        // No panel: no rounded shape is wider than the Mods button.
        let widest = drawn(MenuArt::default())
            .iter()
            .filter_map(|command| match command {
                DrawCommand::RoundedRect { rect, .. } => Some(rect.width),
                _ => None,
            })
            .fold(0.0_f32, f32::max);
        assert!(
            widest <= mods_layout(VIEWPORT).mods.width + 1e-3,
            "{widest}"
        );
    }

    #[test]
    fn the_mods_button_is_a_card_with_a_rim_an_icon_and_tracked_capitals() {
        let layout = mods_layout(VIEWPORT);
        let commands = drawn(MenuArt::default());
        let inside = |rect: &Rect| {
            rect.x >= layout.mods.x - 1e-3
                && rect.x + rect.width <= layout.mods.x + layout.mods.width + 1e-3
                && rect.y >= layout.mods.y - 1e-3
                && rect.y + rect.height <= layout.mods.y + layout.mods.height + 1e-3
        };
        let shapes = commands
            .iter()
            .filter(
                |command| matches!(command, DrawCommand::RoundedRect { rect, .. } if inside(rect)),
            )
            .count();
        // The rim, the fill and the four squares of the icon.
        assert_eq!(shapes, 6);
        // The label is spelled in capitals, one glyph at a time.
        let letters: String = commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, .. } if text.chars().count() == 1 => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(letters.contains("MODS"), "{letters}");
    }

    #[test]
    fn the_rim_lights_up_under_the_pointer() {
        let layout = mods_layout(VIEWPORT);
        let rim_of = |fade: f32| {
            let atlas = FontAtlas::fallback();
            frame(crate::input::UiInput::default(), |ui| {
                let id = ui.id("probe");
                mods_button(ui, &atlas, (id, layout.mods), (1.0, fade), "Mods");
                ui.draw()
                    .commands()
                    .iter()
                    .find_map(|command| match command {
                        DrawCommand::RoundedRect { rect, color, .. } if *rect == layout.mods => {
                            Some(*color)
                        }
                        _ => None,
                    })
            })
            .expect("the rim is the card-sized shape")
        };
        let (rest, hover) = (rim_of(0.0), rim_of(1.0));
        assert!(hover[3] > rest[3], "{hover:?} over {rest:?}");
        assert!(hover[2] > hover[0], "the hovered rim is blue");
    }

    #[test]
    fn the_name_is_written_in_the_brand_face_above_the_button() {
        let commands = drawn(MenuArt::default());
        let letters: String = commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, style, .. } if style.face == FontFace::Brand => {
                    Some(text.as_str())
                }
                _ => None,
            })
            .filter(|text| text.chars().count() == 1 && WORDMARK.contains(*text))
            .collect();
        assert!(letters.contains(WORDMARK), "{letters}");
    }

    #[test]
    fn the_logo_is_drawn_inside_its_box_when_the_sprite_exists() {
        let logo = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 359, 354);
        let art = MenuArt {
            brand: BrandArt {
                logo: Some(logo),
                ..BrandArt::default()
            },
            ..MenuArt::default()
        };
        let area = mods_layout(VIEWPORT).logo;
        let images: Vec<Rect> = drawn(art)
            .into_iter()
            .filter_map(|command| match command {
                DrawCommand::Image { src, dst, .. } if src == logo.sub(0, 0, 359, 354) => Some(dst),
                _ => None,
            })
            .collect();
        assert_eq!(images.len(), 1);
        assert!(images[0].height <= area.height + 1e-3);
        assert!(images[0].y >= area.y - 1e-3);
    }
}

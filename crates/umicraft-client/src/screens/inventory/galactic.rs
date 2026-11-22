// SPDX-License-Identifier: MIT OR Apache-2.0

//! The row names of the enchanting table, written in the standard galactic
//! alphabet (`GuiEnchantment.drawGuiContainerBackgroundLayer` with
//! `Minecraft.standardGalacticFontRenderer`).
//!
//! The glyphs come from `textures/font/ascii_sga.png`, which the GUI atlas
//! holds as `font/ascii_sga`; each glyph is a tinted image cut from its 8 px
//! cell. The text is wrapped to 86 pixels as `FontRenderer.drawSplitString`
//! does. Everything stays on the stack: the screen redraws every frame.

use umicraft_assets::{GuiAtlas, GuiRegion};
use umicraft_game::inventory::enchant_names::{row_names, EnchantName};
use umicraft_ui::screens::{enchant_row_available, Hotspot, ScreenLayout, ScreenState};
use umicraft_ui::{Rect, Ui};

/// The region of the galactic font in the GUI atlas.
const FONT_REGION: &str = "font/ascii_sga";
/// Window property of the name seed (`ContainerEnchantment.xpSeed`).
const SEED_PROPERTY: usize = 3;
/// Where the text of the first row starts, the distance between rows and the
/// wrap width (`j1 = i + 60 + 20`, `j + 16 + 19 * l`, `k1 = 86`).
const TEXT_X: f32 = 80.0;
const TEXT_TOP: f32 = 16.0;
const ROW_STEP: f32 = 19.0;
const WRAP_WIDTH: u32 = 86;
/// `FontRenderer.FONT_HEIGHT`.
const LINE_HEIGHT: f32 = 9.0;
/// The advance of a space (`getCharWidth(' ')`).
const SPACE_WIDTH: u32 = 4;
/// Room for four words of at most nine letters and their spaces.
const NAME_CAPACITY: usize = 48;
/// Colours of a row name: affordable, under the pointer, and the darker
/// `(6839882 & 16711422) >> 1` of a row the player cannot take.
const NAME_COLOR: u32 = 0x68_5E_4A;
const NAME_HOVER: u32 = 0xFF_FF_80;
const NAME_UNAVAILABLE: u32 = (NAME_COLOR & 0xFE_FE_FE) >> 1;

/// Draws the names of the rows that have a cost.
pub(crate) fn draw_enchant_names(
    ui: &mut Ui,
    layout: &ScreenLayout,
    gui_atlas: Option<&GuiAtlas>,
    state: &ScreenState<'_>,
) {
    let Some((region, widths)) = gui_atlas
        .and_then(|atlas| Some((atlas.region(FONT_REGION)?, atlas.font_widths(FONT_REGION)?)))
    else {
        return;
    };
    let seed = state.properties.get(SEED_PROPERTY).copied().unwrap_or(0);
    for (row, name) in (0..3_u8).zip(row_names(seed)) {
        let cost = state.properties.get(usize::from(row)).copied().unwrap_or(0);
        if cost <= 0 {
            continue;
        }
        let color = if !enchant_row_available(state, row) {
            NAME_UNAVAILABLE
        } else if state.hover == Some(Hotspot::Enchant(row)) {
            NAME_HOVER
        } else {
            NAME_COLOR
        };
        let top = TEXT_TOP + f32::from(row) * ROW_STEP;
        let (text, len) = spelled(&name);
        for (line, part) in wrapped(&text[..len], &widths).enumerate() {
            let y = top + line as f32 * LINE_HEIGHT;
            draw_line(ui, layout, (region, &widths), part, (TEXT_X, y), color);
        }
    }
}

/// The words of `name` joined by spaces, as ASCII bytes.
fn spelled(name: &EnchantName) -> ([u8; NAME_CAPACITY], usize) {
    let mut text = [0; NAME_CAPACITY];
    let mut len = 0;
    for (index, word) in name.words().enumerate() {
        let separator = if index > 0 { " " } else { "" };
        for &byte in separator.as_bytes().iter().chain(word.as_bytes()) {
            if let Some(slot) = text.get_mut(len) {
                *slot = byte;
                len += 1;
            }
        }
    }
    (text, len)
}

/// The advance of one glyph.
fn advance(byte: u8, widths: &[u8; 256]) -> u32 {
    if byte == b' ' {
        SPACE_WIDTH
    } else {
        u32::from(widths[usize::from(byte)])
    }
}

/// `FontRenderer.sizeStringToWidth`: how many bytes of `text` fit in the wrap
/// width, cut at the last space when the line has to break.
fn fitting(text: &[u8], widths: &[u8; 256]) -> usize {
    let mut width = 0;
    let mut last_space = None;
    let mut end = 0;
    while end < text.len() {
        let byte = text[end];
        if byte == b' ' {
            last_space = Some(end);
        }
        width += advance(byte, widths);
        if width > WRAP_WIDTH {
            break;
        }
        end += 1;
    }
    match last_space {
        Some(space) if end != text.len() && space < end => space,
        _ => end,
    }
}

/// The lines of `wrapFormattedStringToWidth`: a line break eats the space it
/// replaces.
fn wrapped<'a>(text: &'a [u8], widths: &'a [u8; 256]) -> impl Iterator<Item = &'a [u8]> + 'a {
    let mut rest = Some(text);
    std::iter::from_fn(move || {
        let current = rest?;
        let cut = fitting(current, widths).max(1);
        if current.len() <= cut {
            rest = None;
            return Some(current);
        }
        let skip = usize::from(current[cut] == b' ');
        rest = Some(&current[cut + skip..]);
        Some(&current[..cut])
    })
}

/// Draws one line of glyphs from `at` (GUI pixels inside the window), each
/// `width - 1` pixels wide as `renderDefaultChar` draws it.
fn draw_line(
    ui: &mut Ui,
    layout: &ScreenLayout,
    (region, widths): (&GuiRegion, &[u8; 256]),
    text: &[u8],
    at: (f32, f32),
    color: u32,
) {
    let scale = layout.scale as f32;
    let tint = rgb(color);
    let mut pen = 0;
    for &byte in text {
        let width = advance(byte, widths);
        if byte != b' ' {
            let cell = u32::from(byte);
            let glyph_width = width.saturating_sub(1);
            let src = region.sub_region(cell % 16 * 8, cell / 16 * 8, glyph_width, 8);
            let dst = Rect::new(
                layout.background.x + (at.0 + pen as f32) * scale,
                layout.background.y + at.1 * scale,
                glyph_width as f32 * scale,
                8.0 * scale,
            );
            ui.draw().image(src, dst, tint);
        }
        pen += width;
    }
}

/// `0xRRGGBB` as an opaque colour.
fn rgb(value: u32) -> [f32; 4] {
    let channel = |shift: u32| ((value >> shift) & 0xFF) as f32 / 255.0;
    [channel(16), channel(8), channel(0), 1.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every glyph six pixels wide, like a plain lower-case letter.
    const SIX: [u8; 256] = [6; 256];

    fn lines(text: &str) -> Vec<&str> {
        wrapped(text.as_bytes(), &SIX)
            .map(|line| std::str::from_utf8(line).expect("ascii"))
            .collect()
    }

    #[test]
    fn a_short_name_stays_on_one_line() {
        // 3 x 6 + 4 + 3 x 6 = 40 pixels.
        assert_eq!(lines("the air"), ["the air"]);
    }

    #[test]
    fn a_long_name_breaks_at_the_last_space_that_fits() {
        // "galvanize imbue " is 9 x 6 + 4 + 5 x 6 + 4 = 92 > 86.
        assert_eq!(lines("galvanize imbue cold"), ["galvanize", "imbue cold"]);
    }

    #[test]
    fn the_name_is_spelled_with_single_spaces() {
        let name = row_names(-16)[0];
        let (text, len) = spelled(&name);
        assert_eq!(&text[..len], b"elemental the water");
    }

    #[test]
    fn the_unavailable_colour_is_the_halved_name_colour() {
        assert_eq!(NAME_UNAVAILABLE, 0x34_2F_25);
    }
}

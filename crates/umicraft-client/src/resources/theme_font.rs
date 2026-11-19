// SPDX-License-Identifier: MIT OR Apache-2.0

//! The UI font of the active theme (ADR-0078).
//!
//! The vanilla theme keeps the font loaded by [`crate::resources::ui_font`]. The other
//! themes draw all text with the HD pixel font of the client pack, in the cell
//! size that stays crisp at the current GUI scale.

use thiserror::Error;
use umicraft_assets::client_pack;
use umicraft_ui::theme::{hd_cell_for_scale, FontSkin};
use umicraft_ui::{FontAtlas, FontError};

use crate::resources::ui_font::UnicodePages;

/// Which glyph grid the UI is currently drawn with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontChoice {
    /// The font of [`crate::resources::ui_font::load`].
    #[default]
    Game,
    /// The HD font with cells of this many texels.
    Hd(u32),
}

impl FontChoice {
    /// The choice for a theme's font skin at `gui_scale`.
    #[must_use]
    pub fn for_skin(skin: FontSkin, gui_scale: u32) -> Self {
        match skin {
            FontSkin::Game => Self::Game,
            FontSkin::Hd => Self::Hd(hd_cell_for_scale(gui_scale)),
        }
    }
}

/// Why an HD font could not be built.
#[derive(Debug, Error)]
pub enum HdFontError {
    /// The client pack has no grid of this cell size.
    #[error("the client pack has no HD font with {0} px cells")]
    Missing(u32),
    /// The embedded PNG is not a valid glyph grid.
    #[error(transparent)]
    Decode(#[from] FontError),
}

/// Builds the HD font atlas with cells of `cell` texels, extended with the
/// Unicode `pages` of the jar when there are some: the HD grid only covers the
/// 1.8 `ascii.png` characters, the rest comes from the vanilla pages.
///
/// # Errors
///
/// [`HdFontError::Missing`] for a size the pack does not ship and
/// [`HdFontError::Decode`] if an embedded file is not a glyph grid.
pub fn hd_atlas(cell: u32, pages: Option<&UnicodePages>) -> Result<FontAtlas, HdFontError> {
    let png = client_pack::hd_font_png(cell).ok_or(HdFontError::Missing(cell))?;
    let grid = FontAtlas::from_hd_png(png)?;
    Ok(pages.map_or(grid.clone(), |pages| pages.extend(&grid)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_ui::theme::HD_CELLS;

    #[test]
    fn the_game_skin_never_picks_the_hd_font() {
        assert_eq!(FontChoice::for_skin(FontSkin::Game, 4), FontChoice::Game);
    }

    #[test]
    fn the_hd_skin_picks_the_cell_of_the_scale() {
        assert_eq!(FontChoice::for_skin(FontSkin::Hd, 3), FontChoice::Hd(24));
    }

    #[test]
    fn every_bundled_size_builds_with_vanilla_metrics() {
        for cell in HD_CELLS {
            let atlas = hd_atlas(cell, None).expect("bundled font decodes");
            // 1.8 metrics, whatever the resolution: `A` is 5 px + 1 spacing.
            assert_eq!(atlas.measure("A", 1.0), 6.0, "cell {cell}");
            assert_eq!(atlas.measure(" ", 1.0), 4.0, "cell {cell}");
            let glyph = atlas.glyph('A').expect("A is in the grid");
            assert_eq!(glyph.scale, 8.0 / cell as f32);
        }
    }

    /// A jar's Unicode data with one drawn character, U+0416 (`Ж`), at the
    /// top-left cell of page 4 (`0x0400..0x04FF`), glyph columns 0 to 7.
    fn cyrillic_pages() -> UnicodePages {
        let mut sizes = vec![0u8; 0x1_0000];
        sizes[0x0416] = 0x07;
        let mut page = vec![0u8; 256 * 256];
        page[..8].fill(255);
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, 256, 256);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .and_then(|mut writer| writer.write_image_data(&page))
            .expect("test page encodes");
        UnicodePages::from_entries(vec![
            ("assets/minecraft/font/glyph_sizes.bin".to_string(), sizes),
            (
                "assets/minecraft/textures/font/unicode_page_04.png".to_string(),
                bytes,
            ),
        ])
        .expect("sizes are present")
    }

    #[test]
    fn the_hd_font_draws_unicode_characters_from_the_jar_pages() {
        // Regression: the HD grid alone turned every character outside the
        // 1.8 grid into `?` under the Ember and Amethyst themes.
        let pages = cyrillic_pages();
        for cell in HD_CELLS {
            let plain = hd_atlas(cell, None).expect("bundled font decodes");
            assert!(plain.glyph('\u{416}').is_none(), "cell {cell}");
            let extended = hd_atlas(cell, Some(&pages)).expect("bundled font decodes");
            assert!(extended.is_unicode('\u{416}'), "cell {cell}");
            // The grid keeps its metrics next to the pages.
            assert_eq!(extended.measure("A", 1.0), 6.0, "cell {cell}");
        }
    }

    #[test]
    fn pages_without_the_sizes_file_are_unusable() {
        assert!(UnicodePages::from_entries(Vec::new()).is_none());
    }

    #[test]
    fn an_unknown_cell_size_is_reported() {
        assert!(matches!(hd_atlas(20, None), Err(HdFontError::Missing(20))));
    }
}

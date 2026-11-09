// SPDX-License-Identifier: MIT OR Apache-2.0

//! The icon set of Umicraft's own screens.
//!
//! The icons are Phosphor Icons (regular weight, MIT: see
//! `assets/icons/LICENSE-phosphor.txt`), rasterized once by
//! `docs/tools/gen_icons.py` into `assets/icons.r8`: one [`SIDE`] x [`SIDE`]
//! coverage bitmap per icon, in the order of [`Icon::ALL`]. [`rasterize`] hands
//! them to the brand atlas, under private use characters, so an icon is drawn,
//! tinted, scaled and clipped like a letter (see
//! [`draw_icon`](crate::modern::draw_icon)) and the renderer learns nothing new.

/// Side of a stored bitmap, in texels: the em of the brand atlas.
const SIDE: u32 = 48;
/// The bitmaps, one after the other.
static BITMAPS: &[u8] = include_bytes!("../assets/icons.r8");
/// The first private use character; icon number `n` is `FIRST + n`.
const FIRST: u32 = 0xE000;

/// An icon of the set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// Four squares: the mods themselves.
    Grid,
    /// Two sliders: settings.
    Sliders,
    /// A magnifier.
    Search,
    /// A cross.
    Close,
    /// A gauge: frames per second.
    Gauge,
    /// A reticle: position and direction.
    Target,
    /// A pointer arrow: clicks per second.
    Cursor,
    /// Signal bars: latency.
    Signal,
    /// A running figure: sprinting.
    Run,
    /// Three speed lines: motion blur.
    Blur,
    /// A cogwheel: the settings of a mod.
    Gear,
    /// A light bulb: full brightness.
    Lightbulb,
}

impl Icon {
    /// Every icon, in the order of their bitmaps and characters.
    pub const ALL: [Icon; 12] = [
        Icon::Grid,
        Icon::Sliders,
        Icon::Search,
        Icon::Close,
        Icon::Gauge,
        Icon::Target,
        Icon::Cursor,
        Icon::Signal,
        Icon::Run,
        Icon::Blur,
        Icon::Gear,
        Icon::Lightbulb,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|&icon| icon == self).unwrap_or(0)
    }

    /// The private use character the brand atlas draws this icon under.
    #[must_use]
    pub fn character(self) -> char {
        char::from_u32(FIRST + self.index() as u32).unwrap_or('\u{fffd}')
    }
}

/// The coverage bitmap of `icon`, `size` x `size` texels. At the stored size it
/// is the file's; at another it is resampled bilinearly.
#[must_use]
pub fn rasterize(icon: Icon, size: u32) -> Vec<u8> {
    let area = (SIDE * SIDE) as usize;
    let source = BITMAPS
        .get(icon.index() * area..(icon.index() + 1) * area)
        .unwrap_or(&[]);
    if source.len() != area {
        return vec![0; (size * size) as usize];
    }
    if size == SIDE {
        return source.to_vec();
    }
    let texel = |x: i64, y: i64| {
        let (x, y) = (x.clamp(0, SIDE as i64 - 1), y.clamp(0, SIDE as i64 - 1));
        f32::from(source[(y as u32 * SIDE + x as u32) as usize])
    };
    let ratio = SIDE as f32 / size as f32;
    let mut out = Vec::with_capacity((size * size) as usize);
    for ty in 0..size {
        for tx in 0..size {
            let (sx, sy) = (
                (tx as f32 + 0.5) * ratio - 0.5,
                (ty as f32 + 0.5) * ratio - 0.5,
            );
            let (x0, y0) = (sx.floor(), sy.floor());
            let (fx, fy) = (sx - x0, sy - y0);
            let (x0, y0) = (x0 as i64, y0 as i64);
            let top = texel(x0, y0) * (1.0 - fx) + texel(x0 + 1, y0) * fx;
            let bottom = texel(x0, y0 + 1) * (1.0 - fx) + texel(x0 + 1, y0 + 1) * fx;
            out.push((top * (1.0 - fy) + bottom * fy).round() as u8);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_is_one_bitmap_per_icon() {
        assert_eq!(BITMAPS.len(), Icon::ALL.len() * (SIDE * SIDE) as usize);
    }

    #[test]
    fn every_icon_has_its_own_private_use_character() {
        let mut seen = std::collections::HashSet::new();
        for icon in Icon::ALL {
            let character = icon.character();
            assert!(('\u{e000}'..='\u{f8ff}').contains(&character));
            assert!(seen.insert(character), "{icon:?} shares its character");
        }
    }

    #[test]
    fn every_icon_draws_ink_and_stays_off_the_edge() {
        for icon in Icon::ALL {
            let coverage = rasterize(icon, SIDE);
            let ink = coverage.iter().filter(|&&c| c > 128).count();
            assert!(ink > 60, "{icon:?} is empty ({ink})");
            assert!(ink < (SIDE * SIDE / 2) as usize, "{icon:?} is a blob");
            for edge in 0..SIDE {
                for (x, y) in [(edge, 0), (edge, SIDE - 1), (0, edge), (SIDE - 1, edge)] {
                    assert_eq!(
                        coverage[(y * SIDE + x) as usize],
                        0,
                        "{icon:?} touches the edge"
                    );
                }
            }
        }
    }

    #[test]
    fn a_stroke_is_anti_aliased() {
        let coverage = rasterize(Icon::Close, SIDE);
        assert!(coverage.iter().any(|&c| c > 0 && c < 255));
    }

    #[test]
    fn another_size_keeps_the_ink() {
        let small = rasterize(Icon::Gauge, 24);
        assert_eq!(small.len(), 24 * 24);
        assert!(small.iter().filter(|&&c| c > 128).count() > 15);
    }
}

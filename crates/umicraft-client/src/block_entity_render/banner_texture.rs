// SPDX-License-Identifier: MIT OR Apache-2.0

//! Which banner a block entity shows and the texture that makes it.
//!
//! A [`BannerDesign`] is the base dye and the patterns of the NBT of a
//! banner (MCP `TileEntityBanner`). [`compose`] draws it on the cloth sheet the
//! way `LayeredColorMaskTexture` does: every layer is a grayscale mask whose
//! red channel is the coverage, painted with the layer's dye multiplied onto
//! the sheet. The masks are read from the user's own client jar by the caller;
//! none is stored in the repository.

use std::hash::Hasher;

use umicraft_game::nbt::Nbt;
use umicraft_model::entity::TextureSlot;

use super::banner_patterns::{pattern, pattern_index, DYE_RGB};

/// Most layers of a banner: the base and sixteen patterns
/// (`j < 17` in `LayeredColorMaskTexture.loadTexture`).
const MAX_LAYERS: usize = 17;
/// Dye damage of white, the colour of a banner whose data cannot be read.
const WHITE: u8 = 15;

/// The base sheet's path under `textures/entity/`.
pub(super) const SHEET: &str = "banner_base";

/// The mask path of a pattern under `textures/entity/`.
fn mask_path(name: &str) -> String {
    format!("banner/{name}")
}

/// A decoded RGBA image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Image {
    /// Row-major RGBA bytes, `width * height * 4` of them.
    pub rgba: Vec<u8>,
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
}

/// The look of a banner: its base dye and the patterns drawn over it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BannerDesign {
    /// Dye damage (`0` black .. `15` white) of the cloth.
    base: u8,
    /// Index into the pattern table and the dye damage, bottom layer first.
    layers: Vec<(usize, u8)>,
}

/// A dye damage as `EnumDyeColor.byDyeDamage` reads it: out of range is `0`.
fn dye(value: Option<&Nbt>) -> u8 {
    value
        .and_then(Nbt::as_int)
        .and_then(|damage| u8::try_from(damage).ok())
        .filter(|damage| *damage < 16)
        .unwrap_or(0)
}

impl BannerDesign {
    /// A banner without patterns.
    #[must_use]
    pub(crate) const fn plain(base: u8) -> Self {
        Self {
            base,
            layers: Vec::new(),
        }
    }

    /// The design of the NBT of an Update Block Entity packet.
    ///
    /// Missing or malformed data gives a plain white banner. A compound
    /// without `Base` reads as `0`, black, as `getInteger` does; unknown
    /// patterns are skipped, as `initializeBannerData` does.
    #[must_use]
    pub(crate) fn from_nbt(bytes: Option<&[u8]>) -> Self {
        let Some(root) = bytes.and_then(|bytes| Nbt::parse(bytes).ok()) else {
            return Self::plain(WHITE);
        };
        Self::from_tree(&root, dye(root.get("Base")))
    }

    /// The design of a banner item (`TileEntityBanner.setItemValues`).
    ///
    /// The patterns and the base dye live in `BlockEntityTag`; without a
    /// `Base` there, the base dye is the item damage (`metadata & 15`).
    /// Unreadable NBT is a banner without a tag.
    #[must_use]
    pub(crate) fn from_item(nbt: Option<&[u8]>, damage: i16) -> Self {
        let from_damage = (damage & 15) as u8;
        let root = nbt.and_then(|bytes| Nbt::parse(bytes).ok());
        match root.as_ref().and_then(|root| root.get("BlockEntityTag")) {
            Some(tag) => {
                Self::from_tree(tag, tag.get("Base").map_or(from_damage, |b| dye(Some(b))))
            }
            None => Self::plain(from_damage),
        }
    }

    /// The patterns of `tag` over the dye `base`.
    fn from_tree(tag: &Nbt, base: u8) -> Self {
        let mut design = Self::plain(base);
        let entries = tag.get("Patterns").and_then(Nbt::as_list).unwrap_or(&[]);
        for entry in entries.iter().take(MAX_LAYERS - 1) {
            let index = entry
                .get("Pattern")
                .and_then(Nbt::as_str)
                .and_then(pattern_index);
            if let Some(index) = index {
                design.layers.push((index, dye(entry.get("Color"))));
            }
        }
        design
    }

    /// The texture slot of the design.
    ///
    /// It is the FNV-1a hash of the cache key of the reference
    /// (`func_175116_e`: `b`, the base, then each pattern id and dye), so equal
    /// designs share a texture. Two designs colliding in 64 bits would share
    /// one texture; that is accepted for a purely cosmetic cache.
    #[must_use]
    pub(crate) fn slot(&self) -> TextureSlot {
        let mut hash = Fnv1a(0xcbf2_9ce4_8422_2325);
        hash.write(format!("b{}", self.base).as_bytes());
        for (index, color) in &self.layers {
            hash.write(format!("{}{color}", pattern(*index).0).as_bytes());
        }
        TextureSlot::Banner(hash.finish())
    }
}

/// The 64-bit FNV-1a hash, which is stable across runs and platforms.
struct Fnv1a(u64);

impl Hasher for Fnv1a {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
}

/// `MathHelper.func_180188_d`: a texel channel times a dye channel / 255.
fn multiply(texel: u8, dye: u8) -> u8 {
    (f32::from(texel) * f32::from(dye) / 255.0) as u8
}

/// Paints the straight-alpha `source` over `destination` (`drawImage`).
fn paint_over(destination: &mut [u8], source: [u8; 4]) {
    let source_alpha = u32::from(source[3]);
    let destination_alpha = u32::from(destination[3]);
    let weight = destination_alpha * (255 - source_alpha);
    let total = source_alpha * 255 + weight;
    if total == 0 {
        return;
    }
    for channel in 0..3 {
        let mixed = u32::from(source[channel]) * source_alpha * 255
            + u32::from(destination[channel]) * weight;
        destination[channel] = (mixed / total) as u8;
    }
    destination[3] = (total / 255) as u8;
}

/// Draws one mask over `image`: where the mask is not transparent, its red
/// channel becomes the alpha of the texel of `sheet` multiplied by the dye.
/// A mask of another size is ignored, as in the reference.
fn paint_layer(image: &mut Image, sheet: &Image, mask: &Image, color: u8) {
    if (mask.width, mask.height) != (image.width, image.height) {
        return;
    }
    let tint = DYE_RGB[usize::from(color & 15)];
    let texels = image
        .rgba
        .chunks_exact_mut(4)
        .zip(sheet.rgba.chunks_exact(4))
        .zip(mask.rgba.chunks_exact(4));
    for ((target, cloth), coverage) in texels {
        if coverage[3] == 0 {
            continue;
        }
        let source = [
            multiply(cloth[0], tint[0]),
            multiply(cloth[1], tint[1]),
            multiply(cloth[2], tint[2]),
            coverage[0],
        ];
        paint_over(target, source);
    }
}

/// The texture of `design`.
///
/// `read` returns the decoded PNG at a path under `textures/entity/`
/// (`banner_base`, `banner/<pattern>`). Without the sheet the banner is the
/// flat colour of its base dye, 64x64; a missing mask leaves its layer out.
#[must_use]
pub(crate) fn compose(design: &BannerDesign, read: impl Fn(&str) -> Option<Image>) -> Image {
    let Some(sheet) = read(SHEET) else {
        return flat(design.base);
    };
    let mut image = sheet.clone();
    let layers = std::iter::once((0, design.base)).chain(design.layers.iter().copied());
    for (index, color) in layers {
        if let Some(mask) = read(&mask_path(pattern(index).1)) {
            paint_layer(&mut image, &sheet, &mask, color);
        }
    }
    image
}

/// A 64x64 opaque image of the colour of a dye.
fn flat(base: u8) -> Image {
    let [red, green, blue] = DYE_RGB[usize::from(base & 15)];
    Image {
        rgba: [red, green, blue, 255].repeat(64 * 64),
        width: 64,
        height: 64,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// A 2x1 image from two texels.
    fn image(first: [u8; 4], second: [u8; 4]) -> Image {
        Image {
            rgba: [first, second].concat(),
            width: 2,
            height: 1,
        }
    }

    fn files(entries: &[(&str, Image)]) -> impl Fn(&str) -> Option<Image> {
        let map: HashMap<String, Image> = entries
            .iter()
            .map(|(path, image)| ((*path).to_owned(), image.clone()))
            .collect();
        move |path| map.get(path).cloned()
    }

    fn compound(base: Option<i32>, patterns: &[(&str, i32)]) -> Vec<u8> {
        let mut out = vec![10, 0, 0];
        if let Some(base) = base {
            out.extend([3, 0, 4]);
            out.extend(b"Base");
            out.extend(base.to_be_bytes());
        }
        out.extend([9, 0, 8]);
        out.extend(b"Patterns");
        out.push(10);
        out.extend((patterns.len() as i32).to_be_bytes());
        for (id, color) in patterns {
            out.extend([3, 0, 5]);
            out.extend(b"Color");
            out.extend(color.to_be_bytes());
            out.extend([8, 0, 7]);
            out.extend(b"Pattern");
            out.extend((id.len() as u16).to_be_bytes());
            out.extend(id.as_bytes());
            out.push(0);
        }
        out.push(0);
        out
    }

    #[test]
    fn the_nbt_gives_the_base_and_the_known_patterns_in_order() {
        let bytes = compound(Some(1), &[("bs", 4), ("nope", 2), ("moj", 15)]);
        let design = BannerDesign::from_nbt(Some(&bytes));
        assert_eq!(design.base, 1);
        assert_eq!(design.layers, [(5, 4), (38, 15)]);
    }

    #[test]
    fn missing_or_malformed_data_is_a_white_banner() {
        assert_eq!(BannerDesign::from_nbt(None), BannerDesign::plain(WHITE));
        assert_eq!(
            BannerDesign::from_nbt(Some(&[10, 0])),
            BannerDesign::plain(WHITE)
        );
        // No `Base` reads as 0 (black), an out-of-range dye as 0.
        let bytes = compound(None, &[("bs", 99)]);
        let design = BannerDesign::from_nbt(Some(&bytes));
        assert_eq!((design.base, design.layers), (0, vec![(5, 0)]));
    }

    #[test]
    fn at_most_sixteen_patterns_are_read() {
        let many = vec![("bs", 1); 30];
        let design = BannerDesign::from_nbt(Some(&compound(Some(0), &many)));
        assert_eq!(design.layers.len(), 16);
    }

    #[test]
    fn equal_designs_share_a_slot_and_different_ones_do_not() {
        let a = BannerDesign::from_nbt(Some(&compound(Some(3), &[("bs", 4)])));
        let b = BannerDesign::from_nbt(Some(&compound(Some(3), &[("bs", 4)])));
        let c = BannerDesign::from_nbt(Some(&compound(Some(3), &[("bs", 5)])));
        assert_eq!(a.slot(), b.slot());
        assert_ne!(a.slot(), c.slot());
    }

    #[test]
    fn the_pattern_table_matches_the_reference_ids() {
        assert_eq!(pattern_index("b"), Some(0));
        assert_eq!(pattern_index("flo"), Some(37));
        assert_eq!(pattern_index("moj"), Some(38));
        assert_eq!(pattern(38), ("moj", "mojang"));
        assert_eq!(pattern_index("zz"), None);
    }

    #[test]
    fn the_base_mask_tints_the_sheet_with_the_dye() {
        let sheet = image([255, 128, 0, 255], [10, 10, 10, 255]);
        // Covers the first texel only; the second has no alpha.
        let base = image([255, 0, 0, 255], [255, 0, 0, 0]);
        let read = files(&[(SHEET, sheet), ("banner/base", base)]);
        // Dye damage 1 is red (0x99, 0x33, 0x33).
        let out = compose(&BannerDesign::plain(1), read);
        assert_eq!(out.rgba[..4], [153, 25, 0, 255]);
        // Untouched where the mask is transparent.
        assert_eq!(out.rgba[4..], [10, 10, 10, 255]);
    }

    #[test]
    fn a_pattern_blends_by_the_red_channel_of_its_mask() {
        let sheet = image([255, 255, 255, 255], [255, 255, 255, 255]);
        let base = image([255, 0, 0, 255], [255, 0, 0, 255]);
        let stripe = image([255, 9, 9, 255], [128, 9, 9, 255]);
        let read = files(&[
            (SHEET, sheet),
            ("banner/base", base),
            ("banner/stripe_bottom", stripe),
        ]);
        let mut design = BannerDesign::plain(15);
        design.layers.push((5, 0));
        let out = compose(&design, read);
        // Full coverage: the dye. Half coverage: halfway to it from white.
        assert_eq!(out.rgba[..4], [25, 25, 25, 255]);
        let blended = out.rgba[4];
        assert!((i32::from(blended) - 140).abs() <= 1, "{blended}");
        assert_eq!(out.rgba[7], 255);
    }

    #[test]
    fn a_mask_of_another_size_or_a_missing_one_is_skipped() {
        let sheet = image([200, 200, 200, 255], [200, 200, 200, 255]);
        let wrong = Image {
            rgba: vec![255; 4],
            width: 1,
            height: 1,
        };
        let read = files(&[(SHEET, sheet.clone()), ("banner/base", wrong)]);
        assert_eq!(compose(&BannerDesign::plain(1), read), sheet);
        let read = files(&[(SHEET, sheet.clone())]);
        assert_eq!(compose(&BannerDesign::plain(1), read), sheet);
    }

    #[test]
    fn without_the_sheet_the_banner_is_flat_in_its_base_colour() {
        let out = compose(&BannerDesign::plain(15), |_: &str| None);
        assert_eq!((out.width, out.height), (64, 64));
        assert_eq!(out.rgba[..4], [255, 255, 255, 255]);
        assert_eq!(out.rgba.len(), 64 * 64 * 4);
    }

    #[test]
    fn painting_over_a_transparent_texel_keeps_the_colour_and_alpha() {
        let mut texel = [0, 0, 0, 0];
        paint_over(&mut texel, [10, 20, 30, 100]);
        assert_eq!(texel, [10, 20, 30, 100]);
    }
}

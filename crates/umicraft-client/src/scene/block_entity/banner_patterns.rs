// SPDX-License-Identifier: MIT OR Apache-2.0

//! The data tables of banners: the dye colours and the patterns (MCP
//! `EnumDyeColor`, `TileEntityBanner.EnumBannerPattern`).

/// `MapColor.colorValue` of the dyes by dye damage (`EnumDyeColor.byDyeDamage`):
/// black, red, green, brown, blue, purple, cyan, silver, gray, pink, lime,
/// yellow, light blue, magenta, orange, white.
pub(super) const DYE_RGB: [[u8; 3]; 16] = [
    [0x19, 0x19, 0x19],
    [0x99, 0x33, 0x33],
    [0x66, 0x7F, 0x33],
    [0x66, 0x4C, 0x33],
    [0x33, 0x4C, 0xB2],
    [0x7F, 0x3F, 0xB2],
    [0x4C, 0x7F, 0x99],
    [0x99, 0x99, 0x99],
    [0x4C, 0x4C, 0x4C],
    [0xF2, 0x7F, 0xA5],
    [0x7F, 0xCC, 0x19],
    [0xE5, 0xE5, 0x33],
    [0x66, 0x99, 0xD8],
    [0xB2, 0x4C, 0xD8],
    [0xD8, 0x7F, 0x33],
    [0xFF, 0xFF, 0xFF],
];

/// `EnumBannerPattern`: the id of the NBT and the name of the mask file.
pub(super) const PATTERNS: [(&str, &str); 39] = [
    ("b", "base"),
    ("bl", "square_bottom_left"),
    ("br", "square_bottom_right"),
    ("tl", "square_top_left"),
    ("tr", "square_top_right"),
    ("bs", "stripe_bottom"),
    ("ts", "stripe_top"),
    ("ls", "stripe_left"),
    ("rs", "stripe_right"),
    ("cs", "stripe_center"),
    ("ms", "stripe_middle"),
    ("drs", "stripe_downright"),
    ("dls", "stripe_downleft"),
    ("ss", "small_stripes"),
    ("cr", "cross"),
    ("sc", "straight_cross"),
    ("bt", "triangle_bottom"),
    ("tt", "triangle_top"),
    ("bts", "triangles_bottom"),
    ("tts", "triangles_top"),
    ("ld", "diagonal_left"),
    ("rd", "diagonal_up_right"),
    ("lud", "diagonal_up_left"),
    ("rud", "diagonal_right"),
    ("mc", "circle"),
    ("mr", "rhombus"),
    ("vh", "half_vertical"),
    ("hh", "half_horizontal"),
    ("vhr", "half_vertical_right"),
    ("hhb", "half_horizontal_bottom"),
    ("bo", "border"),
    ("cbo", "curly_border"),
    ("cre", "creeper"),
    ("gra", "gradient"),
    ("gru", "gradient_up"),
    ("bri", "bricks"),
    ("sku", "skull"),
    ("flo", "flower"),
    ("moj", "mojang"),
];

/// The pattern with the given id of the NBT, `None` for an unknown one
/// (`EnumBannerPattern.getPatternByID`).
pub(super) fn pattern_index(id: &str) -> Option<usize> {
    PATTERNS.iter().position(|(known, _)| *known == id)
}

/// The id and mask name of the pattern at `index`.
pub(super) fn pattern(index: usize) -> (&'static str, &'static str) {
    PATTERNS.get(index).copied().unwrap_or(PATTERNS[0])
}

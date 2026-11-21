// SPDX-License-Identifier: MIT OR Apache-2.0

//! `color.properties`: the fixed colours and the block palettes.

use crate::{Properties, PropertiesError};

use super::resolve_path;

/// A colour as red, green, blue bytes.
pub type Rgb = [u8; 3];

/// The text colour codes of `text.code.N`: the 16 chat colours and their 16
/// shadow colours.
pub const TEXT_CODES: usize = 32;

/// What `color.properties` sets.
///
/// Every field is `None` when the key is absent or its value is not a
/// hexadecimal colour (OptiFine logs and ignores such a value).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColorProperties {
    /// `palette.format`: the default format of colormaps without a
    /// properties file of their own. `None` when absent or unknown.
    pub palette_format: Option<String>,
    /// `particle.water` or `drop.water`: multiplies the water particles.
    pub particle_water: Option<Rgb>,
    /// `particle.portal`: the portal particles.
    pub particle_portal: Option<Rgb>,
    /// `lilypad`: the lily pad tint.
    pub lilypad: Option<Rgb>,
    /// `text.xpbar`: the experience level number.
    pub text_xpbar: Option<Rgb>,
    /// `text.boss`: the boss health bar title.
    pub text_boss: Option<Rgb>,
    /// `text.sign`: the text on signs.
    pub text_sign: Option<Rgb>,
    /// `fog.nether`: the Nether fog.
    pub fog_nether: Option<Rgb>,
    /// `fog.end`: the End fog.
    pub fog_end: Option<Rgb>,
    /// `sky.end`: the End sky colour, which the fog blends toward.
    pub sky_end: Option<Rgb>,
    /// `text.code.N`: replacements of the 32 text colours.
    pub text_codes: [Option<Rgb>; TEXT_CODES],
    /// `palette.block.<picture>=<blocks>`: `(picture path relative to the
    /// feature root, the blocks text)`, in key order.
    pub palette_blocks: Vec<(String, String)>,
}

impl ColorProperties {
    /// Parses `bytes`.
    ///
    /// # Errors
    ///
    /// [`PropertiesError`] when the file's escapes are broken.
    ///
    /// # Example
    ///
    /// ```
    /// use umicraft_packext::colors::ColorProperties;
    ///
    /// let colors = ColorProperties::parse(b"fog.nether=330808\ntext.code.1 = 0000aa")?;
    /// assert_eq!(colors.fog_nether, Some([0x33, 0x08, 0x08]));
    /// assert_eq!(colors.text_codes[1], Some([0, 0, 0xaa]));
    /// # Ok::<(), umicraft_packext::PropertiesError>(())
    /// ```
    pub fn parse(bytes: &[u8]) -> Result<Self, PropertiesError> {
        let props = Properties::parse(bytes)?;
        let color = |key: &str| props.get(key).and_then(parse_color);
        let mut parsed = Self {
            palette_format: props
                .get("palette.format")
                .filter(|format| matches!(*format, "vanilla" | "grid" | "fixed"))
                .map(str::to_string),
            particle_water: color("particle.water").or_else(|| color("drop.water")),
            particle_portal: color("particle.portal"),
            lilypad: color("lilypad"),
            text_xpbar: color("text.xpbar"),
            text_boss: color("text.boss"),
            text_sign: color("text.sign"),
            fog_nether: color("fog.nether"),
            fog_end: color("fog.end"),
            sky_end: color("sky.end"),
            ..Self::default()
        };
        for key in props.keys() {
            let value = props.get(key).unwrap_or_default();
            if let Some(index) = key.strip_prefix("text.code.") {
                let slot = index
                    .trim()
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| parsed.text_codes.get_mut(index));
                if let (Some(slot), Some(rgb)) = (slot, parse_color(value)) {
                    *slot = Some(rgb);
                }
            } else if let Some(path) = key.strip_prefix("palette.block.") {
                parsed
                    .palette_blocks
                    .push((resolve_path(path, ""), value.to_string()));
            }
        }
        Ok(parsed)
    }
}

/// A hexadecimal colour, the way OptiFine's `parseColor` reads it:
/// `Integer.parseInt(text, 16) & 0xFFFFFF`. A value that does not parse, or
/// that is negative, gives `None`.
pub(super) fn parse_color(text: &str) -> Option<Rgb> {
    let value = i64::from_str_radix(text.trim(), 16).ok()?;
    if !(0..=i64::from(i32::MAX)).contains(&value) {
        return None;
    }
    Some([(value >> 16) as u8, (value >> 8) as u8, value as u8])
}

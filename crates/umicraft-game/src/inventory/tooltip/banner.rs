// SPDX-License-Identifier: MIT OR Apache-2.0

//! The banner parts of a tooltip (MCP `ItemBanner`).
//!
//! A banner is named after its base colour, which the `BlockEntityTag.Base`
//! tag overrides over the damage value (`getBaseColor`). Its pattern list
//! (`BlockEntityTag.Patterns`) adds one gray line per layer, at most six
//! (`addInformation`), such as "Red Stripe" or "Cyan Border".

use super::{TooltipColor, TooltipLine, TooltipText};
use crate::items::dye_colour;
use crate::nbt::Nbt;

/// Registry name of the banner item.
pub(super) const BANNER: &str = "banner";
/// Most pattern lines shown (`i < 6` in `ItemBanner.addInformation`).
const MAX_PATTERN_LINES: usize = 6;

/// `TileEntityBanner.EnumBannerPattern`: `(pattern id, pattern name)`.
const PATTERNS: &[(&str, &str)] = &[
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

fn entity_tag(root: Option<&Nbt>) -> Option<&Nbt> {
    root?.get("BlockEntityTag")
}

/// The unlocalised name of a banner (`item.banner.<colour>`), from the
/// `Base` tag when present and the damage value otherwise.
pub(super) fn unlocalized_name(damage: i16, root: Option<&Nbt>) -> String {
    let base = entity_tag(root)
        .and_then(|tag| tag.get("Base"))
        .and_then(Nbt::as_int)
        .and_then(|base| i16::try_from(base).ok())
        .unwrap_or(damage);
    format!("item.banner.{}", dye_colour(base))
}

/// One line per pattern layer; unknown pattern ids are skipped like the
/// reference does.
pub(super) fn lines(root: Option<&Nbt>) -> Vec<TooltipLine> {
    let layers = entity_tag(root)
        .and_then(|tag| tag.get("Patterns"))
        .and_then(Nbt::as_list)
        .unwrap_or_default();
    layers
        .iter()
        .take(MAX_PATTERN_LINES)
        .filter_map(layer_line)
        .collect()
}

fn layer_line(layer: &Nbt) -> Option<TooltipLine> {
    let id = layer.get("Pattern")?.as_str()?;
    let (_, name) = PATTERNS.iter().find(|(pattern, _)| *pattern == id)?;
    let colour = layer.get("Color").and_then(Nbt::as_int).unwrap_or(0);
    let colour = dye_colour(i16::try_from(colour).unwrap_or(0));
    Some(TooltipLine::plain(
        TooltipText::key(format!("item.banner.{name}.{colour}")),
        TooltipColor::Gray,
    ))
}

#[cfg(test)]
mod tests {
    use umicraft_protocol::packets::play::ItemStack;

    use super::super::tooltip;
    use super::*;

    const BANNER: i16 = 425;

    fn named(out: &mut Vec<u8>, tag: u8, name: &str) {
        out.push(tag);
        out.extend((name.len() as u16).to_be_bytes());
        out.extend(name.as_bytes());
    }

    /// `{BlockEntityTag: {Base: base, Patterns: [{Pattern, Color}, ...]}}`.
    fn banner_tag(base: Option<i32>, patterns: &[(&str, i32)]) -> Vec<u8> {
        let mut nbt = vec![10, 0, 0];
        named(&mut nbt, 10, "BlockEntityTag");
        if let Some(base) = base {
            named(&mut nbt, 3, "Base");
            nbt.extend(base.to_be_bytes());
        }
        named(&mut nbt, 9, "Patterns");
        nbt.push(10);
        nbt.extend((patterns.len() as i32).to_be_bytes());
        for (pattern, colour) in patterns {
            named(&mut nbt, 8, "Pattern");
            nbt.extend((pattern.len() as u16).to_be_bytes());
            nbt.extend(pattern.as_bytes());
            named(&mut nbt, 3, "Color");
            nbt.extend(colour.to_be_bytes());
            nbt.push(0);
        }
        nbt.extend([0, 0]);
        nbt
    }

    fn banner(damage: i16, nbt: Vec<u8>) -> ItemStack {
        ItemStack {
            item_id: BANNER,
            stack_size: 1,
            damage,
            nbt: Some(nbt),
        }
    }

    #[test]
    fn the_base_tag_names_the_banner_over_the_damage() {
        let lines = tooltip(&banner(0, banner_tag(Some(1), &[])));
        assert_eq!(lines[0].text, TooltipText::key("item.banner.red.name"));
        let plain = tooltip(&banner(15, banner_tag(None, &[])));
        assert_eq!(plain[0].text, TooltipText::key("item.banner.white.name"));
    }

    #[test]
    fn each_pattern_layer_is_a_gray_line() {
        let lines = tooltip(&banner(0, banner_tag(None, &[("bs", 1), ("bo", 6)])));
        assert_eq!(lines.len(), 3);
        assert_eq!(
            lines[1].text,
            TooltipText::key("item.banner.stripe_bottom.red")
        );
        assert_eq!(lines[2].text, TooltipText::key("item.banner.border.cyan"));
        assert_eq!(lines[1].color, TooltipColor::Gray);
    }

    #[test]
    fn unknown_patterns_are_skipped_and_at_most_six_layers_show() {
        let many: Vec<(&str, i32)> = std::iter::repeat_n(("ss", 0), 8).collect();
        assert_eq!(lines(Root::of(&banner_tag(None, &many)).as_ref()).len(), 6);
        let skipped = [("zzz", 0), ("cr", 0)];
        assert_eq!(
            lines(Root::of(&banner_tag(None, &skipped)).as_ref()).len(),
            1
        );
    }

    /// Parses a tag for the tests that call [`lines`] directly.
    struct Root;

    impl Root {
        fn of(bytes: &[u8]) -> Option<Nbt> {
            Nbt::parse(bytes).ok()
        }
    }
}

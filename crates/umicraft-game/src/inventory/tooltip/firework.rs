// SPDX-License-Identifier: MIT OR Apache-2.0

//! Firework rockets and stars in a tooltip (MCP `ItemFirework.addInformation`,
//! `ItemFireworkCharge.addInformation` and `addExplosionInfo`).

use super::{TooltipColor, TooltipLine, TooltipText};
use crate::nbt::Nbt;

/// Registry names of the rocket and the star.
pub(super) const ROCKET: &str = "fireworks";
pub(super) const STAR: &str = "firework_charge";

/// `ItemDye.dyeColors` with the `EnumDyeColor.getUnlocalizedName` of each
/// dye damage (black first).
const DYES: [(i32, &str); 16] = [
    (1_973_019, "black"),
    (11_743_532, "red"),
    (3_887_386, "green"),
    (5_320_730, "brown"),
    (2_437_522, "blue"),
    (8_073_150, "purple"),
    (2_651_799, "cyan"),
    (11_250_603, "silver"),
    (4_408_131, "gray"),
    (14_188_952, "pink"),
    (4_312_372, "lime"),
    (14_602_026, "yellow"),
    (6_719_955, "lightBlue"),
    (12_801_229, "magenta"),
    (15_435_844, "orange"),
    (15_790_320, "white"),
];

/// The rocket's flight duration, then each star's lines, the ones after a
/// star's first indented by two spaces.
pub(super) fn rocket_lines(root: Option<&Nbt>) -> Vec<TooltipLine> {
    let Some(fireworks) = root.and_then(|tag| tag.get("Fireworks")) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    if let Some(flight) = fireworks.get("Flight").and_then(Nbt::as_int) {
        lines.push(gray(TooltipText::Concat(vec![
            TooltipText::key("item.fireworks.flight"),
            TooltipText::literal(format!(" {}", flight as i8)),
        ])));
    }
    for explosion in fireworks
        .get("Explosions")
        .and_then(Nbt::as_list)
        .unwrap_or_default()
    {
        for (index, mut line) in explosion_lines(explosion).into_iter().enumerate() {
            if index > 0 {
                line.text = TooltipText::Concat(vec![TooltipText::literal("  "), line.text]);
            }
            lines.push(line);
        }
    }
    lines
}

/// A star's lines, from its `Explosion` compound.
pub(super) fn star_lines(root: Option<&Nbt>) -> Vec<TooltipLine> {
    root.and_then(|tag| tag.get("Explosion"))
        .map(explosion_lines)
        .unwrap_or_default()
}

/// `addExplosionInfo`: the shape, the colours, the fade colours, then trail
/// and twinkle. The reference trims the shape name; the 1.8 language files
/// have no spaces around it.
fn explosion_lines(explosion: &Nbt) -> Vec<TooltipLine> {
    let byte = |key| explosion.get(key).and_then(Nbt::as_int).unwrap_or(0);
    let shape = byte("Type") as i8;
    let shape_key = if (0..=4).contains(&shape) {
        format!("item.fireworksCharge.type.{shape}")
    } else {
        "item.fireworksCharge.type".to_owned()
    };
    let mut lines = vec![gray(TooltipText::key(shape_key))];
    if let Some(colors) = colour_names(explosion.get("Colors")) {
        lines.push(gray(colors));
    }
    if let Some(fades) = colour_names(explosion.get("FadeColors")) {
        lines.push(gray(TooltipText::Concat(vec![
            TooltipText::key("item.fireworksCharge.fadeTo"),
            TooltipText::literal(" "),
            fades,
        ])));
    }
    for (key, line) in [("Trail", "trail"), ("Flicker", "flicker")] {
        if byte(key) != 0 {
            lines.push(gray(TooltipText::key(format!(
                "item.fireworksCharge.{line}"
            ))));
        }
    }
    lines
}

/// The names of the colours of an int array, joined by `", "`; a colour that
/// is no dye's is `customColor`. `None` for an empty or missing array.
fn colour_names(tag: Option<&Nbt>) -> Option<TooltipText> {
    let Some(Nbt::IntArray(colours)) = tag else {
        return None;
    };
    if colours.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for (index, colour) in colours.iter().enumerate() {
        if index > 0 {
            parts.push(TooltipText::literal(", "));
        }
        let name = DYES
            .iter()
            .find(|(rgb, _)| rgb == colour)
            .map_or("customColor", |(_, name)| name);
        parts.push(TooltipText::key(format!("item.fireworksCharge.{name}")));
    }
    Some(TooltipText::Concat(parts))
}

fn gray(text: TooltipText) -> TooltipLine {
    TooltipLine::plain(text, TooltipColor::Gray)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn explosion() -> Nbt {
        Nbt::Compound(vec![
            ("Type".into(), Nbt::Byte(1)),
            ("Colors".into(), Nbt::IntArray(vec![11_743_532, 123])),
            ("FadeColors".into(), Nbt::IntArray(vec![15_790_320])),
            ("Trail".into(), Nbt::Byte(1)),
        ])
    }

    #[test]
    fn a_star_lists_shape_colours_fade_and_trail() {
        let tag = Nbt::Compound(vec![("Explosion".into(), explosion())]);
        let lines = star_lines(Some(&tag));
        assert_eq!(lines.len(), 4);
        assert_eq!(
            lines[0].text,
            TooltipText::key("item.fireworksCharge.type.1")
        );
        assert_eq!(
            lines[1].text,
            TooltipText::Concat(vec![
                TooltipText::key("item.fireworksCharge.red"),
                TooltipText::literal(", "),
                TooltipText::key("item.fireworksCharge.customColor"),
            ])
        );
        assert_eq!(
            lines[3].text,
            TooltipText::key("item.fireworksCharge.trail")
        );
    }

    #[test]
    fn a_rocket_shows_its_flight_then_indented_stars() {
        let tag = Nbt::Compound(vec![(
            "Fireworks".into(),
            Nbt::Compound(vec![
                ("Flight".into(), Nbt::Byte(2)),
                ("Explosions".into(), Nbt::List(vec![explosion()])),
            ]),
        )]);
        let lines = rocket_lines(Some(&tag));
        assert_eq!(lines.len(), 5);
        assert_eq!(
            lines[0].text,
            TooltipText::Concat(vec![
                TooltipText::key("item.fireworks.flight"),
                TooltipText::literal(" 2"),
            ])
        );
        assert_eq!(
            lines[1].text,
            TooltipText::key("item.fireworksCharge.type.1")
        );
        assert!(
            matches!(&lines[2].text, TooltipText::Concat(parts) if parts[0] == TooltipText::literal("  "))
        );
    }
}

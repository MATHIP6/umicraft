// SPDX-License-Identifier: MIT OR Apache-2.0

//! The tooltip of a `show_achievement` hover event in the chat.
//!
//! Source: MCP `GuiScreen.handleComponentHover`, `StatBase.getStatName` and
//! `Achievement.getStatName` / `getDescription`. The event value is the id of
//! a statistic; an achievement shows its name (green, or dark purple when
//! special), an italic "Achievement" line and its description wrapped to 150
//! units. Statistics other than achievements have only a language name here.

use umicraft_core::lang::Lang;
use umicraft_game::achievements::ACHIEVEMENTS;
use umicraft_ui::legacy_text::wrap_legacy;
use umicraft_ui::widgets::tooltip::TooltipRow;
use umicraft_ui::FontAtlas;

/// The colour of plain tooltip text.
const WHITE: u32 = 0x00FF_FFFF;
/// The colour of the "invalid statistic" line (`EnumChatFormatting.RED`).
const RED: u32 = 0x00FF_5555;
/// The width the description is wrapped to (`listFormattedStringToWidth`).
const DESCRIPTION_WIDTH: f32 = 150.0;

/// The rows of the tooltip for the statistic `id`.
///
/// `inventory_key` fills the `%1$s` of the inventory achievement's
/// description (`StatList.openInventory` formatter); an unknown id gives the
/// red `umicraft.hover.invalidStat` line.
pub(crate) fn stat_rows(
    id: &str,
    lang: &Lang,
    font: &FontAtlas,
    inventory_key: &str,
) -> Vec<TooltipRow> {
    let achievement = id
        .strip_prefix("achievement.")
        .and_then(|name| ACHIEVEMENTS.iter().find(|def| def.id == name));
    if achievement.is_none() && lang.get(id).is_none() {
        return vec![row(lang.tr("umicraft.hover.invalidStat"), RED)];
    }
    let (colour, kind) = match achievement {
        Some(def) if def.special => ("\u{a7}5", "stats.tooltip.type.achievement"),
        Some(_) => ("\u{a7}a", "stats.tooltip.type.achievement"),
        None => ("\u{a7}7", "stats.tooltip.type.statistic"),
    };
    let mut rows = vec![
        row(&format!("{colour}{}\u{a7}r", lang.tr(id)), WHITE),
        row(&format!("\u{a7}o{}", lang.tr(kind)), WHITE),
    ];
    if achievement.is_some() {
        let description = lang.tr_args(&format!("{id}.desc"), &[inventory_key]);
        rows.extend(
            wrap_legacy(font, &description, DESCRIPTION_WIDTH)
                .iter()
                .map(|line| row(line, WHITE)),
        );
    }
    rows
}

fn row(text: &str, rgb: u32) -> TooltipRow {
    TooltipRow {
        text: text.to_string(),
        rgb,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lang() -> Lang {
        Lang::from_texts(
            "en_US",
            Some(
                "achievement.openInventory=Taking Inventory\n\
                 achievement.openInventory.desc=Press '%1$s' to open your inventory.\n\
                 achievement.theEnd2=The End?\n\
                 stats.tooltip.type.achievement=Achievement\n\
                 stats.tooltip.type.statistic=Statistic\n\
                 stat.jump=Jumps\n\
                 umicraft.hover.invalidStat=Invalid statistic/achievement!\n",
            ),
            None,
        )
    }

    #[test]
    fn an_achievement_shows_name_kind_and_description() {
        let rows = stat_rows(
            "achievement.openInventory",
            &lang(),
            &FontAtlas::fallback(),
            "E",
        );
        assert_eq!(rows[0].text, "\u{a7}aTaking Inventory\u{a7}r");
        assert_eq!(rows[1].text, "\u{a7}oAchievement");
        assert_eq!(rows[2].text, "Press 'E' to open your inventory.");
    }

    #[test]
    fn a_special_achievement_is_dark_purple() {
        let rows = stat_rows("achievement.theEnd2", &lang(), &FontAtlas::fallback(), "E");
        assert!(rows[0].text.starts_with("\u{a7}5"), "{rows:?}");
    }

    #[test]
    fn a_plain_statistic_is_grey_without_description() {
        let rows = stat_rows("stat.jump", &lang(), &FontAtlas::fallback(), "E");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].text, "\u{a7}7Jumps\u{a7}r");
        assert_eq!(rows[1].text, "\u{a7}oStatistic");
    }

    #[test]
    fn an_unknown_id_is_the_red_invalid_line() {
        let rows = stat_rows("nonsense", &lang(), &FontAtlas::fallback(), "E");
        assert_eq!(rows, vec![row("Invalid statistic/achievement!", RED)]);
    }
}

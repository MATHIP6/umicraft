// SPDX-License-Identifier: MIT OR Apache-2.0

//! The search tab's filter (MCP `GuiContainerCreative.updateCreativeSearch`).
//!
//! Vanilla 1.8 keeps an item when any line of its tooltip, stripped of
//! formatting codes and lower-cased, contains the lower-cased query. There is
//! no `#tag` or tab-name syntax in 1.8 (that arrived in 1.9), so none is
//! implemented here.

use umicraft_protocol::packets::play::ItemStack;

use super::items::tab_items;
use super::tabs::CreativeTab;
use crate::inventory::tooltip::{tooltip, TooltipText};

/// The character that starts a formatting code (`EnumChatFormatting`).
const SECTION_SIGN: char = '\u{a7}';

/// `EnumChatFormatting.getTextWithoutFormattingCodes`: drops each section
/// sign together with the character after it.
fn strip_formatting(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == SECTION_SIGN {
            chars.next();
        } else {
            plain.push(c);
        }
    }
    plain
}

/// Whether any tooltip line of `stack` contains the (lower-case) `query`.
fn matches(stack: &ItemStack, query: &str, resolve: &dyn Fn(&TooltipText) -> String) -> bool {
    tooltip(stack).iter().any(|line| {
        strip_formatting(&resolve(&line.text))
            .to_lowercase()
            .contains(query)
    })
}

/// The stacks of the search tab for a query, in catalogue order.
///
/// `resolve` turns the unresolved tooltip text (language keys) into the
/// displayed string, so the filter sees exactly what the player reads. An
/// empty query keeps everything.
///
/// ```
/// use umicraft_game::inventory::creative::search_items;
/// use umicraft_game::inventory::tooltip::TooltipText;
///
/// // A resolver that shows the language key itself.
/// let resolve = |text: &TooltipText| match text {
///     TooltipText::Translation { key, .. } => key.clone(),
///     TooltipText::Literal(text) => text.clone(),
///     TooltipText::Concat(_) => String::new(),
/// };
/// let found = search_items("diamond_sword", &resolve);
/// assert!(found.iter().any(|stack| stack.item_id == 276));
/// ```
#[must_use]
pub fn search_items(query: &str, resolve: &dyn Fn(&TooltipText) -> String) -> Vec<ItemStack> {
    let catalogue = tab_items(CreativeTab::Search);
    if query.is_empty() {
        return catalogue.to_vec();
    }
    let query = query.to_lowercase();
    catalogue
        .iter()
        .filter(|stack| matches(stack, &query, resolve))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resolves keys to `key` and concatenates parts; enough to test matching.
    fn resolve(text: &TooltipText) -> String {
        match text {
            TooltipText::Literal(text) => text.clone(),
            TooltipText::Translation { key, args } => {
                let mut out = key.clone();
                out.extend(args.iter().map(resolve));
                out
            }
            TooltipText::Concat(parts) => parts.iter().map(resolve).collect(),
        }
    }

    #[test]
    fn an_empty_query_keeps_the_whole_catalogue() {
        assert_eq!(
            search_items("", &resolve).len(),
            tab_items(CreativeTab::Search).len()
        );
    }

    #[test]
    fn search_matches_by_name_case_insensitively() {
        let found = search_items("DIAMOND_SWORD", &resolve);
        assert!(found.iter().any(|s| s.item_id == 276));
        assert!(found.iter().all(|s| {
            tooltip(s)
                .iter()
                .any(|l| resolve(&l.text).to_lowercase().contains("diamond_sword"))
        }));
    }

    #[test]
    fn a_resolver_sees_the_translated_text() {
        let found = search_items("zzz", &|_| "A ZZZ thing".to_owned());
        assert_eq!(found.len(), tab_items(CreativeTab::Search).len());
        assert!(search_items("nothing like it", &|_| "plain".to_owned()).is_empty());
    }

    #[test]
    fn formatting_codes_are_ignored_by_the_match() {
        let found = search_items("red", &|_| "\u{a7}cRed\u{a7}r Thing".to_owned());
        assert!(!found.is_empty());
        assert!(search_items("c", &|_| "\u{a7}c".to_owned()).is_empty());
    }

    #[test]
    fn book_levels_are_searchable_through_the_enchantment_lines() {
        let found = search_items("enchantment.level.3", &resolve);
        assert!(found.iter().any(|s| s.item_id == 403));
    }
}

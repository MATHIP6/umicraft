// SPDX-License-Identifier: MIT OR Apache-2.0

//! The item lists of the creative tabs, built once on first use.
//!
//! Plain `(id, damage)` rows come from the generated [`table`](super::table);
//! the rows that need NBT or a computation are expanded by
//! [`Entry::expand`](super::entry::Entry::expand). Vanilla rebuilds the list
//! on every tab switch; the content never changes, so it is cached.

use std::sync::OnceLock;

use umicraft_protocol::packets::play::ItemStack;

use super::entry::Entry;
use super::table::{BOOK_ENCHANTMENTS, SEARCH_ENTRIES, TAB_ENTRIES};
use super::tabs::CreativeTab;

struct Catalog {
    tabs: [Vec<ItemStack>; 12],
    search: Vec<ItemStack>,
}

fn expand(entries: &[Entry]) -> Vec<ItemStack> {
    let mut stacks = Vec::with_capacity(entries.len());
    for entry in entries {
        entry.expand(&mut stacks);
    }
    stacks
}

/// The search tab's list before filtering (`updateCreativeSearch`): every
/// item of every tab in registry order, then an enchanted book for each
/// level of each enchantment.
fn search_stacks() -> Vec<ItemStack> {
    let mut stacks = expand(SEARCH_ENTRIES);
    for &(enchantment, max_level) in BOOK_ENCHANTMENTS {
        let books = (1..=max_level).map(|level| Entry::Book { enchantment, level });
        for book in books {
            book.expand(&mut stacks);
        }
    }
    stacks
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| Catalog {
        tabs: std::array::from_fn(|index| expand(TAB_ENTRIES[index])),
        search: search_stacks(),
    })
}

/// The stacks a tab lists, in vanilla order (`displayAllReleventItems`).
///
/// The search tab returns the unfiltered search catalogue (see
/// [`search`](super::search)); the inventory tab lists no items.
///
/// ```
/// use umicraft_game::inventory::creative::{tab_items, CreativeTab};
///
/// let blocks = tab_items(CreativeTab::BuildingBlocks);
/// assert_eq!((blocks[0].item_id, blocks[0].damage), (1, 0)); // stone
/// ```
#[must_use]
pub fn tab_items(tab: CreativeTab) -> &'static [ItemStack] {
    match tab {
        CreativeTab::Search => &catalog().search,
        _ => &catalog().tabs[tab.index()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(tab: CreativeTab) -> Vec<(i16, i16)> {
        tab_items(tab)
            .iter()
            .map(|s| (s.item_id, s.damage))
            .collect()
    }

    #[test]
    fn building_blocks_start_with_the_stone_variants() {
        let first: Vec<_> = ids(CreativeTab::BuildingBlocks)
            .into_iter()
            .take(9)
            .collect();
        assert_eq!(
            first,
            [
                (1, 0),
                (1, 1),
                (1, 2),
                (1, 3),
                (1, 4),
                (1, 5),
                (1, 6),
                (2, 0),
                (3, 0)
            ]
        );
    }

    #[test]
    fn every_listing_tab_has_items_and_the_inventory_tab_none() {
        for tab in CreativeTab::ALL {
            let empty = tab_items(tab).is_empty();
            assert_eq!(empty, tab == CreativeTab::Inventory, "{tab:?}");
        }
    }

    #[test]
    fn every_listed_item_is_a_known_item() {
        for tab in CreativeTab::ALL {
            for stack in tab_items(tab) {
                assert!(
                    crate::items::lookup(i32::from(stack.item_id)).is_some(),
                    "{tab:?} lists unknown id {}",
                    stack.item_id
                );
                assert_eq!(stack.stack_size, 1);
            }
        }
    }

    #[test]
    fn dyes_wool_and_eggs_expand_by_damage() {
        let materials = ids(CreativeTab::Materials);
        assert_eq!(materials.iter().filter(|(id, _)| *id == 351).count(), 16);
        let misc = ids(CreativeTab::Misc);
        assert_eq!(misc.iter().filter(|(id, _)| *id == 383).count(), 27);
        assert!(misc.contains(&(383, 120)));
    }

    #[test]
    fn tools_and_combat_end_with_enchanted_books_at_max_level() {
        let tools_books = tab_items(CreativeTab::Tools)
            .iter()
            .filter(|s| s.item_id == 403)
            .count();
        let combat_books = tab_items(CreativeTab::Combat)
            .iter()
            .filter(|s| s.item_id == 403)
            .count();
        assert_eq!((tools_books, combat_books), (6, 19));
        assert_eq!(
            tab_items(CreativeTab::Tools).last().map(|s| s.item_id),
            Some(403)
        );
        assert!(tab_items(CreativeTab::Misc)
            .iter()
            .all(|s| s.item_id != 403));
    }

    #[test]
    fn brewing_lists_the_potion_variants_after_the_ghast_tear() {
        let brewing = ids(CreativeTab::Brewing);
        assert_eq!(brewing[0], (370, 0));
        assert_eq!(brewing[1], (373, 0));
        assert!(brewing.contains(&(373, 8194)));
        assert!(brewing.iter().filter(|(id, _)| *id == 373).count() > 40);
    }

    #[test]
    fn decorations_end_with_sixteen_banners() {
        let banners: Vec<_> = tab_items(CreativeTab::Decorations)
            .iter()
            .filter(|s| s.item_id == 425)
            .collect();
        assert_eq!(banners.len(), 16);
        assert_eq!(banners[0].damage, 15);
        assert!(banners.iter().all(|b| b.nbt.is_some()));
    }

    #[test]
    fn search_catalogue_has_every_item_then_every_book_level() {
        let search = tab_items(CreativeTab::Search);
        let books: Vec<_> = search.iter().filter(|s| s.item_id == 403).collect();
        let expected: usize = BOOK_ENCHANTMENTS.iter().map(|&(_, max)| max as usize).sum();
        assert_eq!(books.len(), expected);
        let tabbed: usize = CreativeTab::ALL
            .iter()
            .filter(|&&t| t != CreativeTab::Search)
            .map(|&t| tab_items(t).iter().filter(|s| s.item_id != 403).count())
            .sum();
        assert_eq!(search.len() - books.len(), tabbed);
        assert_eq!(search[0].item_id, 1);
    }
}

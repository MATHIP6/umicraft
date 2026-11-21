// SPDX-License-Identifier: MIT OR Apache-2.0

//! Which cards the search, the selected category and the All / Enabled filter
//! leave visible.

use super::view::{ModCardView, ModsView};

/// The tab that shows every category.
pub(super) const ALL_TAB: usize = 0;

/// Whether the name or the description of `card` contains `query`, ignoring
/// case. An empty query matches everything.
fn matches(card: &ModCardView, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || card.name.to_lowercase().contains(&query)
        || card.description.to_lowercase().contains(&query)
}

/// The indices in `view.cards` of the cards to list: those of the category of
/// `tab` (`ALL_TAB` for every one, else category `tab - 1`) that match `query`
/// and, with `only_enabled`, are on.
pub(super) fn visible(
    view: &ModsView,
    (query, tab): (&str, usize),
    only_enabled: bool,
) -> Vec<usize> {
    view.cards
        .iter()
        .enumerate()
        .filter(|(_, card)| tab == ALL_TAB || card.category + 1 == tab)
        .filter(|(_, card)| !only_enabled || card.enabled)
        .filter(|(_, card)| matches(card, query))
        .map(|(index, _)| index)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(name: &str, category: usize) -> ModCardView {
        ModCardView {
            id: name.to_lowercase(),
            name: name.into(),
            description: String::new(),
            category,
            enabled: false,
            settings: Vec::new(),
        }
    }

    fn view() -> ModsView {
        ModsView {
            categories: vec!["HUD".into(), "Gameplay".into()],
            cards: vec![
                card("FPS", 0),
                card("Coordinates", 0),
                card("Toggle Sprint", 1),
            ],
            ..ModsView::default()
        }
    }

    #[test]
    fn no_query_and_the_all_tab_list_everything() {
        assert_eq!(visible(&view(), ("", ALL_TAB), false), vec![0, 1, 2]);
        assert_eq!(visible(&view(), ("   ", ALL_TAB), false), vec![0, 1, 2]);
    }

    #[test]
    fn the_search_ignores_case_and_looks_inside_the_name() {
        assert_eq!(visible(&view(), ("fPs", ALL_TAB), false), vec![0]);
        assert_eq!(visible(&view(), ("or", ALL_TAB), false), vec![1]);
        assert!(visible(&view(), ("zzz", ALL_TAB), false).is_empty());
    }

    #[test]
    fn the_search_also_looks_inside_the_description() {
        let mut view = view();
        view.cards[2].description = "Keeps you running".into();
        assert_eq!(visible(&view, ("running", ALL_TAB), false), vec![2]);
    }

    #[test]
    fn a_category_tab_keeps_only_its_mods() {
        assert_eq!(visible(&view(), ("", 1), false), vec![0, 1]);
        assert_eq!(visible(&view(), ("", 2), false), vec![2]);
        assert_eq!(visible(&view(), ("sprint", 1), false), Vec::<usize>::new());
        assert_eq!(visible(&view(), ("sprint", 2), false), vec![2]);
    }

    #[test]
    fn the_enabled_filter_keeps_the_mods_that_are_on() {
        let mut view = view();
        view.cards[1].enabled = true;
        assert_eq!(visible(&view, ("", ALL_TAB), true), vec![1]);
        assert!(visible(&view, ("", 2), true).is_empty());
    }
}

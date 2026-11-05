// SPDX-License-Identifier: MIT OR Apache-2.0

//! The server icons of the list: the favicons the pings returned, handed to
//! the window as dynamic GUI-atlas images and bound back to the rows as
//! sprite sheets.
//!
//! The window owns the atlas and the GPU; this module only knows the RGBA
//! pixels and a revision counter that changes whenever the set of icons or
//! their order does, so the window re-uploads only then. An entry that is
//! removed from the book loses its icon at the next rebuild of the rows, so no
//! texture outlives its entry (vanilla deletes `servers/<ip>/icon`).

use umicraft_assets::{DynamicImage, GuiAtlas, DYNAMIC_PREFIX};

use super::MenuFlow;
use crate::official::sprite;
use crate::servers::ICON_SIDE;

/// The atlas name of the icon of row `index` (without the dynamic prefix).
fn icon_name(index: usize) -> String {
    format!("server{index}")
}

impl MenuFlow {
    /// Changes whenever [`Self::icon_images`] would return something else.
    #[must_use]
    pub fn icons_revision(&self) -> u64 {
        self.icons_revision
    }

    /// The favicons of the current rows, as atlas images named `server<row>`.
    #[must_use]
    pub fn icon_images(&self) -> Vec<DynamicImage> {
        self.book
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let pixels = self.favicons.get(entry.address())?;
                Some(DynamicImage {
                    name: icon_name(index),
                    width: ICON_SIDE,
                    height: ICON_SIDE,
                    pixels: pixels.clone(),
                })
            })
            .collect()
    }

    /// Points each row at its icon region in `atlas`, which the window built
    /// from [`Self::icon_images`]; a row without a region keeps the default.
    pub fn assign_icon_sheets(&mut self, atlas: &GuiAtlas) {
        for (index, row) in self.rows.iter_mut().enumerate() {
            let region = atlas.region(&format!("{DYNAMIC_PREFIX}{}", icon_name(index)));
            row.favicon = region.map(sprite);
        }
    }

    /// Stores or drops the icon of `address`; bumps the revision on a change.
    pub(super) fn set_favicon(&mut self, address: &str, rgba: Option<Vec<u8>>) {
        let changed = match rgba {
            Some(pixels) => {
                self.favicons.insert(address.to_string(), pixels.clone()) != Some(pixels)
            }
            None => self.favicons.remove(address).is_some(),
        };
        if changed {
            self.icons_revision += 1;
        }
    }

    /// Forgets the icons of entries that left the book and tells the window
    /// the row order may have changed.
    pub(super) fn prune_favicons(&mut self) {
        let book = &self.book;
        self.favicons
            .retain(|address, _| book.iter().any(|entry| entry.address() == address));
        self.icons_revision += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::servers::ServerBook;

    fn flow_with(name: &str, addresses: &[&str]) -> MenuFlow {
        let folder = std::env::temp_dir().join(format!("umicraft-menu-icons-{name}"));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("temp folder");
        let mut book = ServerBook::open(&folder.join("servers.toml"));
        for address in addresses {
            book.add("", address).expect("valid entry");
        }
        MenuFlow::new(book, None, String::new(), 0)
    }

    #[test]
    fn an_icon_is_listed_by_row_and_dropped_with_its_entry() {
        let mut flow = flow_with("list", &["a.example", "b.example"]);
        assert!(flow.icon_images().is_empty());
        let before = flow.icons_revision();
        flow.set_favicon("b.example", Some(vec![7; 64 * 64 * 4]));
        assert!(flow.icons_revision() > before);
        let images = flow.icon_images();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].name, "server1");
        flow.set_favicon("b.example", None);
        assert!(flow.icon_images().is_empty());
    }

    #[test]
    fn an_unchanged_icon_does_not_bump_the_revision() {
        let mut flow = flow_with("same", &["a.example"]);
        flow.set_favicon("a.example", Some(vec![1; 4]));
        let revision = flow.icons_revision();
        flow.set_favicon("a.example", Some(vec![1; 4]));
        assert_eq!(flow.icons_revision(), revision);
    }

    #[test]
    fn deleting_an_entry_drops_its_icon() {
        let mut flow = flow_with("delete", &["a.example", "b.example"]);
        flow.set_favicon("a.example", Some(vec![1; 4]));
        flow.delete(0);
        assert!(flow.icon_images().is_empty());
        assert!(flow.favicons.is_empty(), "the pixels are freed too");
    }

    #[test]
    fn rows_pick_up_their_region_of_the_rebuilt_atlas() {
        let mut flow = flow_with("assign", &["a.example", "b.example"]);
        flow.set_favicon("b.example", Some(vec![7; 64 * 64 * 4]));
        let atlas = GuiAtlas::empty().with_dynamic(&flow.icon_images());
        flow.assign_icon_sheets(&atlas);
        assert!(flow.rows[0].favicon.is_none());
        let sheet = flow.rows[1].favicon.expect("row 1 has an icon");
        assert_eq!((sheet.width, sheet.height), (64, 64));
    }
}

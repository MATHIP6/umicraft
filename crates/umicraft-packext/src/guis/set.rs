// SPDX-License-Identifier: MIT OR Apache-2.0

//! The rules of every pack, ordered, with the lookup of a replacement.

use super::rule::{GuiContext, GuiRule};
use crate::cem::PackPath;

/// The rules of the packs in match order.
///
/// # Example
///
/// ```
/// use umicraft_packext::guis::{Container, GuiContext, GuiSet};
///
/// let set = GuiSet::parse([(
///     "gui/container/furnace.properties",
///     &b"container=furnace\ntexture=hot\n"[..],
/// )]);
/// let found = set.texture(&GuiContext::of(Container::Furnace), "container/furnace");
/// assert!(found.is_some());
/// ```
#[derive(Debug, Clone, Default)]
pub struct GuiSet {
    rules: Vec<GuiRule>,
}

impl GuiSet {
    /// Reads the rule files, `(path relative to the pack-feature folder,
    /// bytes)`; only `gui/container/**.properties` count. A broken file is
    /// skipped with a warning. Rules are ordered by path.
    pub fn parse<'a>(files: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> Self {
        let mut rules: Vec<GuiRule> = files
            .into_iter()
            .filter(|(path, _)| path.starts_with("gui/container/") && path.ends_with(".properties"))
            .filter_map(|(path, bytes)| {
                GuiRule::parse(bytes, path)
                    .map_err(|error| tracing::warn!(%path, %error, "custom GUI rule ignored"))
                    .ok()
            })
            .collect();
        rules.sort_by(|a, b| a.path.cmp(&b.path));
        Self { rules }
    }

    /// The rules in match order.
    #[must_use]
    pub fn rules(&self) -> &[GuiRule] {
        &self.rules
    }

    /// Whether no rule was read.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The replacement of the vanilla picture `region` for the screen of
    /// `context`: from the first rule that matches the screen, `None` when it
    /// has no replacement for `region`.
    #[must_use]
    pub fn texture(&self, context: &GuiContext<'_>, region: &str) -> Option<&PackPath> {
        self.rules
            .iter()
            .find(|rule| rule.matches(context))?
            .texture_for(region)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guis::Container;

    fn set(files: &[(&str, &str)]) -> GuiSet {
        GuiSet::parse(files.iter().map(|(path, text)| (*path, text.as_bytes())))
    }

    #[test]
    fn the_first_matching_rule_decides_even_without_the_picture() {
        let set = set(&[
            (
                "gui/container/a_large.properties",
                "container=chest\nlarge=true\ntexture.container/other=o\n",
            ),
            (
                "gui/container/b_any.properties",
                "container=chest\ntexture=any\n",
            ),
        ]);
        let large = GuiContext {
            large: Some(true),
            ..GuiContext::of(Container::Chest)
        };
        assert_eq!(set.texture(&large, "container/generic_54"), None);
        assert!(set.texture(&large, "container/other").is_some());
        let small = GuiContext {
            large: Some(false),
            ..large
        };
        assert!(set.texture(&small, "container/generic_54").is_some());
    }

    #[test]
    fn rules_are_ordered_by_path_and_foreign_files_skipped() {
        let set = set(&[
            ("gui/container/z.properties", "container=chest\ntexture=z\n"),
            (
                "gui/container/b/a.properties",
                "container=chest\ntexture=a\n",
            ),
            ("gui/container/bad.properties", "container=nothing\n"),
            ("gui/other.properties", "container=chest\ntexture=o\n"),
            ("gui/container/readme.txt", "container=chest\ntexture=o\n"),
        ]);
        let paths: Vec<_> = set.rules().iter().map(|r| r.path.as_str()).collect();
        assert_eq!(
            paths,
            ["gui/container/b/a.properties", "gui/container/z.properties"]
        );
        assert!(GuiSet::default().is_empty());
    }
}

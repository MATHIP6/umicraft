// SPDX-License-Identifier: MIT OR Apache-2.0

//! The language the client shows its texts in (ADR-0039).
//!
//! [`load`] builds a [`Lang`] from the downloaded `.lang` files: `en_US` is
//! always read (the vanilla fallback), then the selected language on top. The
//! window holds the result in an `Arc` and also [`install`]s it as the shared
//! current language, so code on other threads (the game thread parses chat
//! components) sees the same texts. A language switch is one [`install`] of a
//! new handle.

use std::sync::{Arc, RwLock};

use umicraft_assets::cache::AssetCache;
use umicraft_core::lang::Lang;

/// The shared current language; the builtin fallback until [`install`].
static CURRENT: RwLock<Option<Arc<Lang>>> = RwLock::new(None);

/// Builds the language handle for `code` from the downloaded files.
///
/// A language that cannot be read is logged and replaced by `en_US`, and a
/// missing `en_US` leaves the offline fallback, so the client always starts.
pub(crate) fn load(code: &str) -> Lang {
    let english = crate::official::load_en_us_lang();
    let selected = (code != Lang::DEFAULT_CODE)
        .then(|| load_selected(code))
        .flatten();
    let code = if selected.is_some() || code == Lang::DEFAULT_CODE {
        code
    } else {
        Lang::DEFAULT_CODE
    };
    Lang::from_texts(code, english.as_deref(), selected.as_deref())
}

fn load_selected(code: &str) -> Option<String> {
    let cache = AssetCache::in_data_dir().ok()?;
    match umicraft_assets::languages::load_language(&cache, code) {
        Ok(text) => Some(text),
        Err(error) => {
            tracing::warn!(language = code, %error, "language not loaded, using en_US");
            None
        }
    }
}

/// Makes `lang` the language [`current`] returns.
pub(crate) fn install(lang: Arc<Lang>) {
    if let Ok(mut current) = CURRENT.write() {
        *current = Some(lang);
    }
}

/// The installed language, or the offline fallback before [`install`].
pub(crate) fn current() -> Arc<Lang> {
    CURRENT
        .read()
        .ok()
        .and_then(|current| current.clone())
        .unwrap_or_else(|| Arc::new(Lang::builtin()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_installed_language_becomes_the_current_one() {
        let lang = Arc::new(Lang::from_texts("xx_XX", None, Some("k=v\n")));
        install(lang);
        assert_eq!(current().tr("k"), "v");
        install(Arc::new(Lang::builtin()));
        assert_eq!(current().tr("k"), "k");
    }
}

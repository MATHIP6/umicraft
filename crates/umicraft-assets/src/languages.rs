// SPDX-License-Identifier: MIT OR Apache-2.0

//! The languages of the downloaded assets (ADR-0039).
//!
//! The asset index lists one `minecraft/lang/<code>.lang` object per
//! language, and its `pack.mcmeta` object carries the `language` table with
//! the display name and region of each (`LanguageManager.parseLanguageMetadata`
//! reads the same `language` section of the pack metadata). Nothing is read
//! from the repository: the files are the user's download (spec §3).

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::cache::AssetCache;
use crate::error::{AssetsError, AssetsResult};
use crate::objects::AssetIndex;

const LANG_PREFIX: &str = "minecraft/lang/";
const LANG_SUFFIX: &str = ".lang";
const PACK_META: &str = "pack.mcmeta";

/// One selectable language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageInfo {
    /// The code, `en_US` style; also the name of the `.lang` file.
    pub code: String,
    /// The display name (`English`); the code when `pack.mcmeta` has none.
    pub name: String,
    /// The region (`US`); the part of the code after `_` when there is none.
    pub region: String,
}

#[derive(Deserialize)]
struct PackMeta {
    #[serde(default)]
    language: BTreeMap<String, LanguageMeta>,
}

#[derive(Deserialize)]
struct LanguageMeta {
    name: String,
    region: String,
}

/// Whether `code` has the shape of a language code: two or three lowercase
/// letters, `_`, two or three uppercase letters (`en_US`, `fil_PH`).
///
/// A code names a file in the object index, so anything else, such as a path
/// with separators, is rejected before it is used.
#[must_use]
pub fn is_language_code(code: &str) -> bool {
    let Some((language, region)) = code.split_once('_') else {
        return false;
    };
    let all = |part: &str, test: fn(&u8) -> bool| {
        (2..=3).contains(&part.len()) && part.bytes().all(|b| test(&b))
    };
    all(language, u8::is_ascii_lowercase) && all(region, u8::is_ascii_uppercase)
}

/// The languages `index` has a `.lang` object for, sorted by code.
///
/// `pack_meta` is the text of the `pack.mcmeta` object when it is available;
/// it provides the names, the file names alone give a language its code as
/// name. A malformed `pack.mcmeta` is ignored for the same reason.
#[must_use]
pub fn languages_in(index: &AssetIndex, pack_meta: Option<&str>) -> Vec<LanguageInfo> {
    let meta = pack_meta
        .and_then(|text| serde_json::from_str::<PackMeta>(text).ok())
        .map(|meta| meta.language)
        .unwrap_or_default();
    let mut languages: Vec<LanguageInfo> = index
        .entries()
        .iter()
        .filter_map(|(path, _)| path.strip_prefix(LANG_PREFIX)?.strip_suffix(LANG_SUFFIX))
        .filter(|code| is_language_code(code))
        .map(|code| match meta.get(code) {
            Some(info) => LanguageInfo {
                code: code.to_string(),
                name: info.name.clone(),
                region: info.region.clone(),
            },
            None => LanguageInfo {
                code: code.to_string(),
                name: code.to_string(),
                region: code
                    .split_once('_')
                    .map_or("", |(_, region)| region)
                    .to_string(),
            },
        })
        .collect();
    languages.sort_by(|a, b| a.code.cmp(&b.code));
    languages
}

/// Reads an index object as UTF-8 text, verifying its hash.
fn read_text(cache: &AssetCache, index: &AssetIndex, path: &str) -> Option<AssetsResult<String>> {
    let object = index.get(path)?;
    Some(cache.read_object(&object.hash).and_then(|bytes| {
        String::from_utf8(bytes).map_err(|error| AssetsError::Io {
            path: path.to_string(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, error),
        })
    }))
}

/// The languages the downloaded assets in `cache` contain.
///
/// # Errors
///
/// [`AssetsError::Missing`] when no asset index is cached yet.
pub fn available_languages(cache: &AssetCache) -> AssetsResult<Vec<LanguageInfo>> {
    let index = AssetIndex::load_cached(cache).ok_or_else(|| AssetsError::Missing {
        what: "asset index".to_string(),
        path: cache.indexes_dir().display().to_string(),
    })?;
    let meta = read_text(cache, &index, PACK_META).and_then(Result::ok);
    Ok(languages_in(&index, meta.as_deref()))
}

/// The text of the `.lang` file of language `code`.
///
/// # Errors
///
/// [`AssetsError::UnknownLanguage`] when `code` is malformed or the index has
/// no such file, [`AssetsError::Missing`] when no index or object is cached,
/// [`AssetsError::HashMismatch`] when the object is corrupt and
/// [`AssetsError::Io`] when it is not UTF-8.
pub fn load_language(cache: &AssetCache, code: &str) -> AssetsResult<String> {
    if !is_language_code(code) {
        return Err(AssetsError::UnknownLanguage(code.to_string()));
    }
    let index = AssetIndex::load_cached(cache).ok_or_else(|| AssetsError::Missing {
        what: "asset index".to_string(),
        path: cache.indexes_dir().display().to_string(),
    })?;
    let path = format!("{LANG_PREFIX}{code}{LANG_SUFFIX}");
    read_text(cache, &index, &path)
        .unwrap_or_else(|| Err(AssetsError::UnknownLanguage(code.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash;

    const PACK: &str = r#"{"pack":{"pack_format":1},"language":{
        "en_US":{"name":"English","region":"US","bidirectional":false},
        "de_DE":{"name":"Deutsch","region":"Deutschland","bidirectional":false}}}"#;

    fn cache_with(tag: &str, files: &[(&str, &str)]) -> (AssetCache, std::path::PathBuf) {
        let root =
            std::env::temp_dir().join(format!("umicraft-languages-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let cache = AssetCache::new(&root);
        cache.ensure_dirs().expect("dirs");
        let mut objects = Vec::new();
        for (path, text) in files {
            let sha = hash::sha1_hex(text.as_bytes());
            cache.store_object(&sha, text.as_bytes()).expect("store");
            objects.push(format!(
                r#""{path}":{{"hash":"{sha}","size":{}}}"#,
                text.len()
            ));
        }
        let index = format!(r#"{{"objects":{{{}}}}}"#, objects.join(","));
        std::fs::write(cache.indexes_dir().join("1.8.json"), index).expect("index");
        (cache, root)
    }

    #[test]
    fn language_codes_are_validated() {
        assert!(is_language_code("en_US"));
        assert!(is_language_code("fil_PH"));
        assert!(!is_language_code("en"));
        assert!(!is_language_code("../en_US"));
        assert!(!is_language_code("en_us"));
        assert!(!is_language_code(""));
    }

    #[test]
    fn languages_get_their_names_from_pack_mcmeta() {
        let (cache, root) = cache_with(
            "names",
            &[
                ("minecraft/lang/en_US.lang", "a=b\n"),
                ("minecraft/lang/de_DE.lang", "a=c\n"),
                ("minecraft/lang/xx_YY.lang", "a=d\n"),
                ("minecraft/sounds/x.ogg", "x"),
                ("pack.mcmeta", PACK),
            ],
        );
        let languages = available_languages(&cache).expect("languages");
        let codes: Vec<&str> = languages.iter().map(|l| l.code.as_str()).collect();
        assert_eq!(codes, ["de_DE", "en_US", "xx_YY"]);
        assert_eq!(languages[0].name, "Deutsch");
        assert_eq!(languages[1].region, "US");
        assert_eq!(languages[2].name, "xx_YY", "no metadata gives the code");
        assert_eq!(languages[2].region, "YY");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_language_file_is_loaded_by_code() {
        let (cache, root) = cache_with(
            "load",
            &[("minecraft/lang/de_DE.lang", "gui.done=Fertig\n")],
        );
        assert_eq!(
            load_language(&cache, "de_DE").expect("loads"),
            "gui.done=Fertig\n"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn unknown_and_malformed_codes_are_typed_errors() {
        let (cache, root) = cache_with("unknown", &[("minecraft/lang/de_DE.lang", "a=b\n")]);
        assert!(matches!(
            load_language(&cache, "fr_FR"),
            Err(AssetsError::UnknownLanguage(_))
        ));
        assert!(matches!(
            load_language(&cache, "../x"),
            Err(AssetsError::UnknownLanguage(_))
        ));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn without_an_index_the_error_is_missing() {
        let cache = AssetCache::new("/tmp/umicraft-languages-none");
        assert!(matches!(
            available_languages(&cache),
            Err(AssetsError::Missing { .. })
        ));
    }
}

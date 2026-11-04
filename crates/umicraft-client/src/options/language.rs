// SPDX-License-Identifier: MIT OR Apache-2.0

//! The language option (`GameSettings.language`).

use serde::{Deserialize, Serialize};
use umicraft_assets::languages::is_language_code;
use umicraft_core::lang::Lang;

/// The language option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LanguageOptions {
    /// The code of the language of the texts, `en_US` style (the name of a
    /// `minecraft/lang/<code>.lang` asset).
    pub code: String,
}

impl Default for LanguageOptions {
    fn default() -> Self {
        Self {
            code: Lang::DEFAULT_CODE.to_string(),
        }
    }
}

impl LanguageOptions {
    /// Replaces a code that is not shaped like a language code with the
    /// default. Whether the language is actually installed is only known
    /// when the file is loaded, which falls back to `en_US` itself.
    pub fn clamp(&mut self) {
        if !is_language_code(&self.code) {
            *self = Self::default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_language_is_english() {
        assert_eq!(LanguageOptions::default().code, "en_US");
    }

    #[test]
    fn a_malformed_code_returns_to_the_default() {
        for bad in ["", "english", "../en_US", "en_us"] {
            let mut options = LanguageOptions {
                code: bad.to_string(),
            };
            options.clamp();
            assert_eq!(options, LanguageOptions::default(), "{bad:?}");
        }
        let mut german = LanguageOptions {
            code: "de_DE".to_string(),
        };
        german.clamp();
        assert_eq!(german.code, "de_DE");
    }
}

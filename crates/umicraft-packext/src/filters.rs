// SPDX-License-Identifier: MIT OR Apache-2.0

//! The conditions several features share: biomes and weather. Heights are a
//! plain [`RangeList`](crate::RangeList).
//!
//! The biome rules follow OptiFine's `ConnectedParser.parseBiomes` and
//! `findBiome` (ADR-0088):
//! - names are separated by spaces and compared case-insensitively;
//! - a biome's own name is compared with its spaces removed, so
//!   `extremehills` matches "Extreme Hills";
//! - `nether` is an alias of the biome "Hell";
//! - a leading `!` inverts the list.
//!
//! OptiFine warns about unknown names and drops them. Here an unknown name
//! simply never matches, which gives the same result with or without `!`.
//!
//! The weather list follows `CustomSkyLayer.parseWeatherList`: the words are
//! `clear`, `rain` and `thunder`, and an unknown word is dropped with a
//! warning.

/// A biome condition, matched against the biome's display name.
///
/// # Example
///
/// ```
/// use umicraft_packext::BiomeFilter;
///
/// let filter = BiomeFilter::parse("Plains extremeHills");
/// assert!(filter.matches("Extreme Hills"));
/// assert!(!filter.matches("Desert"));
///
/// let not_nether = BiomeFilter::parse("!nether");
/// assert!(!not_nether.matches("Hell"));
/// assert!(not_nether.matches("Plains"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BiomeFilter {
    /// Normalised names: lowercase, no spaces.
    names: Vec<String>,
    inverted: bool,
}

impl BiomeFilter {
    /// Parses a space-separated list of biome names, optionally preceded by `!`.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let text = text.trim();
        let (inverted, list) = match text.strip_prefix('!') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let names = list
            .split(' ')
            .filter(|name| !name.is_empty())
            .map(|name| alias(&name.to_lowercase()))
            .collect();
        Self { names, inverted }
    }

    /// Whether the biome called `display_name` (for example "Extreme Hills")
    /// passes the filter.
    #[must_use]
    pub fn matches(&self, display_name: &str) -> bool {
        let listed = self.names.iter().any(|name| same_name(name, display_name));
        listed != self.inverted
    }
}

/// `nether` names the biome "Hell" in 1.8.
fn alias(name: &str) -> String {
    if name == "nether" {
        "hell".to_string()
    } else {
        name.to_string()
    }
}

/// Compares a normalised filter name with a display name, ignoring the
/// display name's spaces and case, without allocating.
fn same_name(normalised: &str, display_name: &str) -> bool {
    let mut display = display_name
        .chars()
        .filter(|&c| c != ' ')
        .flat_map(char::to_lowercase);
    let mut wanted = normalised.chars();
    loop {
        match (wanted.next(), display.next()) {
            (None, None) => return true,
            (Some(a), Some(b)) if a == b => {}
            _ => return false,
        }
    }
}

/// The weathers a feature is active in.
///
/// # Example
///
/// ```
/// use umicraft_packext::WeatherSet;
///
/// let weather = WeatherSet::parse("rain thunder");
/// assert!(!weather.clear && weather.rain && weather.thunder);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WeatherSet {
    /// Active in clear weather.
    pub clear: bool,
    /// Active while it rains.
    pub rain: bool,
    /// Active during a thunderstorm.
    pub thunder: bool,
}

impl WeatherSet {
    /// Parses a space-separated list of `clear`, `rain` and `thunder`.
    /// Unknown words are dropped with a warning, as OptiFine does.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut set = Self::default();
        for word in text.split(' ').filter(|word| !word.is_empty()) {
            match word {
                "clear" => set.clear = true,
                "rain" => set.rain = true,
                "thunder" => set.thunder = true,
                unknown => tracing::warn!(weather = unknown, "unknown weather, ignored"),
            }
        }
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_ignore_case_and_the_spaces_of_the_biome_name() {
        let filter = BiomeFilter::parse("extremehills MESA");
        assert!(filter.matches("Extreme Hills"));
        assert!(filter.matches("Mesa"));
        assert!(!filter.matches("Extreme Hills+"));
    }

    #[test]
    fn nether_is_hell() {
        assert!(BiomeFilter::parse("nether").matches("Hell"));
    }

    #[test]
    fn a_bang_inverts_and_unknown_names_never_match() {
        let filter = BiomeFilter::parse(" !Ocean noSuchBiome ");
        assert!(!filter.matches("Ocean"));
        assert!(filter.matches("Desert"));
        assert!(!BiomeFilter::parse("noSuchBiome").matches("Desert"));
    }

    #[test]
    fn an_empty_list_matches_nothing_and_its_inverse_everything() {
        assert!(!BiomeFilter::parse("").matches("Plains"));
        assert!(BiomeFilter::parse("!").matches("Plains"));
    }

    #[test]
    fn weather_words_and_unknown_ones() {
        assert_eq!(
            WeatherSet::parse("clear  snow rain"),
            WeatherSet {
                clear: true,
                rain: true,
                thunder: false
            }
        );
        assert_eq!(WeatherSet::parse(""), WeatherSet::default());
    }
}

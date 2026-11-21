// SPDX-License-Identifier: MIT OR Apache-2.0

//! One `.properties` rule of Custom GUIs and its matching.

use thiserror::Error;

use crate::cem::{resolve_path, PackPath};
use crate::random_entities::Profession;
use crate::{BiomeFilter, Properties, PropertiesError, RangeList, TextMatch};

/// The dye colours, by name, for `colors`.
const COLORS: [&str; 16] = [
    "white",
    "orange",
    "magenta",
    "light_blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light_gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];

/// Why a rule file was rejected. The rule is skipped; the others stay.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RuleError {
    /// The file is not a valid `.properties` file.
    #[error(transparent)]
    Properties(#[from] PropertiesError),
    /// `container` is missing or unknown.
    #[error("unknown or missing container {0:?}")]
    UnknownContainer(String),
    /// No `texture` or `texture.<path>`.
    #[error("no texture given")]
    NoTexture,
    /// `professions`, `variants` or `colors` holds an unknown entry.
    #[error("invalid {0}")]
    InvalidList(&'static str),
    /// `name` is an invalid regular expression.
    #[error("invalid name: {0}")]
    InvalidName(String),
}

/// The container screens a rule may replace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Container {
    /// The anvil.
    Anvil,
    /// The beacon.
    Beacon,
    /// The brewing stand.
    BrewingStand,
    /// Chests, large and small, and the ender chest.
    Chest,
    /// The crafting table.
    Crafting,
    /// The creative inventory (every texture is named by path).
    Creative,
    /// The dispenser and the dropper.
    Dispenser,
    /// The enchanting table.
    Enchantment,
    /// The furnace.
    Furnace,
    /// The hopper.
    Hopper,
    /// The horse inventory.
    Horse,
    /// The survival inventory.
    Inventory,
    /// The shulker box (a later version's container, parsed for packs that
    /// share one rule set).
    ShulkerBox,
    /// The villager trade screen.
    Villager,
}

impl Container {
    /// The container named by the `container` key.
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "anvil" => Self::Anvil,
            "beacon" => Self::Beacon,
            "brewing_stand" => Self::BrewingStand,
            "chest" => Self::Chest,
            "crafting" => Self::Crafting,
            "creative" => Self::Creative,
            "dispenser" => Self::Dispenser,
            "enchantment" => Self::Enchantment,
            "furnace" => Self::Furnace,
            "hopper" => Self::Hopper,
            "horse" => Self::Horse,
            "inventory" => Self::Inventory,
            "shulker_box" => Self::ShulkerBox,
            "villager" => Self::Villager,
            _ => return None,
        })
    }

    /// The GUI atlas region of the container's default texture, which a plain
    /// `texture=` key replaces. `None` for the creative inventory.
    #[must_use]
    pub fn default_region(self) -> Option<&'static str> {
        Some(match self {
            Self::Anvil => "container/anvil",
            Self::Beacon => "container/beacon",
            Self::BrewingStand => "container/brewing_stand",
            Self::Chest => "container/generic_54",
            Self::Crafting => "container/crafting_table",
            Self::Creative => return None,
            Self::Dispenser => "container/dispenser",
            Self::Enchantment => "container/enchanting_table",
            Self::Furnace => "container/furnace",
            Self::Hopper => "container/hopper",
            Self::Horse => "container/horse",
            Self::Inventory => "container/inventory",
            Self::ShulkerBox => "container/shulker_box",
            Self::Villager => "container/villager",
        })
    }

    /// The `variants` words the container accepts.
    fn variants(self) -> &'static [&'static str] {
        match self {
            Self::Horse => &["horse", "donkey", "mule", "llama"],
            Self::Dispenser => &["dispenser", "dropper"],
            _ => &[],
        }
    }
}

/// What the screen being drawn is, as far as the client knows.
///
/// A fact the client cannot know stays `None`, and a rule that asks for it
/// does not match.
#[derive(Debug, Clone, Copy)]
pub struct GuiContext<'a> {
    /// The container.
    pub container: Container,
    /// The custom name or title of the container.
    pub name: Option<&'a str>,
    /// The display name of the biome where it stands.
    pub biome: Option<&'a str>,
    /// The block height where it stands.
    pub height: Option<i32>,
    /// Whether a chest is a double chest.
    pub large: Option<bool>,
    /// Whether a chest is a trapped chest.
    pub trapped: Option<bool>,
    /// Whether a chest has its Christmas look.
    pub christmas: Option<bool>,
    /// Whether a chest is an ender chest.
    pub ender: Option<bool>,
    /// The levels of a beacon.
    pub levels: Option<i32>,
    /// The villager profession and, when known, career.
    pub profession: Option<(i32, Option<i32>)>,
    /// The variant word: `donkey`, `dropper`, ...
    pub variant: Option<&'a str>,
    /// The colour word: `red`, `light_blue`, ...
    pub color: Option<&'a str>,
}

impl<'a> GuiContext<'a> {
    /// A context that knows only the container.
    #[must_use]
    pub fn of(container: Container) -> Self {
        Self {
            container,
            name: None,
            biome: None,
            height: None,
            large: None,
            trapped: None,
            christmas: None,
            ender: None,
            levels: None,
            profession: None,
            variant: None,
            color: None,
        }
    }
}

/// One rule.
#[derive(Debug, Clone)]
pub struct GuiRule {
    /// The folder-relative path of the file, which orders the rules.
    pub path: String,
    /// The container it replaces.
    pub container: Container,
    /// The replacements: the GUI atlas region name of the vanilla picture
    /// (`container/generic_54`) and the replacement file.
    pub textures: Vec<(String, PackPath)>,
    name: Option<TextMatch>,
    biomes: Option<BiomeFilter>,
    heights: Option<RangeList>,
    large: Option<bool>,
    trapped: Option<bool>,
    christmas: Option<bool>,
    ender: Option<bool>,
    levels: Option<RangeList>,
    professions: Option<Vec<Profession>>,
    variants: Option<Vec<String>>,
    colors: Option<Vec<String>>,
}

impl GuiRule {
    /// Parses the file at `path` (relative to the pack-feature folder, for
    /// example `gui/container/chest/blue.properties`).
    ///
    /// # Errors
    ///
    /// [`RuleError`] when the file is unusable.
    ///
    /// # Example
    ///
    /// ```
    /// use umicraft_packext::guis::{GuiRule, Container};
    ///
    /// let rule = GuiRule::parse(
    ///     b"container=chest\ntexture=blue_chest\nlarge=true\n",
    ///     "gui/container/blue.properties",
    /// ).expect("valid");
    /// assert_eq!(rule.container, Container::Chest);
    /// assert_eq!(rule.textures[0].0, "container/generic_54");
    /// ```
    pub fn parse(bytes: &[u8], path: &str) -> Result<Self, RuleError> {
        let props = Properties::parse(bytes)?;
        let base = path.rsplit_once('/').map_or("", |(dir, _)| dir);
        let container = props
            .get("container")
            .and_then(Container::parse)
            .ok_or_else(|| {
                RuleError::UnknownContainer(props.get("container").unwrap_or("").to_string())
            })?;
        let textures = textures(&props, container, base);
        if textures.is_empty() {
            return Err(RuleError::NoTexture);
        }
        Ok(Self {
            path: path.to_string(),
            container,
            textures,
            name: props
                .get("name")
                .map(|text| {
                    TextMatch::parse(text).map_err(|e| RuleError::InvalidName(e.to_string()))
                })
                .transpose()?,
            biomes: props.get("biomes").map(BiomeFilter::parse),
            heights: props
                .get("heights")
                .and_then(|text| RangeList::parse(text).ok()),
            large: boolean(&props, "large"),
            trapped: boolean(&props, "trapped"),
            christmas: boolean(&props, "christmas"),
            ender: boolean(&props, "ender"),
            levels: props
                .get("levels")
                .and_then(|text| RangeList::parse(text).ok()),
            professions: professions(&props)?,
            variants: words(&props, "variants", container.variants())?,
            colors: words(&props, "colors", &COLORS)?,
        })
    }

    /// The replacement for the vanilla picture `region`, if this rule has one.
    #[must_use]
    pub fn texture_for(&self, region: &str) -> Option<&PackPath> {
        self.textures
            .iter()
            .find(|(name, _)| name == region)
            .map(|(_, path)| path)
    }

    /// Whether the rule applies to the screen described by `context`.
    #[must_use]
    pub fn matches(&self, context: &GuiContext<'_>) -> bool {
        self.container == context.container
            && self
                .biomes
                .as_ref()
                .is_none_or(|filter| context.biome.is_some_and(|biome| filter.matches(biome)))
            && self.heights.as_ref().is_none_or(|heights| {
                context
                    .height
                    .is_some_and(|y| heights.contains(i64::from(y)))
            })
            && self.name.as_ref().is_none_or(|name| {
                context
                    .name
                    .map_or_else(|| name.matches_absent(), |text| name.matches(text))
            })
            && flag(self.large, context.large)
            && flag(self.trapped, context.trapped)
            && flag(self.christmas, context.christmas)
            && flag(self.ender, context.ender)
            && self.levels.as_ref().is_none_or(|levels| {
                context
                    .levels
                    .is_some_and(|n| levels.contains(i64::from(n)))
            })
            && self.professions.as_ref().is_none_or(|list| {
                context
                    .profession
                    .is_some_and(|(id, career)| list.iter().any(|p| p.matches(id, career)))
            })
            && word(self.variants.as_deref(), context.variant)
            && word(self.colors.as_deref(), context.color)
    }
}

/// A boolean condition: no condition holds; a condition needs the fact.
fn flag(wanted: Option<bool>, actual: Option<bool>) -> bool {
    wanted.is_none_or(|wanted| actual == Some(wanted))
}

/// A word-list condition: no condition holds; a condition needs a listed word.
fn word(list: Option<&[String]>, actual: Option<&str>) -> bool {
    list.is_none_or(|list| actual.is_some_and(|word| list.iter().any(|w| w == word)))
}

/// `true` or `false`; anything else is no condition.
fn boolean(props: &Properties, key: &str) -> Option<bool> {
    match props.get(key)?.to_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// `texture` (the container's default picture) and every `texture.<path>`
/// (a picture below `textures/gui/`, with or without `.png`).
fn textures(props: &Properties, container: Container, base: &str) -> Vec<(String, PackPath)> {
    let mut found = Vec::new();
    if let (Some(path), Some(region)) = (props.get("texture"), container.default_region()) {
        found.push((region.to_string(), resolve_path(base, path, ".png")));
    }
    for key in props.keys() {
        let Some(target) = key.strip_prefix("texture.") else {
            continue;
        };
        let region = target
            .replace('\\', "/")
            .trim_matches('/')
            .trim_end_matches(".png")
            .to_string();
        if let (false, Some(path)) = (region.is_empty(), props.get(key)) {
            found.push((region, resolve_path(base, path, ".png")));
        }
    }
    found
}

/// `None` without the key; an unknown entry is an error.
fn professions(props: &Properties) -> Result<Option<Vec<Profession>>, RuleError> {
    let Some(text) = props.get("professions") else {
        return Ok(None);
    };
    let list = text
        .split(' ')
        .filter(|entry| !entry.is_empty())
        .map(Profession::parse)
        .collect::<Option<Vec<_>>>()
        .ok_or(RuleError::InvalidList("professions"))?;
    Ok((!list.is_empty()).then_some(list))
}

/// A list of lowercase words that must all be in `allowed`.
fn words(
    props: &Properties,
    key: &'static str,
    allowed: &[&str],
) -> Result<Option<Vec<String>>, RuleError> {
    let Some(text) = props.get(key) else {
        return Ok(None);
    };
    let list: Vec<String> = text
        .to_lowercase()
        .split(' ')
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect();
    if list.iter().any(|word| !allowed.contains(&word.as_str())) {
        return Err(RuleError::InvalidList(key));
    }
    Ok(Some(list))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<GuiRule, RuleError> {
        GuiRule::parse(text.as_bytes(), "gui/container/chest/blue.properties")
    }

    fn chest() -> GuiContext<'static> {
        GuiContext::of(Container::Chest)
    }

    #[test]
    fn textures_name_vanilla_regions() {
        let rule = parse(
            "container=chest\ntexture=blue\ntexture.container/creative_inventory/tabs=./tabs\ntexture.\\widgets.png=w\n",
        )
        .expect("valid");
        assert_eq!(
            rule.texture_for("container/generic_54"),
            Some(&PackPath::Feature(
                "gui/container/chest/blue.png".to_string()
            ))
        );
        assert!(rule
            .texture_for("container/creative_inventory/tabs")
            .is_some());
        assert!(rule.texture_for("widgets").is_some());
        assert!(rule.texture_for("container/furnace").is_none());
    }

    #[test]
    fn creative_needs_path_textures() {
        assert_eq!(
            parse("container=creative\ntexture=x\n").err(),
            Some(RuleError::NoTexture)
        );
        assert!(parse("container=creative\ntexture.container/creative_inventory/tabs=x\n").is_ok());
    }

    #[test]
    fn invalid_rules_are_rejected() {
        assert!(matches!(
            parse("texture=x\n"),
            Err(RuleError::UnknownContainer(_))
        ));
        assert!(matches!(
            parse("container=jukebox\ntexture=x\n"),
            Err(RuleError::UnknownContainer(_))
        ));
        assert_eq!(
            parse("container=villager\ntexture=x\nprofessions=wizard\n").err(),
            Some(RuleError::InvalidList("professions"))
        );
        assert_eq!(
            parse("container=dispenser\ntexture=x\nvariants=horse\n").err(),
            Some(RuleError::InvalidList("variants"))
        );
        assert_eq!(
            parse("container=chest\ntexture=x\ncolors=teal\n").err(),
            Some(RuleError::InvalidList("colors"))
        );
        assert!(matches!(
            parse("container=chest\ntexture=x\nname=regex:(\n"),
            Err(RuleError::InvalidName(_))
        ));
    }

    #[test]
    fn the_container_must_match() {
        let rule = parse("container=chest\ntexture=x\n").expect("valid");
        assert!(rule.matches(&chest()));
        assert!(!rule.matches(&GuiContext::of(Container::Furnace)));
    }

    #[test]
    fn biome_height_and_name_conditions() {
        let rule =
            parse("container=chest\ntexture=x\nbiomes=desert\nheights=60-70\nname=ipattern:my *\n")
                .expect("valid");
        let here = GuiContext {
            biome: Some("Desert"),
            height: Some(64),
            name: Some("My Chest"),
            ..chest()
        };
        assert!(rule.matches(&here));
        assert!(!rule.matches(&GuiContext {
            biome: Some("Plains"),
            ..here
        }));
        assert!(!rule.matches(&GuiContext {
            height: Some(80),
            ..here
        }));
        assert!(!rule.matches(&GuiContext { name: None, ..here }));
        assert!(
            !rule.matches(&GuiContext {
                biome: None,
                ..here
            }),
            "unknown biome"
        );
    }

    #[test]
    fn a_negated_name_matches_an_unnamed_container() {
        let rule = parse("container=chest\ntexture=x\nname=!Trap\n").expect("valid");
        assert!(rule.matches(&chest()));
        assert!(!rule.matches(&GuiContext {
            name: Some("Trap"),
            ..chest()
        }));
    }

    #[test]
    fn chest_flags_need_the_fact() {
        let rule = parse("container=chest\ntexture=x\nlarge=true\nender=false\n").expect("valid");
        let big = GuiContext {
            large: Some(true),
            ender: Some(false),
            ..chest()
        };
        assert!(rule.matches(&big));
        assert!(!rule.matches(&GuiContext {
            large: Some(false),
            ..big
        }));
        assert!(!rule.matches(&chest()), "facts unknown");
        assert!(parse("container=chest\ntexture=x\nlarge=maybe\n")
            .expect("valid")
            .matches(&chest()));
    }

    #[test]
    fn beacon_levels_villager_professions_and_words() {
        let beacon = parse("container=beacon\ntexture=x\nlevels=3-4\n").expect("valid");
        let at = |levels| GuiContext {
            levels,
            ..GuiContext::of(Container::Beacon)
        };
        assert!(beacon.matches(&at(Some(4))));
        assert!(!beacon.matches(&at(Some(1))));
        assert!(!beacon.matches(&at(None)));

        let villager =
            parse("container=villager\ntexture=x\nprofessions=librarian farmer:fisherman\n")
                .expect("valid");
        let with = |profession| GuiContext {
            profession,
            ..GuiContext::of(Container::Villager)
        };
        assert!(villager.matches(&with(Some((1, None)))));
        assert!(villager.matches(&with(Some((0, Some(2))))));
        assert!(!villager.matches(&with(Some((0, Some(1))))));
        assert!(!villager.matches(&with(None)));

        let dropper = parse("container=dispenser\ntexture=x\nvariants=dropper\n").expect("valid");
        let kind = |variant| GuiContext {
            variant,
            ..GuiContext::of(Container::Dispenser)
        };
        assert!(dropper.matches(&kind(Some("dropper"))));
        assert!(!dropper.matches(&kind(Some("dispenser"))));
    }

    proptest::proptest! {
        #[test]
        fn arbitrary_files_never_panic(bytes in proptest::collection::vec(proptest::num::u8::ANY, 0..300)) {
            if let Ok(rule) = GuiRule::parse(&bytes, "gui/container/a.properties") {
                let _ = rule.matches(&chest());
            }
        }

        #[test]
        fn arbitrary_conditions_never_panic(name in ".{0,12}", heights in "[0-9 -]{0,10}", biomes in ".{0,10}", levels in "[0-9 -]{0,10}") {
            let text = format!("container=chest\ntexture=x\nname={name}\nheights={heights}\nbiomes={biomes}\nlevels={levels}\n");
            if let Ok(rule) = parse(&text) {
                let _ = rule.matches(&GuiContext { height: Some(5), biome: Some("Plains"), ..chest() });
            }
        }
    }
}

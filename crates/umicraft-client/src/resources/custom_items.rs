// SPDX-License-Identifier: MIT OR Apache-2.0

//! Custom Items (CIT): item pictures that packs choose by item, damage, stack
//! size, enchantment and NBT (F6 phase 7).
//!
//! The rules (`cit/**.properties`, format in `umicraft_packext::cit`) name a
//! replacement picture per item. [`CustomItems::scan`] copies the picture of
//! each `type=item` rule into a small synthetic pack, together with a
//! generated item model that uses it as `layer0`, as OptiFine does
//! (`CustomItemProperties.makeModelBlock`):
//! - `textures/items/optifine_cit/<n>.png` (and its `.mcmeta`, so animated
//!   pictures keep working);
//! - `models/item/optifine_cit_<n>.json`, parent `item/generated`.
//!
//! The atlas then holds the picture, and `ItemResources` bakes the models.
//! [`CustomItems::matching`] finds the first rule for a stack.
//!
//! Not done: `type=armor` and `type=enchantment` rules are read but not used
//! (the armour texture path and the glint renderer have no hook yet), the
//! `texture.<key>` pictures of bows and rods, and the potion and leather
//! overlays of a replaced item (a replaced picture has one layer).
//! Pictures must sit in a pack-feature folder, as for the custom GUIs.

use umicraft_assets::skin::decode_png_bytes;
use umicraft_assets::{PackFiles, ResourcePack};
use umicraft_game::items;
use umicraft_game::nbt::Nbt;
use umicraft_packext::cem::PackPath;
use umicraft_packext::cit::{CitKind, CitSet, ItemQuery, Names};

use super::game_files::PackFeatures;

/// The name prefix of the generated models and pictures.
const NAME_PREFIX: &str = "optifine_cit_";
/// The enchanted book keeps its enchantments under another tag.
const ENCHANTED_BOOK: u16 = 403;

/// The 1.8 enchantments by registry name and id (`Enchantment.effectId`).
const ENCHANTMENTS: [(&str, u16); 25] = [
    ("protection", 0),
    ("fire_protection", 1),
    ("feather_falling", 2),
    ("blast_protection", 3),
    ("projectile_protection", 4),
    ("respiration", 5),
    ("aqua_affinity", 6),
    ("thorns", 7),
    ("depth_strider", 8),
    ("sharpness", 16),
    ("smite", 17),
    ("bane_of_arthropods", 18),
    ("knockback", 19),
    ("fire_aspect", 20),
    ("looting", 21),
    ("efficiency", 32),
    ("silk_touch", 33),
    ("unbreaking", 34),
    ("fortune", 35),
    ("power", 48),
    ("punch", 49),
    ("flame", 50),
    ("infinity", 51),
    ("luck_of_the_sea", 61),
    ("lure", 62),
];

/// The rules of the packs and the models generated for them. Empty when the
/// option is off or no pack brings a rule.
#[derive(Debug, Default)]
pub struct CustomItems {
    set: CitSet,
    /// The generated model name of each rule of the set, `None` for a rule
    /// that is not a usable `type=item` rule.
    models: Vec<Option<String>>,
}

impl CustomItems {
    /// Reads the rules of `files` and returns them with the pack that carries
    /// their pictures and models (`None` when there are none).
    #[must_use]
    pub fn scan(files: &PackFiles, features: PackFeatures) -> (Self, Option<ResourcePack>) {
        if !features.custom_items {
            return (Self::default(), None);
        }
        let listed = files.list_feature("cit/");
        let global = files.feature("cit.properties").map(|file| file.bytes);
        let names = Names {
            item: &item_id,
            enchantment: &enchantment_id,
        };
        let set = CitSet::parse(
            global,
            listed
                .iter()
                .map(|(relative, file)| (relative.as_str(), file.bytes)),
            &names,
        );
        let mut generated = std::collections::BTreeMap::new();
        let models = set
            .rules()
            .iter()
            .enumerate()
            .map(|(index, rule)| {
                let source = rule
                    .texture
                    .as_ref()
                    .filter(|_| rule.kind == CitKind::Item)?;
                generate(files, source, index, &mut generated)
                    .map_err(|reason| {
                        tracing::warn!(rule = %rule.name, reason, "custom item picture ignored");
                    })
                    .ok()
            })
            .collect();
        let pack = (!generated.is_empty())
            .then(|| ResourcePack::from_parts("optifine-custom-items", None, None, generated));
        (Self { set, models }, pack)
    }

    /// Whether no rule was read.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// The rule index of the first `type=item` rule that matches the stack,
    /// `None` when none does or its picture was unusable.
    #[must_use]
    pub fn matching(
        &self,
        item_id: i16,
        damage: i16,
        count: u8,
        nbt: Option<&[u8]>,
    ) -> Option<usize> {
        let id = u16::try_from(item_id).ok()?;
        if !self.set.has_item(id) {
            return None;
        }
        let definition = items::lookup(i32::from(id));
        let tag = nbt.and_then(|bytes| Nbt::parse(bytes).ok());
        let enchantments = tag
            .as_ref()
            .map_or_else(Vec::new, |root| enchantments(root, id));
        let lookup = |path: &[&str]| {
            let mut found = Vec::new();
            if let Some(root) = &tag {
                collect(root, path, &mut found);
            }
            found
        };
        let query = ItemQuery {
            item_id: id,
            damage: i32::from(damage),
            max_damage: definition.map_or(0, |def| i32::from(def.max_damage)),
            stack_size: i32::from(count),
            enchantments: &enchantments,
            nbt: &lookup,
        };
        self.set
            .find_index(CitKind::Item, &query)
            .filter(|index| self.models.get(*index).is_some_and(Option::is_some))
    }

    /// The name of the generated model of rule `index`, for
    /// `ItemModelSet::model_by_name`.
    #[must_use]
    pub fn model_name(&self, index: usize) -> Option<&str> {
        self.models.get(index)?.as_deref()
    }
}

/// Adds the picture, its metadata and the model of rule `index` to
/// `generated`; the model name when it worked.
fn generate(
    files: &PackFiles,
    source: &PackPath,
    index: usize,
    generated: &mut std::collections::BTreeMap<String, Vec<u8>>,
) -> Result<String, &'static str> {
    let (bytes, meta_path) = match source {
        PackPath::Feature(path) => files
            .feature(path)
            .map(|file| (file.bytes, format!("{path}.mcmeta"))),
        PackPath::Resource(path) => files
            .get(&format!("assets/minecraft/{path}"))
            .map(|bytes| (bytes, format!("{path}.mcmeta"))),
    }
    .ok_or("picture not found")?;
    let (_, width, height) = decode_png_bytes(bytes).map_err(|_| "picture does not decode")?;
    if width == 0 || !(height == width || (height > width && height % width == 0)) {
        return Err("picture is not square or a strip of squares");
    }
    let name = format!("{NAME_PREFIX}{index}");
    let sprite = format!("assets/minecraft/textures/items/optifine_cit/{index}.png");
    let meta = match source {
        PackPath::Feature(path) => files
            .feature(&format!("{path}.mcmeta"))
            .map(|file| file.bytes),
        PackPath::Resource(_) => files.get(&format!("assets/minecraft/{meta_path}")),
    };
    if let Some(meta) = meta {
        generated.insert(format!("{sprite}.mcmeta"), meta.to_vec());
    }
    generated.insert(sprite, bytes.to_vec());
    generated.insert(
        format!("assets/minecraft/models/item/{name}.json"),
        format!(
            r#"{{"parent":"item/generated","textures":{{"layer0":"items/optifine_cit/{index}"}}}}"#
        )
        .into_bytes(),
    );
    Ok(name)
}

/// The id of an item name such as `minecraft:diamond_sword`.
fn item_id(name: &str) -> Option<u16> {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    items::all()
        .iter()
        .find(|def| def.name == name)
        .map(|def| def.id)
}

/// The id of an enchantment name such as `minecraft:fire_aspect`.
fn enchantment_id(name: &str) -> Option<u16> {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    ENCHANTMENTS
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, id)| *id)
}

/// The `(id, level)` pairs of a stack: its `ench` list, or the
/// `StoredEnchantments` of an enchanted book.
fn enchantments(root: &Nbt, item_id: u16) -> Vec<(i32, i32)> {
    let key = if item_id == ENCHANTED_BOOK {
        "StoredEnchantments"
    } else {
        "ench"
    };
    let read = |entry: &Nbt, field| entry.get(field).and_then(Nbt::as_int).unwrap_or(0);
    root.get(key)
        .and_then(Nbt::as_list)
        .map(|list| {
            list.iter()
                .map(|e| (read(e, "id"), read(e, "lvl")))
                .collect()
        })
        .unwrap_or_default()
}

/// The values at `path` below `node`, as text; `*` stands for every child.
fn collect(node: &Nbt, path: &[&str], found: &mut Vec<String>) {
    let Some((head, rest)) = path.split_first() else {
        found.extend(scalar(node));
        return;
    };
    match (node, *head) {
        (Nbt::Compound(entries), "*") => entries
            .iter()
            .for_each(|(_, child)| collect(child, rest, found)),
        (Nbt::List(items), "*") => items.iter().for_each(|child| collect(child, rest, found)),
        (Nbt::Compound(_), key) => {
            if let Some(child) = node.get(key) {
                collect(child, rest, found);
            }
        }
        (Nbt::List(items), index) => {
            if let Some(child) = index.parse::<usize>().ok().and_then(|i| items.get(i)) {
                collect(child, rest, found);
            }
        }
        _ => {}
    }
}

/// A tag's value as the text the conditions compare with.
fn scalar(node: &Nbt) -> Option<String> {
    Some(match node {
        Nbt::String(text) => text.clone(),
        Nbt::Byte(n) => n.to_string(),
        Nbt::Short(n) => n.to_string(),
        Nbt::Int(n) => n.to_string(),
        Nbt::Long(n) => n.to_string(),
        Nbt::Float(n) => n.to_string(),
        Nbt::Double(n) => n.to_string(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::resources::game_files::test_support::png;

    const SWORD: i16 = 276;

    fn files(entries: &[(&str, Vec<u8>)]) -> PackFiles {
        let map = entries
            .iter()
            .map(|(path, bytes)| (format!("assets/minecraft/{path}"), bytes.clone()))
            .collect();
        PackFiles::from_packs([ResourcePack::from_parts("cit", None, None, map)])
    }

    fn scan(entries: &[(&str, Vec<u8>)]) -> (CustomItems, Option<ResourcePack>) {
        CustomItems::scan(&files(entries), PackFeatures::ALL)
    }

    /// An NBT compound `{display:{Name:"<name>"}, ench:[{id:<id>,lvl:<lvl>}]}`.
    fn tag(name: &str, enchantment: (i16, i16)) -> Vec<u8> {
        let nbt = Nbt::Compound(vec![
            (
                "display".to_string(),
                Nbt::Compound(vec![("Name".to_string(), Nbt::String(name.to_string()))]),
            ),
            (
                "ench".to_string(),
                Nbt::List(vec![Nbt::Compound(vec![
                    ("id".to_string(), Nbt::Short(enchantment.0)),
                    ("lvl".to_string(), Nbt::Short(enchantment.1)),
                ])]),
            ),
        ]);
        nbt.to_root_bytes().expect("serialises")
    }

    #[test]
    fn a_rule_generates_a_picture_and_a_model() {
        let (items, pack) = scan(&[
            (
                "optifine/cit/blade.properties",
                b"items=diamond_sword\ntexture=blade\n".to_vec(),
            ),
            ("optifine/cit/blade.png", png(16, 16)),
            (
                "optifine/cit/blade.png.mcmeta",
                br#"{"animation":{}}"#.to_vec(),
            ),
        ]);
        let pack: BTreeMap<_, _> = pack.expect("a pack").files().clone();
        assert!(pack.contains_key("assets/minecraft/textures/items/optifine_cit/0.png"));
        assert!(pack.contains_key("assets/minecraft/textures/items/optifine_cit/0.png.mcmeta"));
        let model = &pack["assets/minecraft/models/item/optifine_cit_0.json"];
        assert!(std::str::from_utf8(model)
            .expect("utf8")
            .contains("items/optifine_cit/0"));
        let rule = items.matching(SWORD, 0, 1, None).expect("matches");
        assert_eq!(items.model_name(rule), Some("optifine_cit_0"));
        assert_eq!(items.matching(280, 0, 1, None), None, "another item");
    }

    #[test]
    fn damage_and_stack_size_choose_the_rule() {
        let (items, _) = scan(&[
            (
                "optifine/cit/worn.properties",
                b"items=276\ndamage=100-\nweight=2\ntexture=pic\n".to_vec(),
            ),
            (
                "optifine/cit/many.properties",
                b"items=276\nstackSize=2-\ntexture=pic\n".to_vec(),
            ),
            ("optifine/cit/pic.png", png(16, 16)),
        ]);
        let name = |damage, count| {
            items
                .matching(SWORD, damage, count, None)
                .and_then(|index| items.model_name(index))
                .map(str::to_string)
        };
        assert_eq!(
            name(150, 1).as_deref(),
            Some("optifine_cit_0"),
            "worn, the heavier rule"
        );
        assert_eq!(name(0, 5).as_deref(), Some("optifine_cit_1"), "many");
        assert_eq!(name(0, 1), None);
    }

    #[test]
    fn nbt_names_and_enchantments_are_read_from_the_stack() {
        let (items, _) = scan(&[
            (
                "optifine/cit/named.properties",
                b"items=276\ntexture=pic\nnbt.display.Name=Excalibur\nenchantments=sharpness\nenchantmentLevels=5-\n".to_vec(),
            ),
            ("optifine/cit/pic.png", png(16, 16)),
        ]);
        let good = tag("Excalibur", (16, 5));
        assert!(items.matching(SWORD, 0, 1, Some(&good)).is_some());
        assert!(items
            .matching(SWORD, 0, 1, Some(&tag("Excalibur", (16, 2))))
            .is_none());
        assert!(items
            .matching(SWORD, 0, 1, Some(&tag("Other", (16, 5))))
            .is_none());
        assert!(items.matching(SWORD, 0, 1, None).is_none());
        assert!(items.matching(SWORD, 0, 1, Some(b"garbage")).is_none());
    }

    #[test]
    fn a_missing_or_unusable_picture_drops_the_rule() {
        let (items, pack) = scan(&[
            (
                "optifine/cit/gone.properties",
                b"items=276\ntexture=gone\n".to_vec(),
            ),
            (
                "optifine/cit/wide.properties",
                b"items=280\ntexture=wide\n".to_vec(),
            ),
            ("optifine/cit/wide.png", png(32, 16)),
        ]);
        assert!(pack.is_none());
        assert!(items.matching(SWORD, 0, 1, None).is_none());
        assert!(items.matching(280, 0, 1, None).is_none());
    }

    #[test]
    fn armour_and_enchantment_rules_make_no_item_model() {
        let (items, pack) = scan(&[
            (
                "optifine/cit/worn.properties",
                b"type=armor\nitems=276\ntexture.x=y\n".to_vec(),
            ),
            (
                "optifine/cit/glint.properties",
                b"type=enchantment\nenchantmentIDs=16\ntexture=pic\n".to_vec(),
            ),
            ("optifine/cit/pic.png", png(16, 16)),
        ]);
        assert!(pack.is_none());
        assert!(!items.is_empty());
        assert!(items.matching(SWORD, 0, 1, None).is_none());
    }

    #[test]
    fn the_option_off_reads_nothing() {
        let off = PackFeatures {
            custom_items: false,
            ..PackFeatures::ALL
        };
        let files = files(&[
            (
                "optifine/cit/blade.properties",
                b"items=276\ntexture=pic\n".to_vec(),
            ),
            ("optifine/cit/pic.png", png(16, 16)),
        ]);
        let (items, pack) = CustomItems::scan(&files, off);
        assert!(items.is_empty() && pack.is_none());
    }

    #[test]
    fn names_resolve_through_the_registries() {
        assert_eq!(item_id("minecraft:diamond_sword"), Some(276));
        assert_eq!(item_id("no_such_item"), None);
        assert_eq!(enchantment_id("minecraft:fire_aspect"), Some(20));
        assert_eq!(enchantment_id("nothing"), None);
    }

    #[test]
    fn wildcard_paths_read_every_child() {
        let lore = Nbt::Compound(vec![(
            "display".to_string(),
            Nbt::Compound(vec![(
                "Lore".to_string(),
                Nbt::List(vec![Nbt::String("a".into()), Nbt::String("b".into())]),
            )]),
        )]);
        let mut found = Vec::new();
        collect(&lore, &["display", "Lore", "*"], &mut found);
        assert_eq!(found, ["a", "b"]);
        found.clear();
        collect(&lore, &["display", "Lore", "1"], &mut found);
        assert_eq!(found, ["b"]);
    }
}

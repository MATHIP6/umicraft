// SPDX-License-Identifier: MIT OR Apache-2.0

//! Random Entities: mobs that wear one of several textures from a pack
//! (F6 phase 7, ADR-0089).
//!
//! [`RandomEntities::load`] finds, for every entity texture of the table in
//! `specs`, the alternatives the packs bring under `mob/` or `random/entity/`
//! (the format is described in `umicraft_packext::random_entities`), decodes
//! them, and returns them as pictures for the registry to upload under
//! [`TextureSlot::Random`]. [`RandomEntities::retarget`] then swaps the
//! texture of the layers of one mob for its own variant.
//!
//! Which variant a mob wears is a function of its [`random_id`] and of the
//! biome and height where it was first seen, so it never changes while the
//! mob is around: the choice is recorded at first sight and dropped when the
//! mob leaves the snapshot list.
//!
//! The option is read at load time (`GameFiles::features`); a change of the
//! option reloads the resources, which rebuilds this set.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};

use umicraft_game::entity::meta::Variant;
use umicraft_game::entity::{EntityId, EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::TextureSlot;
use umicraft_packext::random_entities::{
    probe_variants, properties_paths, random_id, variant_paths, RandomRules, RandomSkins, Spawn,
    VANILLA,
};

use super::mobs::EntityLayer;
use super::specs::texture_specs;
use crate::resources::game_files::GameFiles;

/// Where the vanilla entity textures sit inside `assets/minecraft/`.
const ENTITY_FOLDER: &str = "assets/minecraft/";

/// Decoded RGBA pixels with their width and height.
type Picture = (Vec<u8>, u32, u32);

/// One decoded alternative texture, for the registry to upload.
pub(super) struct RandomImage {
    /// The slot the layers use.
    pub(super) slot: TextureSlot,
    /// Whether the texture repeats past its edge, like its vanilla one.
    pub(super) repeats: bool,
    /// RGBA pixels.
    pub(super) rgba: Vec<u8>,
    /// Width in pixels.
    pub(super) width: u32,
    /// Height in pixels.
    pub(super) height: u32,
}

/// The textures one vanilla texture may be replaced by.
struct SlotVariants {
    skins: RandomSkins,
    /// The slot of each texture number above 1; number 1 stays the vanilla
    /// slot.
    slots: BTreeMap<u32, TextureSlot>,
}

/// What is known about a mob from the first time it was seen.
struct FirstSight {
    biome: Option<String>,
    height: i32,
    /// The frame this mob was last in the snapshot list, to drop the records
    /// of mobs that left.
    seen: u64,
}

/// The alternative textures of the packs and the sight records of the mobs.
#[derive(Default)]
pub(crate) struct RandomEntities {
    variants: HashMap<TextureSlot, SlotVariants>,
    sights: RefCell<HashMap<EntityId, FirstSight>>,
    frame: Cell<u64>,
}

impl RandomEntities {
    /// Reads the alternatives of `files` and returns them with their pictures.
    /// Empty when the option is off or no pack brings any.
    pub(super) fn load(files: &GameFiles) -> (Self, Vec<RandomImage>) {
        let mut set = Self::default();
        let mut images = Vec::new();
        if !files.features().random_entities || files.packs().is_empty() {
            return (set, images);
        }
        for spec in texture_specs() {
            let Some(texture) = spec.jar_path.strip_prefix(ENTITY_FOLDER) else {
                continue;
            };
            let loader = VariantLoader::new(files, texture);
            let Some(skins) = loader.skins() else {
                continue;
            };
            let mut slots = BTreeMap::new();
            for (number, (rgba, width, height)) in loader.into_pictures() {
                let slot = TextureSlot::Random(images.len() as u32);
                slots.insert(number, slot);
                images.push(RandomImage {
                    slot,
                    repeats: spec.slot.repeats(),
                    rgba,
                    width,
                    height,
                });
            }
            set.variants
                .insert(spec.slot, SlotVariants { skins, slots });
        }
        (set, images)
    }

    /// Whether any texture has alternatives.
    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.variants.is_empty()
    }

    /// Records the mobs of `snapshots` that were not seen yet, with the
    /// biome `biome_at` gives for their column, and forgets the ones that are
    /// gone. Call once per frame before meshing; without it a mob is recorded
    /// on first use, with no biome.
    pub(crate) fn observe(
        &self,
        snapshots: &[EntityRenderSnapshot],
        biome_at: impl Fn(i32, i32) -> Option<String>,
    ) {
        if self.variants.is_empty() {
            return;
        }
        let frame = self.frame.get() + 1;
        self.frame.set(frame);
        let mut sights = self.sights.borrow_mut();
        for entity in snapshots.iter().filter(|entity| is_eligible(entity)) {
            sights
                .entry(entity.id)
                .or_insert_with(|| first_sight(entity, &biome_at))
                .seen = frame;
        }
        sights.retain(|_, sight| sight.seen == frame);
    }

    /// Swaps the texture of the layers of `entity` for its variant.
    pub(super) fn retarget(&self, entity: &EntityRenderSnapshot, layers: &mut [EntityLayer]) {
        if self.variants.is_empty() || !is_eligible(entity) {
            return;
        }
        let mut sights = self.sights.borrow_mut();
        let sight = sights
            .entry(entity.id)
            .or_insert_with(|| first_sight(entity, &|_, _| None));
        let spawn = Spawn {
            biome: sight.biome.as_deref(),
            height: Some(sight.height),
            name: entity.meta.custom_name.as_deref(),
            profession: match entity.meta.variant {
                Variant::Villager { profession } => Some(profession),
                _ => None,
            },
            career: None,
        };
        let id = random_id(entity.uuid, mix(entity.id));
        for layer in layers {
            if let Some(variants) = self.variants.get(&layer.slot) {
                let number = variants.skins.pick(&spawn, id);
                if let Some(slot) = variants.slots.get(&number) {
                    layer.slot = *slot;
                }
            }
        }
    }
}

/// Mobs wear random textures, players their skins and objects their own.
fn is_eligible(entity: &EntityRenderSnapshot) -> bool {
    entity.kind.is_living() && entity.kind != EntityKind::Player
}

/// The record of `entity` at the first frame it is seen.
fn first_sight(
    entity: &EntityRenderSnapshot,
    biome_at: &impl Fn(i32, i32) -> Option<String>,
) -> FirstSight {
    let x = (entity.origin.x + f64::from(entity.position.x)).floor() as i32;
    let y = (entity.origin.y + f64::from(entity.position.y)).floor() as i32;
    let z = (entity.origin.z + f64::from(entity.position.z)).floor() as i32;
    FirstSight {
        biome: biome_at(x, z),
        height: y,
        seen: 0,
    }
}

/// A well-mixed number from the network id: the 1.8 protocol gives a mob no
/// UUID, and vanilla draws one at random on the client, so any stable
/// per-entity number is as good (SplitMix64 finaliser, so neighbouring ids do
/// not alternate textures).
fn mix(id: EntityId) -> u64 {
    let mut z = (id.0 as u64).wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Finds and decodes the alternatives of one vanilla texture.
struct VariantLoader<'a> {
    files: &'a GameFiles,
    texture: &'a str,
    /// The decoded pictures by number, `None` for a number checked and
    /// unusable.
    pictures: RefCell<BTreeMap<u32, Option<Picture>>>,
}

impl<'a> VariantLoader<'a> {
    fn new(files: &'a GameFiles, texture: &'a str) -> Self {
        Self {
            files,
            texture,
            pictures: RefCell::default(),
        }
    }

    /// The choice rules for the texture, or `None` when the packs bring no
    /// alternative.
    fn skins(&self) -> Option<RandomSkins> {
        let rules = self.rules();
        let skins = match rules {
            Some(rules) => RandomSkins::from_rules(rules),
            None => RandomSkins::from_variants(probe_variants(|number| self.available(number))),
        };
        skins.is_useful().then_some(skins)
    }

    /// The rules of the first properties file that parses.
    fn rules(&self) -> Option<RandomRules> {
        let packs = self.files.packs();
        let (relative, file) = properties_paths(self.texture)
            .into_iter()
            .find_map(|relative| packs.feature(&relative).map(|file| (relative, file)))?;
        RandomRules::parse(file.bytes, |number| {
            number == VANILLA || self.available(number)
        })
        .map_err(|error| tracing::warn!(path = %relative, %error, "random entity rules ignored"))
        .ok()
    }

    /// Whether variant `number` exists and decodes. Decoded once.
    fn available(&self, number: u32) -> bool {
        self.pictures
            .borrow_mut()
            .entry(number)
            .or_insert_with(|| self.decode(number))
            .is_some()
    }

    fn decode(&self, number: u32) -> Option<Picture> {
        let packs = self.files.packs();
        let file = variant_paths(self.texture, number)
            .into_iter()
            .find_map(|relative| packs.feature(&relative))?;
        umicraft_assets::skin::decode_png_bytes(file.bytes)
            .map_err(|error| tracing::warn!(path = file.path, %error, "random entity texture does not decode"))
            .ok()
    }

    /// The decoded pictures that were found.
    fn into_pictures(self) -> impl Iterator<Item = (u32, Picture)> {
        self.pictures
            .into_inner()
            .into_iter()
            .filter_map(|(number, picture)| Some((number, picture?)))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use umicraft_assets::{PackFiles, ResourcePack};

    use super::*;
    use crate::resources::game_files::{test_support::png, PackFeatures};
    use crate::scene::entity::mobs::build_entity_layers_for;
    use crate::scene::entity::skins::SkinLookup;
    use crate::scene::entity::Viewer;

    /// A world of textures that only has random entities.
    struct World(RandomEntities);

    impl SkinLookup for World {
        fn skin(&self, _url: &str) -> Option<u32> {
            None
        }

        fn cape(&self, _url: &str) -> Option<u32> {
            None
        }

        fn random_entities(&self) -> Option<&RandomEntities> {
            Some(&self.0)
        }
    }

    fn files(entries: &[(&str, Vec<u8>)], features: PackFeatures) -> GameFiles {
        let map: BTreeMap<String, Vec<u8>> = entries
            .iter()
            .map(|(path, bytes)| (format!("assets/minecraft/{path}"), bytes.clone()))
            .collect();
        let pack = ResourcePack::from_parts("test", None, None, map);
        GameFiles::new(PackFiles::from_packs([pack]), None).with_features(features)
    }

    fn world(entries: &[(&str, Vec<u8>)]) -> (World, usize) {
        let (set, images) = RandomEntities::load(&files(entries, PackFeatures::ALL));
        (World(set), images.len())
    }

    fn creeper(id: i32) -> EntityRenderSnapshot {
        EntityRenderSnapshot::new(EntityId(id), EntityKind::Creeper, glam::Vec3::ZERO)
    }

    fn body_slot(world: &World, entity: &EntityRenderSnapshot) -> TextureSlot {
        build_entity_layers_for(entity, world, Viewer::default())[0].slot
    }

    const MOB: &str = "mcpatcher/mob/creeper/";

    #[test]
    fn variants_without_rules_are_spread_over_the_mobs() {
        let (world, count) = world(&[
            (&format!("{MOB}creeper2.png"), png(64, 32)),
            (&format!("{MOB}creeper3.png"), png(64, 32)),
        ]);
        assert_eq!(count, 2);
        let slots: std::collections::HashSet<_> =
            (0..64).map(|id| body_slot(&world, &creeper(id))).collect();
        assert_eq!(slots.len(), 3, "the vanilla texture and both variants");
        assert!(slots.contains(&TextureSlot::Creeper));
        assert!(slots
            .iter()
            .any(|slot| matches!(slot, TextureSlot::Random(_))));
    }

    #[test]
    fn the_choice_of_a_mob_never_changes() {
        let (world, _) = world(&[(&format!("{MOB}creeper2.png"), png(64, 32))]);
        let first = body_slot(&world, &creeper(7));
        assert!((0..10).all(|_| body_slot(&world, &creeper(7)) == first));
    }

    #[test]
    fn rules_pick_by_height_at_first_sight() {
        let (world, _) = world(&[
            (&format!("{MOB}creeper5.png"), png(64, 32)),
            (
                &format!("{MOB}creeper.properties"),
                b"skins.1=5\nheights.1=0-40\n".to_vec(),
            ),
        ]);
        let mut deep = creeper(1);
        deep.position.y = 20.0;
        let mut surface = creeper(2);
        surface.position.y = 64.0;
        assert!(matches!(body_slot(&world, &deep), TextureSlot::Random(_)));
        assert_eq!(body_slot(&world, &surface), TextureSlot::Creeper);
    }

    #[test]
    fn the_new_folder_and_the_optifine_prefix_work_too() {
        let (world, _) = world(&[("optifine/random/entity/creeper/creeper2.png", png(64, 32))]);
        let slots: std::collections::HashSet<_> =
            (0..32).map(|id| body_slot(&world, &creeper(id))).collect();
        assert_eq!(slots.len(), 2);
    }

    #[test]
    fn a_broken_variant_is_not_used() {
        let (world, count) = world(&[(&format!("{MOB}creeper2.png"), b"not a png".to_vec())]);
        assert_eq!(count, 0);
        assert!(world.0.is_empty());
    }

    #[test]
    fn the_option_off_loads_nothing() {
        let entries = [(format!("{MOB}creeper2.png"), png(64, 32))];
        let pairs: Vec<_> = entries
            .iter()
            .map(|(p, b)| (p.as_str(), b.clone()))
            .collect();
        let off = PackFeatures {
            random_entities: false,
            ..PackFeatures::ALL
        };
        let (set, images) = RandomEntities::load(&files(&pairs, off));
        assert!(set.is_empty() && images.is_empty());
    }

    #[test]
    fn players_keep_their_skin_and_records_follow_the_snapshots() {
        let (world, _) = world(&[(&format!("{MOB}creeper2.png"), png(64, 32))]);
        let mobs = [creeper(1), creeper(2)];
        world.0.observe(&mobs, |_, _| Some("Plains".to_string()));
        assert_eq!(world.0.sights.borrow().len(), 2);
        world.0.observe(&mobs[..1], |_, _| None);
        assert_eq!(world.0.sights.borrow().len(), 1, "the other mob left");
        let player = EntityRenderSnapshot::new(EntityId(9), EntityKind::Player, glam::Vec3::ZERO);
        world.0.observe(&[player], |_, _| None);
        assert!(world.0.sights.borrow().is_empty());
    }

    #[test]
    fn biome_rules_use_the_biome_seen_first() {
        let (world, _) = world(&[
            (&format!("{MOB}creeper2.png"), png(64, 32)),
            (
                &format!("{MOB}creeper.properties"),
                b"skins.1=2\nbiomes.1=Desert\n".to_vec(),
            ),
        ]);
        let mobs = [creeper(1), creeper(2)];
        world.0.observe(&mobs, |_, _| Some("Desert".to_string()));
        assert!(matches!(
            body_slot(&world, &mobs[0]),
            TextureSlot::Random(_)
        ));
        assert!(
            matches!(body_slot(&world, &mobs[1]), TextureSlot::Random(_)),
            "recorded biome is kept"
        );
        world
            .0
            .observe(&mobs[..1], |_, _| Some("Plains".to_string()));
        assert!(matches!(
            body_slot(&world, &mobs[0]),
            TextureSlot::Random(_)
        ));
    }
}

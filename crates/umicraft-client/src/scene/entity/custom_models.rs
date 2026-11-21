// SPDX-License-Identifier: MIT OR Apache-2.0

//! Custom entity models from resource packs (CEM, ADR-0089).
//!
//! [`CustomModels`] reads every `cem/<entity>.jem` of the active packs when
//! the entity textures are (re)loaded, keeps the compiled rigs and their
//! textures, and meshes a mob with its custom model when the "Custom Entity
//! Models" option is on. A mob without a model, a model that fails to load,
//! or the option off all fall back to the hard-coded model.
//!
//! Animation state (`var.*`, `smooth`) is kept per entity, bounded, and
//! dropped when the entity stops being drawn for long enough that the table
//! fills up.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use umicraft_assets::skin::decode_png_bytes;
use umicraft_game::entity::{EntityId, EntityRenderSnapshot};
use umicraft_model::entity::{CemMob, CemRig, ModelVertex, TextureSlot};
use umicraft_packext::cem::{parse_jem, CemState, EntityInputs, PackPath};

use super::mobs::{EntityLayer, Pose};
use crate::resources::game_files::GameFiles;

/// How many entities keep animation state; past it the table starts over,
/// which only resets `smooth` and `var.*` values.
const MAX_STATES: usize = 512;
/// Longest step `smooth` sees, so a stall does not make it jump.
const MAX_DT: f32 = 0.25;

/// A loaded model.
struct LoadedRig {
    rig: CemRig,
    /// The texture the model brings, or `None` to use the vanilla one.
    slot: Option<TextureSlot>,
}

/// One entity's animation state and the time it was last advanced.
struct EntityState {
    rig: Arc<LoadedRig>,
    state: CemState,
    last: Instant,
}

/// The decoded texture of a model, for the registry to upload.
pub(crate) struct CemImage {
    /// The slot the model's layers use.
    pub(crate) slot: TextureSlot,
    /// RGBA pixels.
    pub(crate) rgba: Vec<u8>,
    /// Width in pixels.
    pub(crate) width: u32,
    /// Height in pixels.
    pub(crate) height: u32,
}

/// The custom entity models of the active packs.
#[derive(Default)]
pub(crate) struct CustomModels {
    rigs: HashMap<String, Arc<LoadedRig>>,
    states: RefCell<HashMap<EntityId, EntityState>>,
    enabled: Cell<bool>,
}

/// The mesh of a mob that may have a custom model: what is left of the
/// vanilla model, and the custom parts with their texture.
pub(super) struct Meshes {
    /// Vanilla vertices; all of the model when there is no custom one.
    pub(super) vanilla: Vec<ModelVertex>,
    /// The custom parts and the slot they are drawn with.
    pub(super) custom: Option<(TextureSlot, Vec<ModelVertex>)>,
}

impl Meshes {
    /// Runs `f` on every vertex position, for the scale of a mob.
    pub(super) fn for_each_vertex(&mut self, mut f: impl FnMut(&mut ModelVertex)) {
        let custom = self.custom.iter_mut().flat_map(|(_, v)| v.iter_mut());
        self.vanilla.iter_mut().chain(custom).for_each(&mut f);
    }

    /// Scales every vertex, as the render scale of a mob does.
    pub(super) fn scale(&mut self, factor: f32) {
        self.for_each_vertex(|vertex| vertex.position = vertex.position.map(|c| c * factor));
    }

    /// Takes the layer of the custom parts out, for a caller that builds the
    /// vanilla layers itself (a body with glowing eyes).
    pub(super) fn custom_layer(&mut self) -> Option<EntityLayer> {
        self.custom
            .take()
            .map(|(slot, vertices)| EntityLayer::new(slot, vertices))
    }

    /// The body layers: vanilla on `vanilla_slot`, then the custom parts.
    pub(super) fn into_layers(self, vanilla_slot: TextureSlot) -> Vec<EntityLayer> {
        let mut layers = vec![EntityLayer::new(vanilla_slot, self.vanilla)];
        layers.extend(
            self.custom
                .map(|(slot, vertices)| EntityLayer::new(slot, vertices)),
        );
        layers
    }
}

/// Meshes `model` (already posed), with the custom model of `entity` applied
/// when `custom` has one for it; custom parts without a texture of their own
/// use `slot`, the vanilla texture. `rest` is the same model unposed and `emit`
/// meshes the vanilla remnants. A baby has no custom model (its files are
/// separate in the reference and are not read yet).
pub(super) fn mesh_with_custom<M: CemMob>(
    custom: Option<&CustomModels>,
    ent: &EntityRenderSnapshot,
    entity: &str,
    slot: TextureSlot,
    model: &mut M,
    rest: &M,
    emit: impl FnOnce(&M) -> Vec<ModelVertex>,
) -> Meshes {
    let applied = custom
        .filter(|_| !ent.meta.baby)
        .and_then(|library| library.apply(ent, entity, slot, model, rest));
    Meshes {
        vanilla: emit(model),
        custom: applied,
    }
}

impl CustomModels {
    /// Reads the models of `files`. Returns the set and the textures to
    /// upload. A file that does not parse is skipped with a warning.
    pub(crate) fn load(files: &GameFiles) -> (Self, Vec<CemImage>) {
        let packs = files.packs();
        let read = |path: &str| packs.feature(path).map(|f| f.bytes.to_vec());
        let mut models = Self::default();
        let mut images = Vec::new();
        for (relative, file) in packs.list_feature("cem/") {
            if !relative.ends_with(".jem") {
                continue;
            }
            let model = match parse_jem(&relative, file.bytes, &read) {
                Ok(model) => model,
                Err(error) => {
                    tracing::warn!(path = %relative, %error, "custom entity model ignored");
                    continue;
                }
            };
            if models.rigs.contains_key(&model.entity) {
                tracing::warn!(path = %relative, "a second model for the same entity is ignored");
                continue;
            }
            let slot = model
                .texture
                .as_ref()
                .and_then(|path| load_texture(files, path))
                .map(|(rgba, width, height)| {
                    let slot = TextureSlot::Cem(images.len() as u32);
                    images.push(CemImage {
                        slot,
                        rgba,
                        width,
                        height,
                    });
                    slot
                });
            let entity = model.entity.clone();
            let rig = Arc::new(LoadedRig {
                rig: CemRig::new(model),
                slot,
            });
            models.rigs.insert(entity, rig);
        }
        (models, images)
    }

    /// Switches the models on or off ("Custom Entity Models").
    pub(crate) fn set_enabled(&self, enabled: bool) {
        self.enabled.set(enabled);
    }

    /// How many entity models were loaded.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.rigs.len()
    }

    /// Applies the model of `entity` to the posed `model`; the vertices of the
    /// custom parts and their slot, or `None` when there is no model or the
    /// option is off.
    fn apply<M: CemMob>(
        &self,
        ent: &EntityRenderSnapshot,
        entity: &str,
        fallback: TextureSlot,
        model: &mut M,
        rest: &M,
    ) -> Option<(TextureSlot, Vec<ModelVertex>)> {
        if !self.enabled.get() {
            return None;
        }
        let loaded = self.rigs.get(entity)?;
        let inputs = entity_inputs(ent);
        let mut states = self.states.borrow_mut();
        if states.len() >= MAX_STATES && !states.contains_key(&ent.id) {
            states.clear();
        }
        let now = Instant::now();
        let entry = states.entry(ent.id).or_insert_with(|| EntityState {
            rig: Arc::clone(loaded),
            state: loaded.rig.new_state(ent.id.0 as u32),
            last: now,
        });
        if !Arc::ptr_eq(&entry.rig, loaded) {
            *entry = EntityState {
                rig: Arc::clone(loaded),
                state: loaded.rig.new_state(ent.id.0 as u32),
                last: now,
            };
        }
        let dt = now.duration_since(entry.last).as_secs_f32().min(MAX_DT);
        entry.last = now;
        let vertices = loaded.rig.apply(model, rest, &mut entry.state, &inputs, dt);
        Some((loaded.slot.unwrap_or(fallback), vertices))
    }
}

/// The picture a model names, from the pack or the jar.
fn load_texture(files: &GameFiles, path: &PackPath) -> Option<(Vec<u8>, u32, u32)> {
    let decoded = match path {
        PackPath::Feature(relative) => {
            let file = files.packs().feature(relative)?;
            decode_png_bytes(file.bytes).ok()
        }
        PackPath::Resource(path) => files.read_png(&format!("assets/minecraft/{path}")),
    };
    if decoded.is_none() {
        tracing::warn!(?path, "custom model texture missing or invalid");
    }
    decoded
}

/// The variables a model's animations read, from a snapshot (public CEM
/// documentation, `cem_animation.txt`). `time` has no world clock to read
/// here, so it is the entity age.
pub(super) fn entity_inputs(ent: &EntityRenderSnapshot) -> EntityInputs {
    let pose = Pose::of(ent, None);
    EntityInputs {
        limb_swing: pose.swing,
        limb_speed: pose.amount,
        age: pose.age,
        head_yaw: pose.head_yaw,
        head_pitch: pose.head_pitch,
        swing_progress: ent.swing_progress,
        hurt_time: if ent.hurt { 10.0 } else { 0.0 },
        death_time: ent.death_time,
        id: ent.id.0 as f32,
        position: ent.position.to_array(),
        time: ent.age,
        is_child: ent.meta.baby,
        is_riding: ent.riding,
        is_sneaking: ent.flags.is_sneaking(),
        is_burning: ent.flags.on_fire(),
        is_hurt: ent.hurt,
        is_alive: ent.death_time <= 0.0,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use umicraft_assets::{PackFiles, ResourcePack};
    use umicraft_game::entity::EntityKind;

    use super::*;
    use crate::resources::game_files::test_support::png;
    use crate::scene::entity::mobs::build_entity_layers;
    use crate::scene::entity::skins::SkinLookup;

    /// A world of textures that has only custom models.
    struct Models(CustomModels);

    impl SkinLookup for Models {
        fn skin(&self, _url: &str) -> Option<u32> {
            None
        }

        fn cape(&self, _url: &str) -> Option<u32> {
            None
        }

        fn custom_models(&self) -> Option<&CustomModels> {
            Some(&self.0)
        }
    }

    const HEAD_ONLY: &str = r#"{"texture":"head.png","textureSize":[16,16],"models":[
        {"part":"head","translate":[0,6,0],
         "boxes":[{"coordinates":[-4,-8,-4,8,8,8],"textureOffset":[0,0]}]}]}"#;

    fn files(entries: &[(&str, Vec<u8>)]) -> GameFiles {
        let map: BTreeMap<String, Vec<u8>> = entries
            .iter()
            .map(|(path, bytes)| (format!("assets/minecraft/optifine/{path}"), bytes.clone()))
            .collect();
        let pack = ResourcePack::from_parts("test", None, None, map);
        GameFiles::new(PackFiles::from_packs([pack]), None)
    }

    fn models(entries: &[(&str, Vec<u8>)]) -> Models {
        let (library, _) = CustomModels::load(&files(entries));
        library.set_enabled(true);
        Models(library)
    }

    fn creeper() -> EntityRenderSnapshot {
        EntityRenderSnapshot::new(EntityId(1), EntityKind::Creeper, glam::Vec3::ZERO)
    }

    fn vertex_count(layers: &[EntityLayer], slot: TextureSlot) -> usize {
        layers
            .iter()
            .filter(|layer| layer.slot == slot)
            .map(|layer| layer.vertices.len())
            .sum()
    }

    #[test]
    fn models_and_their_textures_load_from_the_pack() {
        let (library, images) = CustomModels::load(&files(&[
            ("cem/creeper.jem", HEAD_ONLY.as_bytes().to_vec()),
            ("cem/head.png", png(16, 16)),
        ]));
        assert_eq!(library.len(), 1);
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].slot, TextureSlot::Cem(0));
        assert_eq!((images[0].width, images[0].height), (16, 16));
    }

    #[test]
    fn a_broken_model_is_skipped_and_the_others_load() {
        let (library, _) = CustomModels::load(&files(&[
            ("cem/creeper.jem", b"{ not json".to_vec()),
            ("cem/cow.jem", HEAD_ONLY.as_bytes().to_vec()),
        ]));
        assert_eq!(library.len(), 1);
    }

    #[test]
    fn a_creeper_with_a_custom_head_draws_it_with_the_model_texture() {
        let world = models(&[
            ("cem/creeper.jem", HEAD_ONLY.as_bytes().to_vec()),
            ("cem/head.png", png(16, 16)),
        ]);
        let plain = build_entity_layers(&creeper(), &crate::scene::entity::NoSkins);
        let custom = build_entity_layers(&creeper(), &world);
        assert_eq!(vertex_count(&custom, TextureSlot::Cem(0)), 36);
        // The vanilla head (one box) is gone from the vanilla layer.
        assert_eq!(
            vertex_count(&custom, TextureSlot::Creeper),
            vertex_count(&plain, TextureSlot::Creeper) - 36
        );
    }

    #[test]
    fn a_model_without_a_texture_uses_the_vanilla_one() {
        let jem = HEAD_ONLY.replace(r#""texture":"head.png","#, "");
        let world = models(&[("cem/creeper.jem", jem.into_bytes())]);
        let layers = build_entity_layers(&creeper(), &world);
        assert!(layers
            .iter()
            .all(|layer| layer.slot == TextureSlot::Creeper));
        assert_eq!(layers.len(), 2, "the remnants and the custom head");
    }

    #[test]
    fn the_option_off_gives_the_vanilla_model() {
        let world = models(&[("cem/creeper.jem", HEAD_ONLY.as_bytes().to_vec())]);
        world.0.set_enabled(false);
        let plain = build_entity_layers(&creeper(), &crate::scene::entity::NoSkins);
        let layers = build_entity_layers(&creeper(), &world);
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].vertices.len(), plain[0].vertices.len());
    }

    #[test]
    fn a_baby_keeps_its_vanilla_model() {
        let world = models(&[("cem/cow.jem", HEAD_ONLY.as_bytes().to_vec())]);
        let mut cow = EntityRenderSnapshot::new(EntityId(2), EntityKind::Cow, glam::Vec3::ZERO);
        let adult = build_entity_layers(&cow, &world);
        assert_eq!(adult.len(), 2, "an adult cow uses the model");
        cow.meta.baby = true;
        assert_eq!(build_entity_layers(&cow, &world).len(), 1);
    }

    #[test]
    fn a_mob_the_pack_does_not_cover_is_untouched() {
        let world = models(&[("cem/cow.jem", HEAD_ONLY.as_bytes().to_vec())]);
        let layers = build_entity_layers(&creeper(), &world);
        assert_eq!(layers.len(), 1);
    }

    #[test]
    fn every_covered_mob_takes_its_model() {
        let kinds = [
            (EntityKind::Creeper, "creeper"),
            (EntityKind::Zombie, "zombie"),
            (EntityKind::Skeleton, "skeleton"),
            (EntityKind::ZombiePigman, "zombie_pigman"),
            (EntityKind::Spider, "spider"),
            (EntityKind::Cow, "cow"),
            (EntityKind::Pig, "pig"),
            (EntityKind::Sheep, "sheep"),
            (EntityKind::Chicken, "chicken"),
            (EntityKind::Wolf, "wolf"),
            (EntityKind::Enderman, "enderman"),
            (EntityKind::Villager, "villager"),
            (EntityKind::IronGolem, "iron_golem"),
        ];
        for (kind, name) in kinds {
            let jem = HEAD_ONLY.replace(r#""texture":"head.png","#, r#""texture":"t.png","#);
            let world = models(&[
                (&format!("cem/{name}.jem"), jem.into_bytes()),
                ("cem/t.png", png(16, 16)),
            ]);
            let ent = EntityRenderSnapshot::new(EntityId(3), kind, glam::Vec3::ZERO);
            let layers = build_entity_layers(&ent, &world);
            assert_eq!(
                vertex_count(&layers, TextureSlot::Cem(0)),
                36,
                "{name} draws its custom head"
            );
        }
    }

    #[test]
    fn animation_state_is_kept_per_entity_and_bounded() {
        let jem = r#"{"models":[{"part":"head","translate":[0,6,0],
            "animations":[{"var.n":"var.n + 1"},{"this.ty":"6 + var.n"}],
            "boxes":[{"coordinates":[-4,-8,-4,8,8,8],"textureOffset":[0,0]}]}]}"#;
        let world = models(&[("cem/creeper.jem", jem.as_bytes().to_vec())]);
        let height = |layers: &[EntityLayer]| {
            layers
                .iter()
                .flat_map(|l| l.vertices.iter())
                .map(|v| v.position[1])
                .fold(f32::MIN, f32::max)
        };
        let first = height(&build_entity_layers(&creeper(), &world));
        let second = height(&build_entity_layers(&creeper(), &world));
        // `ty` grows by one pixel per frame: the counter persists.
        assert!(
            first > second,
            "y-up: a larger ty sits lower ({first} vs {second})"
        );
        let mut other = creeper();
        other.id = EntityId(99);
        let fresh = height(&build_entity_layers(&other, &world));
        assert!((fresh - first).abs() < 1e-5, "a new entity starts over");
        for id in 0..(MAX_STATES as i32 + 10) {
            other.id = EntityId(1000 + id);
            let _ = build_entity_layers(&other, &world);
        }
        assert!(world.0.states.borrow().len() <= MAX_STATES);
    }
}

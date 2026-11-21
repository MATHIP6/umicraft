// SPDX-License-Identifier: MIT OR Apache-2.0

//! Item icons for the GUI (P3): the baked item models, the icon keys and the
//! [`IconProvider`] the renderer asks when a slot shows an item it has not
//! drawn yet.
//!
//! An icon key packs everything that changes the picture: the item id, the
//! damage value (the metadata of non-damageable items; a damageable item
//! ignores it, see [`ItemModelSet::model`]) and the dye colour of leather
//! armour. The renderer treats the key as opaque.
//!
//! Gaps with the reference, tracked in `docs/parity.md`: potion liquid and
//! spawn-egg colours are untinted, and the sprites of the compass and the
//! clock are static.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use umicraft_assets::ClientResources;
use umicraft_game::inventory::tooltip::potion_has_effects;
use umicraft_game::item_nbt::ItemDisplay;

use crate::hud::dials::{is_dial, DialFrames};
use crate::resources::custom_items::CustomItems;
use crate::resources::item_tints::{item_grass_color, layer_tints, nbt_color};
use crate::scene::block_entity::is_entity_item;
use umicraft_game::items;
use umicraft_mesh::item::{ItemMesh, ItemStyle};
use umicraft_mesh::VertexLight;
use umicraft_model::item::pose::gui_pose;
use umicraft_model::item::{ItemModel, ItemModelSet, SpriteMask};
use umicraft_model::{AtlasLayout, ModelResult};
use umicraft_protocol::packets::play::ItemStack;
use umicraft_render::IconProvider;
use umicraft_ui::SlotItem;

/// Set in the colour field of a key when the item is dyed.
const COLOR_PRESENT: u64 = 1 << 24;
const ID_SHIFT: u32 = 0;
const DAMAGE_SHIFT: u32 = 16;
const COLOR_SHIFT: u32 = 32;

/// Skull and banner stacks remembered at most; the keys are built again on
/// the next frame that shows them.
const MAX_ENTITY_STACKS: usize = 1024;

/// The item id field of the icon key of a stack a Custom Items rule replaced;
/// the damage field then holds the rule index.
const CUSTOM_ID: u16 = u16::MAX;
/// Rule indices above this cannot be keyed.
const MAX_CUSTOM_RULES: usize = u16::MAX as usize;

const GOLDEN_APPLE: i16 = 322;
/// `Items.potionitem`.
const POTION: i16 = 373;
const ENCHANTED_BOOK: i16 = 403;
const EXPERIENCE_BOTTLE: i16 = 384;
const NETHER_STAR: i16 = 399;

/// The baked item models and the atlas layer layout they index.
#[derive(Debug)]
pub struct ItemResources {
    models: ItemModelSet,
    atlas: AtlasLayout,
    /// The grass colour of block items, `None` without the colormap.
    grass_color: Option<u32>,
    /// The NBT of the skull and banner stacks behind the icon keys handed
    /// out, which the key only identifies by a hash.
    entity_stacks: Mutex<HashMap<u64, Option<Vec<u8>>>>,
    /// The Custom Items rules of the packs, set once after loading.
    custom: OnceLock<Arc<CustomItems>>,
}

impl ItemResources {
    /// Bakes every item model of `resources`.
    ///
    /// The silhouettes of the `builtin/generated` sprites come from the
    /// block atlas pixels, which hold the item textures too.
    ///
    /// # Errors
    /// [`umicraft_model::ModelError`] for a missing or malformed item model.
    pub fn load(resources: &ClientResources) -> ModelResult<Self> {
        let atlas = resources.atlas();
        let masks = |key: &str| {
            let layer = atlas.layout().layer(key)? as usize;
            let (width, height) = (atlas.width(), atlas.height());
            let size = width as usize * height as usize * 4;
            let pixels = atlas.pixels().get(layer * size..(layer + 1) * size)?;
            SpriteMask::from_rgba(width, height, pixels)
        };
        let grass_color = resources.colormaps().map(item_grass_color);
        let mut models = ItemModelSet::build(resources, &masks)?;
        add_custom_models(&mut models, resources, &masks);
        Ok(Self {
            models,
            atlas: atlas.layout().clone(),
            grass_color,
            entity_stacks: Mutex::default(),
            custom: OnceLock::new(),
        })
    }

    /// Hands over the Custom Items rules whose models [`Self::load`] baked.
    /// Only the first call counts; the rules never change after loading.
    pub fn set_custom_items(&self, custom: Arc<CustomItems>) {
        if !custom.is_empty() {
            // A second call would carry the same rules; ignoring it is right.
            let _ = self.custom.set(custom);
        }
    }

    /// The model a Custom Items rule gives a stack, `None` for the usual one.
    /// The first thing it does is look for a rule of the item, so it costs
    /// one `Option` check without packs that bring rules.
    #[must_use]
    pub fn custom_model(
        &self,
        item_id: i16,
        damage: i16,
        count: u8,
        nbt: Option<&[u8]>,
    ) -> Option<&ItemModel> {
        let custom = self.custom.get()?;
        let index = custom.matching(item_id, damage, count, nbt)?;
        self.models.model_by_name(custom.model_name(index)?)
    }

    /// The rule index behind the icon key of a stack, when one replaces it.
    fn custom_rule(&self, stack: &ItemStack) -> Option<u16> {
        let custom = self.custom.get()?;
        let count = u8::try_from(stack.stack_size).unwrap_or(0);
        let index = custom.matching(stack.item_id, stack.damage, count, stack.nbt.as_deref())?;
        u16::try_from(index)
            .ok()
            .filter(|_| index < MAX_CUSTOM_RULES)
    }

    /// Resources without any model, for tests that bind a texture set.
    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self {
            models: ItemModelSet::default(),
            atlas: AtlasLayout::default(),
            grass_color: None,
            entity_stacks: Mutex::default(),
            custom: OnceLock::new(),
        }
    }

    /// Whether an item shows the enchantment glint; `nbt` is parsed only when
    /// present.
    #[must_use]
    pub fn glints(&self, item_id: i16, damage: i16, nbt: Option<&[u8]>) -> bool {
        let display = nbt.map(ItemDisplay::from_nbt).unwrap_or_default();
        has_glint(item_id, damage, nbt, &display)
    }

    /// The tint of each of the first two layers of an item's model (dyed
    /// leather, potions, spawn eggs, firework charges, grass and foliage
    /// block items); white for everything else.
    #[must_use]
    pub fn tints(&self, item_id: i16, damage: i16, nbt: Option<&[u8]>) -> [u32; 2] {
        layer_tints(item_id, damage, nbt_color(item_id, nbt), self.grass_color)
    }

    /// For a skull or banner, the hash that tells its icons apart (it takes
    /// the place of the dye colour in the icon key); the stack is remembered
    /// for [`Self::entity_stack`].
    fn entity_stack_hash(&self, stack: &ItemStack) -> Option<u32> {
        if !is_entity_item(stack.item_id) {
            return None;
        }
        let hash = fnv_24(stack.nbt.as_deref().unwrap_or_default());
        let key = icon_key(stack, None, Some(hash), DialFrames::default());
        let Ok(mut stacks) = self.entity_stacks.lock() else {
            return Some(hash);
        };
        if stacks.len() >= MAX_ENTITY_STACKS {
            stacks.clear();
        }
        stacks.entry(key).or_insert_with(|| stack.nbt.clone());
        Some(hash)
    }

    /// The item id, damage and NBT of the skull or banner behind an icon
    /// key, as [`Self::slot_item`] saw it.
    #[must_use]
    pub(crate) fn entity_stack(&self, key: u64) -> Option<(i16, i16, Option<Vec<u8>>)> {
        let (item_id, damage, _) = unpack_key(key);
        let nbt = self.entity_stacks.lock().ok()?.get(&key)?.clone();
        Some((item_id, damage, nbt))
    }

    /// The atlas layer of the picture a particle of an item shows (MCP
    /// `ItemModelMesher.getParticleIcon`), for the pieces of a cracked item;
    /// `None` for an item without a sprite model or a picture.
    #[must_use]
    pub fn particle_layer(&self, item_id: u16, damage: u16) -> Option<u32> {
        let damageable =
            items::lookup(i32::from(item_id)).is_some_and(items::ItemDef::is_damageable);
        let model = self.models.model(item_id, damage, damageable)?;
        self.atlas.layer(model.particle.as_deref()?)
    }

    /// The texture layers the item models index.
    #[must_use]
    pub fn atlas(&self) -> &AtlasLayout {
        &self.atlas
    }

    /// The model set.
    #[must_use]
    pub fn models(&self) -> &ItemModelSet {
        &self.models
    }

    /// The widget data for a stack: its icon key, count, durability and
    /// glint.
    #[must_use]
    pub fn slot_item(&self, stack: &ItemStack, dials: DialFrames) -> SlotItem {
        let definition = items::lookup(i32::from(stack.item_id));
        let display = stack
            .nbt
            .as_deref()
            .map(ItemDisplay::from_nbt)
            .unwrap_or_default();
        let color = nbt_color(stack.item_id, stack.nbt.as_deref())
            .or_else(|| self.entity_stack_hash(stack));
        let key = self
            .custom_rule(stack)
            .map_or_else(|| icon_key(stack, definition, color, dials), custom_key);
        SlotItem {
            key,
            count: u8::try_from(stack.stack_size).unwrap_or(0),
            damage: stack.damage,
            max_durability: definition
                .map_or(0, |def| i16::try_from(def.max_damage).unwrap_or(i16::MAX)),
            glint: has_glint(stack.item_id, stack.damage, stack.nbt.as_deref(), &display),
        }
    }
}

/// What the UI needs to turn a stack into a slot: the item resources, when
/// loaded, and the frames the dials show this frame.
#[derive(Debug, Clone, Copy)]
pub struct ItemView<'a> {
    resources: Option<&'a ItemResources>,
    dials: DialFrames,
}

impl<'a> ItemView<'a> {
    /// A view over `resources` (`None` for the procedural fallback, which
    /// has no item sprites).
    #[must_use]
    pub fn new(resources: Option<&'a ItemResources>, dials: DialFrames) -> Self {
        Self { resources, dials }
    }
}

/// The widget data for a stack, or an icon-less slot while no item resources
/// are loaded (the procedural fallback has no item sprites).
#[must_use]
pub fn slot_item(view: ItemView<'_>, stack: &ItemStack) -> SlotItem {
    match view.resources {
        Some(items) => items.slot_item(stack, view.dials),
        None => SlotItem {
            key: u64::MAX,
            count: u8::try_from(stack.stack_size).unwrap_or(0),
            damage: stack.damage,
            max_durability: 0,
            glint: false,
        },
    }
}

/// Lends the item resources to the renderer as an [`IconProvider`].
pub struct IconSource<'a>(pub &'a ItemResources);

impl IconProvider for IconSource<'_> {
    fn build_icon(&mut self, key: u64, mesh: &mut ItemMesh) -> bool {
        self.0.build_icon(key, mesh)
    }
}

impl ItemResources {
    /// Fills `mesh` with the GUI geometry of the icon `key`, `false` when the
    /// item has no sprite model.
    fn build_icon(&self, key: u64, mesh: &mut ItemMesh) -> bool {
        let (item_id, key_damage, color) = unpack_key(key);
        if item_id as u16 == CUSTOM_ID {
            return self.build_custom_icon(key_damage as u16, mesh);
        }
        let damageable =
            items::lookup(i32::from(item_id)).is_some_and(items::ItemDef::is_damageable);
        // A dial's key holds its frame in the damage field; its model has none.
        let damage = if is_dial(item_id) { 0 } else { key_damage };
        let Some(model) = self.models.model(item_id as u16, damage as u16, damageable) else {
            return false;
        };
        let umicraft_model::item::ItemGeometry::Quads(quads) = &model.geometry else {
            // Entity-rendered items (chest, banner, skull) have no sprite yet.
            return false;
        };
        let tints = layer_tints(item_id, damage, color, self.grass_color);
        let style = ItemStyle {
            atlas: &self.atlas,
            // Full light: lit blocks get the GUI lights baked into the tint
            // (`push_quads_gui`), flat sprites stay unlit.
            light: VertexLight::new(0, 15, 15, false),
            tints: &tints,
        };
        let pose = gui_pose(model);
        if model.gui_3d {
            mesh.push_quads_gui(quads, &pose, &style);
        } else {
            mesh.push_quads(quads, &pose, &style);
        }
        !mesh.is_empty()
    }
}

impl ItemResources {
    /// Fills `mesh` with the icon of the model rule `index` of the Custom
    /// Items rules generated, untinted.
    fn build_custom_icon(&self, index: u16, mesh: &mut ItemMesh) -> bool {
        let Some(custom) = self.custom.get() else {
            return false;
        };
        let Some(model) = custom
            .model_name(usize::from(index))
            .and_then(|name| self.models.model_by_name(name))
        else {
            return false;
        };
        let umicraft_model::item::ItemGeometry::Quads(quads) = &model.geometry else {
            return false;
        };
        let tints = layer_tints(0, 0, None, None);
        let style = ItemStyle {
            atlas: &self.atlas,
            light: VertexLight::new(0, 15, 15, false),
            tints: &tints,
        };
        mesh.push_quads(quads, &gui_pose(model), &style);
        !mesh.is_empty()
    }
}

/// The icon key of the stack that Custom Items rule `index` replaced.
fn custom_key(index: u16) -> u64 {
    (u64::from(CUSTOM_ID) << ID_SHIFT) | (u64::from(index) << DAMAGE_SHIFT)
}

/// Bakes the generated models of the Custom Items rules, which the packs
/// carry as `models/item/optifine_cit_<n>.json`. One that does not bake is
/// skipped with a warning: its rule then never matches.
fn add_custom_models(
    models: &mut ItemModelSet,
    resources: &ClientResources,
    masks: &dyn Fn(&str) -> Option<SpriteMask>,
) {
    use umicraft_model::ResourceSource;
    const PREFIX: &str = "assets/minecraft/models/item/optifine_cit_";
    for path in resources.list(PREFIX) {
        let Some(name) = path
            .strip_prefix("assets/minecraft/models/item/")
            .and_then(|rest| rest.strip_suffix(".json"))
        else {
            continue;
        };
        if let Err(error) = models.add_named(resources, masks, name) {
            tracing::warn!(%name, %error, "custom item model ignored");
        }
    }
}

/// A 24-bit FNV-1a hash, for the part of an icon key that identifies NBT.
fn fnv_24(bytes: &[u8]) -> u32 {
    let hash = bytes.iter().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(*byte)).wrapping_mul(0x0100_0193)
    });
    (hash ^ (hash >> 24)) & 0x00FF_FFFF
}

/// Packs what decides the picture of a stack into an icon key.
///
/// The damage field holds the metadata of items that look different with
/// it, nothing for a damageable item (it looks the same at any damage) and
/// the shown frame for the compass and the clock.
fn icon_key(
    stack: &ItemStack,
    definition: Option<&items::ItemDef>,
    color: Option<u32>,
    dials: DialFrames,
) -> u64 {
    let color = color.map_or(0, |rgb| COLOR_PRESENT | u64::from(rgb & 0x00FF_FFFF));
    let damage = if let Some(frame) = dials.frame_of(stack.item_id) {
        frame
    } else if definition.is_some_and(items::ItemDef::is_damageable) {
        0
    } else {
        stack.damage as u16
    };
    (u64::from(stack.item_id as u16) << ID_SHIFT)
        | (u64::from(damage) << DAMAGE_SHIFT)
        | (color << COLOR_SHIFT)
}

/// The inverse of [`icon_key`]: item id, damage and the dye colour.
fn unpack_key(key: u64) -> (i16, i16, Option<u32>) {
    let item_id = (key >> ID_SHIFT) as u16 as i16;
    let damage = (key >> DAMAGE_SHIFT) as u16 as i16;
    let color_field = key >> COLOR_SHIFT;
    let color = (color_field & COLOR_PRESENT != 0).then_some((color_field & 0x00FF_FFFF) as u32);
    (item_id, damage, color)
}

/// Whether the stack shows the enchantment glint (MCP `Item.hasEffect` and
/// its overrides for the golden apple, enchanted book, experience bottle and
/// nether star, and potions with an effect).
fn has_glint(item_id: i16, damage: i16, nbt: Option<&[u8]>, display: &ItemDisplay) -> bool {
    display.has_glint()
        || match item_id {
            POTION => potion_has_effects(damage, nbt),
            GOLDEN_APPLE => damage > 0,
            ENCHANTED_BOOK | EXPERIENCE_BOTTLE | NETHER_STAR => true,
            _ => false,
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(item_id: i16, damage: i16) -> ItemStack {
        ItemStack {
            item_id,
            stack_size: 1,
            damage,
            nbt: None,
        }
    }

    #[test]
    fn a_key_round_trips_id_damage_and_tint() {
        let key = icon_key(
            &stack(298, 12),
            items::lookup(298),
            Some(0x12_3456),
            DialFrames::default(),
        );
        assert_eq!(unpack_key(key), (298, 0, Some(0x12_3456)));
        let key = icon_key(&stack(1, 3), items::lookup(1), None, DialFrames::default());
        assert_eq!(unpack_key(key), (1, 3, None));
    }

    #[test]
    fn damage_only_changes_the_key_of_items_without_durability() {
        let sword = |damage| {
            icon_key(
                &stack(276, damage),
                items::lookup(276),
                None,
                DialFrames::default(),
            )
        };
        assert_eq!(sword(0), sword(900));
        let wool = |damage| {
            icon_key(
                &stack(35, damage),
                items::lookup(35),
                None,
                DialFrames::default(),
            )
        };
        assert_ne!(wool(0), wool(14));
    }

    #[test]
    fn intrinsic_glints_follow_the_reference() {
        let none = ItemDisplay::default();
        assert!(!has_glint(322, 0, None, &none));
        assert!(has_glint(322, 1, None, &none));
        assert!(has_glint(399, 0, None, &none));
        assert!(!has_glint(1, 0, None, &none));
        // Potions glint when they carry an effect: swiftness, splash healing,
        // but not the water bottle or an unbrewed one.
        assert!(has_glint(373, 8194, None, &none));
        assert!(has_glint(373, 16421, None, &none));
        assert!(!has_glint(373, 0, None, &none));
        assert!(!has_glint(373, 16, None, &none));
    }
}

#[cfg(test)]
mod render_tests {
    use super::*;
    use crate::resources::models::ModelResources;
    use umicraft_render::{Camera, Overlay, Renderer};
    use umicraft_ui::{DrawList, Rect};

    fn stack(item_id: i16, damage: i16) -> ItemStack {
        ItemStack {
            item_id,
            stack_size: 1,
            damage,
            nbt: None,
        }
    }

    /// Renders a row of real icons offscreen; set `UMICRAFT_ICON_DUMP` to a
    /// path to keep the PNG for a visual check.
    #[test]
    #[ignore = "requires the official 1.8.8 client jar and a GPU adapter"]
    fn real_icons_render_into_the_atlas() {
        let resources = ModelResources::load().expect("official resources");
        let items = resources.items();
        let atlas = resources.atlas();
        let mut renderer = Renderer::new_offscreen(320, 96).expect("renderer");
        renderer
            .set_block_atlas(
                atlas.width(),
                atlas.height(),
                atlas.layers(),
                atlas.pixels(),
            )
            .expect("atlas");
        let stacks = [
            stack(373, 8197),
            stack(373, 8194),
            stack(373, 16453),
            stack(383, 50),
            stack(383, 91),
            stack(298, 0),
            stack(345, 0),
            stack(347, 0),
            stack(402, 0),
            stack(261, 0),
        ];
        let mut list = DrawList::new();
        for (index, item) in stacks.iter().enumerate() {
            let x = 8.0 + (index % 5) as f32 * 60.0;
            let y = 8.0 + (index / 5) as f32 * 44.0;
            list.item(
                items
                    .slot_item(
                        item,
                        DialFrames {
                            compass: 8,
                            clock: 20,
                        },
                    )
                    .key,
                Rect::new(x, y, 48.0, 48.0),
                false,
            );
        }
        renderer
            .render_with_ui(
                &Camera::default(),
                &Overlay::new(320, 96),
                &list,
                &mut IconSource(&items),
            )
            .expect("render");
        let image = renderer.read_pixels().expect("readback");
        let first = &image.rgba[..4];
        assert!(image.rgba.chunks_exact(4).filter(|p| *p != first).count() > 2000);
        if let Ok(path) = std::env::var("UMICRAFT_ICON_DUMP") {
            let file = std::fs::File::create(path).expect("dump file");
            let mut encoder = png::Encoder::new(file, image.width, image.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .and_then(|mut writer| writer.write_image_data(&image.rgba))
                .expect("png");
        }
    }

    #[test]
    fn a_custom_items_rule_gives_the_stack_its_own_icon_key() {
        use crate::resources::game_files::test_support::png;
        use crate::resources::game_files::PackFeatures;
        let files = [
            (
                "optifine/cit/blade.properties",
                b"items=276\ntexture=pic\n".to_vec(),
            ),
            ("optifine/cit/pic.png", png(16, 16)),
        ]
        .into_iter()
        .map(|(path, bytes)| (format!("assets/minecraft/{path}"), bytes))
        .collect();
        let pack = umicraft_assets::ResourcePack::from_parts("cit", None, None, files);
        let packs = umicraft_assets::PackFiles::from_packs([pack]);
        let (custom, _) = CustomItems::scan(&packs, PackFeatures::ALL);

        let resources = ItemResources::empty();
        let plain = resources.slot_item(&stack(276, 0), DialFrames::default());
        resources.set_custom_items(Arc::new(custom));
        let replaced = resources.slot_item(&stack(276, 0), DialFrames::default());
        let other = resources.slot_item(&stack(280, 0), DialFrames::default());

        assert_ne!(plain.key, replaced.key);
        assert_eq!(unpack_key(replaced.key).0 as u16, CUSTOM_ID);
        assert_eq!(unpack_key(replaced.key).1, 0, "rule index 0");
        assert_eq!(
            other.key,
            resources
                .slot_item(&stack(280, 0), DialFrames::default())
                .key
        );
        assert_eq!(
            replaced.damage, 0,
            "the durability bar still reads the damage"
        );
    }
}

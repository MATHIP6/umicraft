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

use umicraft_assets::ClientResources;
use umicraft_game::item_nbt::ItemDisplay;
use umicraft_game::items::{self, ArmorMaterial, ItemKind};
use umicraft_mesh::item::{ItemMesh, ItemStyle};
use umicraft_mesh::tint::WHITE;
use umicraft_mesh::VertexLight;
use umicraft_model::item::pose::gui_pose;
use umicraft_model::item::{ItemModelSet, SpriteMask};
use umicraft_model::{AtlasLayout, ModelResult};
use umicraft_protocol::packets::play::ItemStack;
use umicraft_render::IconProvider;
use umicraft_ui::SlotItem;

/// The colour of undyed leather armour (MCP `ItemArmor.getColor`).
const DEFAULT_LEATHER_COLOR: u32 = 0x00A0_6540;
/// Foliage colours of the leaf, vine and lily pad items (MCP
/// `ColorizerFoliage.getFoliageColorBasic` / `Pine` / `Birch`,
/// `BlockLilyPad.getRenderColor`).
const FOLIAGE_BASIC: u32 = 4_764_952;
const FOLIAGE_PINE: u32 = 6_396_257;
const FOLIAGE_BIRCH: u32 = 8_431_445;
const LILY_PAD_COLOR: u32 = 7_455_580;
/// Climate sample of the grass colour of an item (MCP
/// `ColorizerGrass.getGrassColor(0.5, 1.0)`).
const GRASS_TEMPERATURE: f32 = 0.5;
const GRASS_RAINFALL: f32 = 1.0;
const GRASS: i16 = 2;
const LEAVES: i16 = 18;
const TALL_GRASS: i16 = 31;
const VINE: i16 = 106;
const LILY_PAD: i16 = 111;
const LEAVES_2: i16 = 161;
const DOUBLE_PLANT: i16 = 175;
/// Set in the colour field of a key when the item is dyed.
const COLOR_PRESENT: u64 = 1 << 24;
const ID_SHIFT: u32 = 0;
const DAMAGE_SHIFT: u32 = 16;
const COLOR_SHIFT: u32 = 32;

const GOLDEN_APPLE: i16 = 322;
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
        let grass_color = resources.colormaps().map(|maps| {
            let [r, g, b] = maps.grass.sample(GRASS_TEMPERATURE, GRASS_RAINFALL);
            u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
        });
        Ok(Self {
            models: ItemModelSet::build(resources, &masks)?,
            atlas: atlas.layout().clone(),
            grass_color,
        })
    }

    /// Resources without any model, for tests that bind a texture set.
    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self {
            models: ItemModelSet::default(),
            atlas: AtlasLayout::default(),
            grass_color: None,
        }
    }

    /// The colour that multiplies the first layer of the stack's icon: dyed
    /// or default leather, and the foliage and grass block items.
    fn tint(
        &self,
        stack: &ItemStack,
        definition: Option<&items::ItemDef>,
        display: &ItemDisplay,
    ) -> Option<u32> {
        let leather = matches!(
            definition.map(|def| def.kind),
            Some(ItemKind::Armor(ArmorMaterial::Leather, _))
        );
        if leather {
            return Some(display.leather_color.unwrap_or(DEFAULT_LEATHER_COLOR));
        }
        block_item_tint(stack.item_id, stack.damage, self.grass_color)
    }

    /// The model set.
    #[must_use]
    pub fn models(&self) -> &ItemModelSet {
        &self.models
    }

    /// The widget data for a stack: its icon key, count, durability and
    /// glint.
    #[must_use]
    pub fn slot_item(&self, stack: &ItemStack) -> SlotItem {
        let definition = items::lookup(i32::from(stack.item_id));
        let display = stack
            .nbt
            .as_deref()
            .map(ItemDisplay::from_nbt)
            .unwrap_or_default();
        SlotItem {
            key: icon_key(stack, definition, self.tint(stack, definition, &display)),
            count: u8::try_from(stack.stack_size).unwrap_or(0),
            damage: stack.damage,
            max_durability: definition
                .map_or(0, |def| i16::try_from(def.max_damage).unwrap_or(i16::MAX)),
            glint: has_glint(stack, &display),
        }
    }
}

/// The widget data for a stack, or an icon-less slot while no item resources
/// are loaded (the procedural fallback has no item sprites).
#[must_use]
pub fn slot_item(items: Option<&ItemResources>, stack: &ItemStack) -> SlotItem {
    match items {
        Some(items) => items.slot_item(stack),
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
        let (item_id, damage, color) = unpack_key(key);
        let damageable =
            items::lookup(i32::from(item_id)).is_some_and(items::ItemDef::is_damageable);
        let Some(model) = self.models.model(item_id as u16, damage as u16, damageable) else {
            return false;
        };
        let umicraft_model::item::ItemGeometry::Quads(quads) = &model.geometry else {
            // Entity-rendered items (chest, banner, skull) have no sprite yet.
            return false;
        };
        let tints = [color.unwrap_or(WHITE)];
        let style = ItemStyle {
            atlas: &self.atlas,
            // Flat sprites are drawn unlit; lit blocks keep the face shade.
            light: VertexLight::new(0, 15, 15, model.gui_3d),
            tints: &tints,
        };
        mesh.push_quads(quads, &gui_pose(model), &style);
        !mesh.is_empty()
    }
}

/// Packs what decides the picture of a stack into an icon key.
fn icon_key(stack: &ItemStack, definition: Option<&items::ItemDef>, tint: Option<u32>) -> u64 {
    let color = tint.map_or(0, |rgb| COLOR_PRESENT | u64::from(rgb & 0x00FF_FFFF));
    // A damageable item looks the same at any damage: keep its key stable.
    let damage = if definition.is_some_and(items::ItemDef::is_damageable) {
        0
    } else {
        stack.damage as u16
    };
    (u64::from(stack.item_id as u16) << ID_SHIFT)
        | (u64::from(damage) << DAMAGE_SHIFT)
        | (color << COLOR_SHIFT)
}

/// The tint of the block items that the reference colours
/// (`Block.getRenderColor` through `ItemColored` and `ItemLeaves`).
fn block_item_tint(item_id: i16, damage: i16, grass: Option<u32>) -> Option<u32> {
    match item_id {
        GRASS => grass,
        TALL_GRASS if damage != 0 => grass,
        DOUBLE_PLANT if matches!(damage & 7, 2 | 3) => grass,
        LEAVES => Some(match damage & 3 {
            1 => FOLIAGE_PINE,
            2 => FOLIAGE_BIRCH,
            _ => FOLIAGE_BASIC,
        }),
        LEAVES_2 | VINE => Some(FOLIAGE_BASIC),
        LILY_PAD => Some(LILY_PAD_COLOR),
        _ => None,
    }
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
/// nether star).
fn has_glint(stack: &ItemStack, display: &ItemDisplay) -> bool {
    display.has_glint()
        || match stack.item_id {
            GOLDEN_APPLE => stack.damage > 0,
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
        let key = icon_key(&stack(298, 12), items::lookup(298), Some(0x12_3456));
        assert_eq!(unpack_key(key), (298, 0, Some(0x12_3456)));
        let key = icon_key(&stack(1, 3), items::lookup(1), None);
        assert_eq!(unpack_key(key), (1, 3, None));
    }

    #[test]
    fn damage_only_changes_the_key_of_items_without_durability() {
        let sword = |damage| icon_key(&stack(276, damage), items::lookup(276), None);
        assert_eq!(sword(0), sword(900));
        let wool = |damage| icon_key(&stack(35, damage), items::lookup(35), None);
        assert_ne!(wool(0), wool(14));
    }

    #[test]
    fn foliage_and_grass_items_are_tinted_like_the_reference() {
        let grass = Some(0x0091_BD59);
        assert_eq!(block_item_tint(2, 0, grass), grass);
        assert_eq!(block_item_tint(31, 0, grass), None, "dead bush stays white");
        assert_eq!(block_item_tint(31, 1, grass), grass);
        assert_eq!(block_item_tint(175, 2, grass), grass);
        assert_eq!(
            block_item_tint(175, 0, grass),
            None,
            "sunflower is untinted"
        );
        assert_eq!(block_item_tint(18, 1, None), Some(FOLIAGE_PINE));
        assert_eq!(block_item_tint(18, 2, None), Some(FOLIAGE_BIRCH));
        assert_eq!(block_item_tint(18, 0, None), Some(FOLIAGE_BASIC));
        assert_eq!(block_item_tint(111, 0, None), Some(LILY_PAD_COLOR));
        assert_eq!(block_item_tint(1, 0, grass), None);
    }

    #[test]
    fn intrinsic_glints_follow_the_reference() {
        let none = ItemDisplay::default();
        assert!(!has_glint(&stack(322, 0), &none));
        assert!(has_glint(&stack(322, 1), &none));
        assert!(has_glint(&stack(399, 0), &none));
        assert!(!has_glint(&stack(1, 0), &none));
    }
}

#[cfg(test)]
mod render_tests {
    use super::*;
    use crate::model_resources::ModelResources;
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
            stack(276, 0),
            stack(1, 0),
            stack(2, 0),
            stack(260, 0),
            stack(298, 0),
            stack(35, 14),
            stack(18, 0),
            stack(18, 1),
            stack(322, 0),
            stack(31, 1),
        ];
        let mut list = DrawList::new();
        for (index, item) in stacks.iter().enumerate() {
            let x = 8.0 + (index % 5) as f32 * 60.0;
            let y = 8.0 + (index / 5) as f32 * 44.0;
            list.item(
                items.slot_item(item).key,
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
}

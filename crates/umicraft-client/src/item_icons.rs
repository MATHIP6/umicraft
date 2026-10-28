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

use crate::dials::{is_dial, DialFrames};
use crate::item_tints::{item_grass_color, layer_tints, nbt_color};
use umicraft_game::items;
use umicraft_mesh::item::{ItemMesh, ItemStyle};
use umicraft_mesh::VertexLight;
use umicraft_model::item::pose::gui_pose;
use umicraft_model::item::{ItemModelSet, SpriteMask};
use umicraft_model::{AtlasLayout, ModelResult};
use umicraft_protocol::packets::play::ItemStack;
use umicraft_render::IconProvider;
use umicraft_ui::SlotItem;

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
        let grass_color = resources.colormaps().map(item_grass_color);
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

    /// Whether an item shows the enchantment glint; `nbt` is parsed only when
    /// present.
    #[must_use]
    pub fn glints(&self, item_id: i16, damage: i16, nbt: Option<&[u8]>) -> bool {
        let display = nbt.map(ItemDisplay::from_nbt).unwrap_or_default();
        has_glint(item_id, damage, &display)
    }

    /// The tint of each of the first two layers of an item's model (dyed
    /// leather, potions, spawn eggs, firework charges, grass and foliage
    /// block items); white for everything else.
    #[must_use]
    pub fn tints(&self, item_id: i16, damage: i16, nbt: Option<&[u8]>) -> [u32; 2] {
        layer_tints(item_id, damage, nbt_color(item_id, nbt), self.grass_color)
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
        SlotItem {
            key: icon_key(
                stack,
                definition,
                nbt_color(stack.item_id, stack.nbt.as_deref()),
                dials,
            ),
            count: u8::try_from(stack.stack_size).unwrap_or(0),
            damage: stack.damage,
            max_durability: definition
                .map_or(0, |def| i16::try_from(def.max_damage).unwrap_or(i16::MAX)),
            glint: has_glint(stack.item_id, stack.damage, &display),
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
            // Flat sprites are drawn unlit; lit blocks keep the face shade.
            light: VertexLight::new(0, 15, 15, model.gui_3d),
            tints: &tints,
        };
        mesh.push_quads(quads, &gui_pose(model), &style);
        !mesh.is_empty()
    }
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
/// nether star).
fn has_glint(item_id: i16, damage: i16, display: &ItemDisplay) -> bool {
    display.has_glint()
        || match item_id {
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
        assert!(!has_glint(322, 0, &none));
        assert!(has_glint(322, 1, &none));
        assert!(has_glint(399, 0, &none));
        assert!(!has_glint(1, 0, &none));
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
}

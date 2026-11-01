// SPDX-License-Identifier: MIT OR Apache-2.0

//! GUI icons of the items a tile-entity model draws: skulls and banners
//! (MCP `RenderItem.renderItemIntoGUI` for a `builtin/entity` model).
//!
//! The icon atlas renders sprite items from the block atlas, but these items
//! wear entity textures (a skin, a composed banner). Before the UI is drawn,
//! this pass finds the entity items of the draw list that the atlas lacks,
//! makes sure their textures exist and has the renderer draw them into the
//! atlas with the entity pipeline.
//!
//! A player skull whose owner's skin is still downloading is drawn with the
//! default skin and drawn again once the skin is ready.

use std::collections::HashSet;

use umicraft_model::item::pose::gui_pose;
use umicraft_render::Renderer;
use umicraft_ui::{DrawCommand, DrawList};
use umicraft_world::LightLevel;

use crate::block_entity_render::entity_item_mesh;
use crate::entity_render::EntityTextures;
use crate::item_icons::ItemResources;

/// The entity icons waiting for a texture that is not final yet.
#[derive(Debug, Default)]
pub(crate) struct EntityIcons {
    /// Keys drawn with the default skin while the owner's skin downloads.
    provisional: HashSet<u64>,
}

impl EntityIcons {
    /// Renders the entity icons of `draw_list` that the atlas does not hold.
    pub(crate) fn prepare(
        &mut self,
        draw_list: &DrawList,
        items: &ItemResources,
        textures: &EntityTextures,
        renderer: &mut Renderer,
    ) {
        for command in draw_list.commands() {
            let DrawCommand::Item { key, .. } = command else {
                continue;
            };
            self.prepare_icon(*key, items, textures, renderer);
        }
    }

    fn prepare_icon(
        &mut self,
        key: u64,
        items: &ItemResources,
        textures: &EntityTextures,
        renderer: &mut Renderer,
    ) {
        let waiting = self.provisional.contains(&key);
        if renderer.has_icon(key) && !waiting {
            return;
        }
        let Some((item_id, damage, nbt)) = items.entity_stack(key) else {
            return;
        };
        let Some(mesh) = entity_item_mesh(item_id, damage, nbt.as_deref(), textures) else {
            return;
        };
        if waiting && mesh.pending_skin {
            return;
        }
        let Some(model) = items.models().model(item_id as u16, damage as u16, false) else {
            return;
        };
        if let Some(design) = &mesh.banner {
            textures.ensure_banner(design, renderer);
        }
        let Some(texture) = textures.bind_group_of(mesh.slot) else {
            return;
        };
        renderer.forget_icon(key);
        // The GUI draws items at full light.
        let vertices = mesh.placed(&gui_pose(model), LightLevel::FULL_SKY);
        renderer.render_entity_icon(key, &vertices, &texture);
        if mesh.pending_skin {
            self.provisional.insert(key);
        } else {
            self.provisional.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dials::DialFrames;
    use crate::model_resources::ModelResources;
    use umicraft_protocol::packets::play::ItemStack;
    use umicraft_render::{Camera, Overlay};
    use umicraft_ui::Rect;

    fn stack(item_id: i16, damage: i16, nbt: Option<Vec<u8>>) -> ItemStack {
        ItemStack {
            item_id,
            stack_size: 1,
            damage,
            nbt,
        }
    }

    /// `{BlockEntityTag: {Base: 1, Patterns: [{Pattern: "ts", Color: 11}]}}`.
    fn striped_banner_nbt() -> Vec<u8> {
        let name = |out: &mut Vec<u8>, tag: u8, name: &str| {
            out.push(tag);
            out.extend((name.len() as u16).to_be_bytes());
            out.extend(name.as_bytes());
        };
        let mut nbt = Vec::new();
        name(&mut nbt, 10, "");
        name(&mut nbt, 10, "BlockEntityTag");
        name(&mut nbt, 3, "Base");
        nbt.extend(1_i32.to_be_bytes());
        name(&mut nbt, 9, "Patterns");
        nbt.extend([10]);
        nbt.extend(1_i32.to_be_bytes());
        name(&mut nbt, 8, "Pattern");
        nbt.extend(2_u16.to_be_bytes());
        nbt.extend(b"ts");
        name(&mut nbt, 3, "Color");
        nbt.extend(11_i32.to_be_bytes());
        nbt.extend([0, 0, 0]);
        nbt
    }

    /// Renders every skull type and a few banners as GUI icons; set
    /// `UMICRAFT_ICON_DUMP` to a path to keep the PNG for a visual check.
    #[test]
    #[ignore = "requires the official 1.8.8 client jar and a GPU adapter"]
    fn real_skull_and_banner_icons_render() {
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
        let textures = EntityTextures::load(&renderer);
        let stacks = [
            stack(397, 0, None),
            stack(397, 1, None),
            stack(397, 2, None),
            stack(397, 3, None),
            stack(397, 4, None),
            stack(425, 15, None),
            stack(425, 1, None),
            stack(425, 4, Some(striped_banner_nbt())),
        ];
        let mut list = DrawList::new();
        for (index, item) in stacks.iter().enumerate() {
            let x = 8.0 + (index % 5) as f32 * 60.0;
            let y = 8.0 + (index / 5) as f32 * 44.0;
            let key = items.slot_item(item, DialFrames::default()).key;
            list.item(key, Rect::new(x, y, 48.0, 48.0), false);
        }
        EntityIcons::default().prepare(&list, &items, &textures, &mut renderer);
        renderer
            .render_with_ui(
                &Camera::default(),
                &Overlay::new(320, 96),
                &list,
                &mut crate::item_icons::IconSource(&items),
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

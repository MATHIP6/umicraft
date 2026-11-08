// SPDX-License-Identifier: MIT OR Apache-2.0

//! Standing and wall banners (MCP `TileEntityBannerRenderer`).
//!
//! The model is `BannerModel`; its texture is composed from the base dye and
//! patterns of the last `Update Block Entity` of type 6 (`banner_texture`).
//! The mesher only names the texture by [`BannerDesign::slot`] and records the
//! design; the texture cache creates the missing ones before the upload.
//!
//! Deviation: the sway of the cloth follows the world tick count in the
//! reference. The scene has none, so the client clock times 20 stands in; it
//! only phases the swing of neighbouring banners.

use umicraft_game::server::block_entities::Pos;
use umicraft_mesh::{BlockEntityKind, BlockEntityRef};
use umicraft_model::entity::{banner_sway, standing_sign_degrees, wall_sign_degrees, BannerModel};

use super::banner_texture::BannerDesign;
use super::{push_at_block, BannerDesigns, Batches, BlockEntityScene};

/// `Update Block Entity` type of a banner (`TileEntityBanner.getDescriptionPacket`).
const BANNER_ACTION: u8 = 6;
/// Ticks per second, to turn the client clock into the sway's world time.
const TICKS_PER_SECOND: f32 = 20.0;

/// Draws the banner at `entry` and records its design in `designs`.
pub(super) fn add_banner(
    batches: &mut Batches,
    designs: &mut BannerDesigns,
    entry: &BlockEntityRef,
    scene: &BlockEntityScene<'_>,
) {
    let standing = BlockEntityKind::of(entry.state) == Some(BlockEntityKind::StandingBanner);
    let metadata = entry.state.metadata();
    // The renderer turns banners with the same formulas as signs.
    let degrees = if standing {
        standing_sign_degrees(metadata)
    } else {
        wall_sign_degrees(metadata)
    };
    let [x, y, z] = scene.world_position(entry);
    let design = design_at(scene, (x, y, z));
    let slot = design.slot();
    designs.entry(slot).or_insert(design);
    let sway = banner_sway(sway_phase(scene.world_position(entry), scene.seconds));
    push_at_block(
        batches.entry(slot).or_default(),
        BannerModel::emit(standing, degrees, sway),
        entry,
    );
}

/// The design the server last sent for the banner at `pos`.
fn design_at(scene: &BlockEntityScene<'_>, pos: Pos) -> BannerDesign {
    let nbt = scene
        .data
        .get(&pos)
        .filter(|data| data.action == BANNER_ACTION)
        .and_then(|data| data.nbt.as_deref());
    BannerDesign::from_nbt(nbt)
}

/// `x * 7 + y * 9 + z * 13 + time`, the argument of the sway.
fn sway_phase([x, y, z]: [i32; 3], seconds: f32) -> f32 {
    (x.wrapping_mul(7) + y.wrapping_mul(9) + z.wrapping_mul(13)) as f32 + seconds * TICKS_PER_SECOND
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use glam::Vec3;
    use umicraft_game::server::block_entities::{BlockEntityData, EntityTable, SignTable};
    use umicraft_game::server::chest_lids::ChestLids;
    use umicraft_world::{BlockState, LightLevel};

    use super::*;
    use crate::entity_render::NoSkins;

    const STANDING_BANNER: u16 = 176;
    const WALL_BANNER: u16 = 177;

    fn entry(id: u16, metadata: u8) -> BlockEntityRef {
        BlockEntityRef {
            position: [4, 70, 9],
            state: BlockState::new(id, metadata),
            same_neighbour: umicraft_mesh::SameNeighbours::default(),
            light: LightLevel { block: 0, sky: 15 },
        }
    }

    fn draw(entry: &BlockEntityRef, data: &EntityTable) -> (Batches, BannerDesigns) {
        let lids = ChestLids::default();
        let signs = SignTable::new();
        let scene = BlockEntityScene {
            data,
            skins: &NoSkins,
            seconds: 1.0,
            lids: &lids,
            signs: &signs,
            font: None,
            partial_tick: 0.0,
            camera: Vec3::ZERO,
            origin: [0; 3],
        };
        let (mut batches, mut designs) = (HashMap::new(), HashMap::new());
        add_banner(&mut batches, &mut designs, entry, &scene);
        (batches, designs)
    }

    #[test]
    fn a_banner_with_no_update_is_white_and_drawn_under_its_own_slot() {
        let (batches, designs) = draw(&entry(STANDING_BANNER, 0), &EntityTable::new());
        assert_eq!(designs.len(), 1);
        let (slot, design) = designs.iter().next().expect("one design");
        assert_eq!(*slot, design.slot());
        assert_eq!(*design, BannerDesign::plain(15));
        assert_eq!(batches[slot].len(), 3 * 36);
    }

    #[test]
    fn a_wall_banner_has_no_pole() {
        let (batches, _) = draw(&entry(WALL_BANNER, 3), &EntityTable::new());
        assert_eq!(batches.values().next().map(Vec::len), Some(2 * 36));
    }

    #[test]
    fn the_design_comes_from_the_banner_update_only() {
        // `Base` = 1 (red) as an int tag.
        let nbt = vec![10, 0, 0, 3, 0, 4, b'B', b'a', b's', b'e', 0, 0, 0, 1, 0];
        let mut table = EntityTable::new();
        table.insert(
            (4, 70, 9),
            BlockEntityData {
                action: 6,
                nbt: Some(nbt.clone()),
            },
        );
        let (_, designs) = draw(&entry(STANDING_BANNER, 0), &table);
        assert!(designs
            .values()
            .all(|design| *design == BannerDesign::plain(1)));
        // The same bytes under another block entity type are not a banner.
        table.insert(
            (4, 70, 9),
            BlockEntityData {
                action: 4,
                nbt: Some(nbt),
            },
        );
        let (_, designs) = draw(&entry(STANDING_BANNER, 0), &table);
        assert!(designs
            .values()
            .all(|design| *design == BannerDesign::plain(15)));
    }

    #[test]
    fn banners_at_different_places_sway_out_of_step() {
        assert_ne!(sway_phase([0, 0, 0], 0.0), sway_phase([1, 0, 0], 0.0));
        assert_eq!(sway_phase([0, 0, 0], 2.0), 40.0);
    }
}

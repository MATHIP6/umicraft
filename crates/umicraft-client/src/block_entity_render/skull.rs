// SPDX-License-Identifier: MIT OR Apache-2.0

//! Skulls placed in the world (MCP `TileEntitySkullRenderer`,
//! `TileEntitySkull.readFromNBT`).
//!
//! The head is the model and texture of the worn skull
//! (`entity_render::skull`): `SkullType` picks them, and a player skull wears
//! the skin of its `Owner` once that is downloaded. The block metadata is the
//! facing (1 floor, 2..5 wall) and the `Rot` byte turns a floor skull in
//! sixteenths of a circle. A skull whose tile entity never sent an update
//! draws as a skeleton skull, which is what a fresh `TileEntitySkull` is.

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use umicraft_game::item_nbt::skull_textures;
use umicraft_game::nbt::Nbt;
use umicraft_mesh::BlockEntityRef;
use umicraft_model::entity::{ModelVertex, TextureSlot};
use umicraft_render::EntityVertex;

use super::{push_at_block, BlockEntityScene};
use crate::entity_render::{owner_skin_slot, skull_part, skull_texture, SkinLookup};

/// The `action` of an Update Block Entity packet for a skull.
const SKULL_ACTION: u8 = 4;
/// Pixels per block: the models are in sixteenths.
const PIXEL: f32 = 0.0625;

/// What the tile entity says about a skull.
#[derive(Debug, Default, PartialEq, Eq)]
struct SkullData {
    /// `SkullType`: 0 skeleton, 1 wither skeleton, 2 zombie, 3 player,
    /// 4 creeper.
    kind: i16,
    /// `Rot`, the turn of a floor skull in sixteenths of a circle.
    rotation: i8,
    /// The `textures` property of the `Owner` profile, base64 JSON.
    owner_textures: Option<String>,
}

impl SkullData {
    /// Reads the update of a skull; anything unreadable is the default
    /// skeleton skull, never an error (the bytes come from the network).
    fn from_nbt(bytes: Option<&[u8]>) -> Self {
        let Some(root) = bytes.and_then(|bytes| Nbt::parse(bytes).ok()) else {
            return Self::default();
        };
        let byte = |key| {
            root.get(key)
                .and_then(Nbt::as_int)
                .map_or(0, |value| value as i8)
        };
        Self {
            kind: i16::from(byte("SkullType")),
            rotation: byte("Rot"),
            owner_textures: root.get("Owner").and_then(skull_textures),
        }
    }
}

/// Draws the skull at `entry`.
pub(super) fn add_skull(
    batches: &mut HashMap<TextureSlot, Vec<EntityVertex>>,
    entry: &BlockEntityRef,
    scene: &BlockEntityScene<'_>,
) {
    let [x, y, z] = entry.position;
    let bytes = scene
        .data
        .get(&(x, y, z))
        .filter(|data| data.action == SKULL_ACTION)
        .and_then(|data| data.nbt.as_deref());
    let data = SkullData::from_nbt(bytes);
    let (slot, humanoid) = skull_texture(data.kind);
    let slot = skin_slot(&data, slot, scene.skins);
    let degrees = f32::from(data.rotation) * 360.0 / 16.0;
    push_at_block(
        batches.entry(slot).or_default(),
        skull_vertices(entry.state.metadata(), degrees, humanoid),
        entry,
    );
}

/// The texture of a skull: the owner's skin for a player skull when it is
/// ready, otherwise the one of its type.
fn skin_slot(data: &SkullData, slot: TextureSlot, skins: &dyn SkinLookup) -> TextureSlot {
    if slot != TextureSlot::Steve {
        return slot;
    }
    data.owner_textures
        .as_deref()
        .and_then(|value| owner_skin_slot(value, skins))
        .unwrap_or(slot)
}

/// The head in block space for the facing in `metadata` (`& 7`) and the
/// turn `floor_degrees` of a floor skull.
///
/// A floor skull stands on the block's centre. A wall skull is pushed to the
/// wall it hangs on (offsets 0.26 and 0.74) at a quarter block high, turned
/// to its facing. The renderer applies `scale(-1, -1, 1)`; the crate's boxes
/// are already y-up, so only the mirror in x is left.
fn skull_vertices(metadata: u8, floor_degrees: f32, humanoid: bool) -> Vec<ModelVertex> {
    // `EnumFacing.getFront`: 0 down, 1 up, 2 north, 3 south, 4 west, 5 east,
    // and the index wraps at 6.
    let (offset, degrees) = match (metadata & 7) % 6 {
        1 => (Vec3::new(0.5, 0.0, 0.5), floor_degrees),
        2 => (Vec3::new(0.5, 0.25, 0.74), 0.0),
        3 => (Vec3::new(0.5, 0.25, 0.26), 180.0),
        4 => (Vec3::new(0.74, 0.25, 0.5), 270.0),
        // East, and down, which the reference's `default` treats as east.
        _ => (Vec3::new(0.26, 0.25, 0.5), 90.0),
    };
    let root = Mat4::from_translation(offset)
        * Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0))
        * Mat4::from_rotation_y(degrees.to_radians())
        * Mat4::from_scale(Vec3::splat(PIXEL));
    let mut out = Vec::new();
    skull_part([0.0; 3], humanoid).emit(root, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use umicraft_game::server::block_entities::{BlockEntityData, EntityTable, SignTable};
    use umicraft_mesh::SameNeighbours;
    use umicraft_world::{BlockState, LightLevel};

    use super::*;
    use crate::entity_render::NoSkins;

    /// A compound with `SkullType` and `Rot` bytes.
    fn nbt(kind: i8, rot: i8) -> Vec<u8> {
        let mut out = vec![10, 0, 0];
        for (name, value) in [("SkullType", kind), ("Rot", rot)] {
            out.extend([1, 0, name.len() as u8]);
            out.extend(name.as_bytes());
            out.push(value as u8);
        }
        out.push(0);
        out
    }

    fn skull(metadata: u8) -> BlockEntityRef {
        BlockEntityRef {
            position: [4, 70, 9],
            state: BlockState::new(144, metadata),
            same_neighbour: SameNeighbours::default(),
            light: LightLevel { block: 0, sky: 15 },
        }
    }

    fn draw(entry: &BlockEntityRef, bytes: Option<Vec<u8>>, skins: &dyn SkinLookup) -> Batches {
        let mut table = EntityTable::new();
        if let Some(nbt) = bytes {
            table.insert(
                (4, 70, 9),
                BlockEntityData {
                    action: 4,
                    nbt: Some(nbt),
                },
            );
        }
        let lids = umicraft_game::server::chest_lids::ChestLids::default();
        let scene = BlockEntityScene {
            data: &table,
            seconds: 0.0,
            lids: &lids,
            signs: &SignTable::new(),
            skins,
            font: None,
            partial_tick: 0.0,
            camera: Vec3::ZERO,
        };
        let mut batches = HashMap::new();
        add_skull(&mut batches, entry, &scene);
        batches
    }

    type Batches = HashMap<TextureSlot, Vec<EntityVertex>>;

    /// The horizontal extent of a mesh as `(min, max)` along `axis`.
    fn extent(vertices: &[ModelVertex], axis: usize) -> (f32, f32) {
        let values = vertices.iter().map(|v| v.position[axis]);
        (
            values.clone().fold(f32::MAX, f32::min),
            values.fold(f32::MIN, f32::max),
        )
    }

    #[test]
    fn a_floor_skull_stands_on_the_block_and_turns_with_rot() {
        let square = skull_vertices(1, 0.0, false);
        assert_eq!(square.len(), 36);
        let (low, high) = extent(&square, 1);
        assert!(low.abs() < 1e-5 && (high - 0.5).abs() < 1e-5);
        // A quarter turn (Rot 4) swaps nothing for a cube about its centre,
        // but the front face moves: compare the first vertex.
        let turned = skull_vertices(1, 4.0 * 360.0 / 16.0, false);
        let half = skull_vertices(1, 8.0 * 360.0 / 16.0, false);
        assert_ne!(square[0].position, turned[0].position);
        assert_ne!(square[0].position, half[0].position);
        // A half turn about the centre mirrors x and z around 0.5.
        for (a, b) in square.iter().zip(&half) {
            assert!((a.position[0] + b.position[0] - 1.0).abs() < 1e-4);
            assert!((a.position[2] + b.position[2] - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn a_wall_skull_is_pushed_to_its_wall() {
        let centre = |meta| {
            let vertices = skull_vertices(meta, 0.0, false);
            let (x0, x1) = extent(&vertices, 0);
            let (y0, y1) = extent(&vertices, 1);
            let (z0, z1) = extent(&vertices, 2);
            [(x0 + x1) / 2.0, (y0 + y1) / 2.0, (z0 + z1) / 2.0]
        };
        let near = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-4);
        assert!(near(centre(2), [0.5, 0.5, 0.74]), "north");
        assert!(near(centre(3), [0.5, 0.5, 0.26]), "south");
        assert!(near(centre(4), [0.74, 0.5, 0.5]), "west");
        assert!(near(centre(5), [0.26, 0.5, 0.5]), "east");
        assert!(near(centre(0), centre(5)), "down draws as east");
        // The metadata bit 3 is not part of the facing.
        assert!(near(centre(8 | 2), centre(2)));
    }

    #[test]
    fn the_type_picks_the_texture_and_the_model() {
        let entry = skull(1);
        let slots = [
            (0, TextureSlot::Skeleton, 36),
            (1, TextureSlot::SkeletonWither, 36),
            (2, TextureSlot::Zombie, 72),
            (3, TextureSlot::Steve, 72),
            (4, TextureSlot::Creeper, 36),
        ];
        for (kind, slot, vertices) in slots {
            let batches = draw(&entry, Some(nbt(kind, 0)), &NoSkins);
            assert_eq!(batches.len(), 1);
            assert_eq!(batches[&slot].len(), vertices, "type {kind}");
        }
    }

    #[test]
    fn a_skull_without_an_update_or_with_bad_nbt_is_a_skeleton() {
        let entry = skull(2);
        for bytes in [None, Some(vec![10, 0]), Some(vec![1, 2, 3])] {
            let batches = draw(&entry, bytes, &NoSkins);
            assert_eq!(batches[&TextureSlot::Skeleton].len(), 36);
        }
        assert_eq!(SkullData::from_nbt(Some(&[0xff])), SkullData::default());
    }

    #[test]
    fn rot_is_read_as_a_signed_byte() {
        let data = SkullData::from_nbt(Some(&nbt(3, 4)));
        assert_eq!((data.kind, data.rotation), (3, 4));
        let data = SkullData::from_nbt(Some(&nbt(-1, -2)));
        assert_eq!((data.kind, data.rotation), (-1, -2));
    }

    #[test]
    fn a_player_skull_uses_the_owner_skin_when_ready() {
        struct Ready;
        impl SkinLookup for Ready {
            fn skin(&self, url: &str) -> Option<u32> {
                (url == "http://t/skin").then_some(7)
            }
            fn cape(&self, _url: &str) -> Option<u32> {
                None
            }
        }
        let value = "eyJ0ZXh0dXJlcyI6eyJTS0lOIjp7InVybCI6Imh0dHA6Ly90L3NraW4ifX19";
        let mut bytes = nbt(3, 0);
        // Insert an Owner compound before the closing End tag.
        bytes.pop();
        bytes.extend([10, 0, 5]);
        bytes.extend(b"Owner");
        bytes.extend([10, 0, 10]);
        bytes.extend(b"Properties");
        bytes.extend([9, 0, 8]);
        bytes.extend(b"textures");
        bytes.extend([10, 0, 0, 0, 1, 8, 0, 5]);
        bytes.extend(b"Value");
        bytes.extend((value.len() as u16).to_be_bytes());
        bytes.extend(value.as_bytes());
        // The texture, Properties, Owner and the root.
        bytes.extend([0, 0, 0, 0]);
        let entry = skull(1);
        assert!(draw(&entry, Some(bytes.clone()), &Ready).contains_key(&TextureSlot::Skin(7)));
        assert!(draw(&entry, Some(bytes), &NoSkins).contains_key(&TextureSlot::Steve));
    }
}

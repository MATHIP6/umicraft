// SPDX-License-Identifier: MIT OR Apache-2.0

//! Map item data: a 128x128 colour buffer and the decorations per map id.
//!
//! Source: MCP `S34PacketMaps.setMapdataTo` (scale, decorations replaced,
//! rectangular colour patch copied into the buffer) and `MapData`. The
//! reference indexes the buffer without checks and throws on a hostile
//! rectangle; here an out-of-range rectangle or a short data array drops the
//! colour patch (the icons and scale still apply) and never panics.

use std::collections::BTreeMap;
use std::sync::Arc;

use umicraft_protocol::packets::play::clientbound::maps::{MapColumns, MapIcon, Maps};

use super::map_colors::map_pixel;

/// Side length of a map in pixels.
pub const MAP_SIZE: usize = 128;
/// Number of pixels of a map.
const PIXELS: usize = MAP_SIZE * MAP_SIZE;
/// Most maps kept at once. Vanilla has no limit, but ids are server-chosen
/// and each map costs 16 KiB.
pub const MAX_MAPS: usize = 1024;

/// The data of one map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapData {
    /// The zoom level (0 = 1:1).
    pub scale: i8,
    /// The decorations (player markers, frames).
    pub icons: Vec<MapIcon>,
    /// Shared copy-on-write so a frame snapshot of the store is cheap.
    colors: Arc<Vec<u8>>,
    revision: u64,
}

impl Default for MapData {
    fn default() -> Self {
        Self {
            scale: 0,
            icons: Vec::new(),
            colors: Arc::new(vec![0; PIXELS]),
            revision: 0,
        }
    }
}

impl MapData {
    /// The colour indices, row-major, `MAP_SIZE * MAP_SIZE` bytes.
    pub fn colors(&self) -> &[u8] {
        &self.colors
    }

    /// A number that changes with every packet that touches this map, so a
    /// texture cache knows when to upload again. Unique across the store.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The map as `MAP_SIZE * MAP_SIZE` RGBA pixels (MCP
    /// `MapItemRenderer.Instance.updateMapTexture`).
    pub fn rgba(&self) -> Vec<u8> {
        self.colors
            .iter()
            .enumerate()
            .flat_map(|(pixel, &color)| map_pixel(color, pixel))
            .collect()
    }

    /// The colour index at `(x, z)`, `None` outside the map.
    pub fn color_at(&self, x: usize, z: usize) -> Option<u8> {
        if x >= MAP_SIZE || z >= MAP_SIZE {
            return None;
        }
        self.colors.get(x + z * MAP_SIZE).copied()
    }

    /// Copies a column patch into the buffer if it fits and is complete.
    fn merge(&mut self, patch: &MapColumns) {
        let (columns, rows) = (usize::from(patch.columns), usize::from(patch.rows));
        let (x0, z0) = (usize::from(patch.x), usize::from(patch.z));
        if x0 + columns > MAP_SIZE || z0 + rows > MAP_SIZE || patch.data.len() < columns * rows {
            return;
        }
        let colors = Arc::make_mut(&mut self.colors);
        for row in 0..rows {
            let source = &patch.data[row * columns..(row + 1) * columns];
            let start = x0 + (z0 + row) * MAP_SIZE;
            colors[start..start + columns].copy_from_slice(source);
        }
    }
}

/// All maps the server sent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MapStore {
    maps: BTreeMap<i32, MapData>,
    revisions: u64,
}

impl MapStore {
    /// Applies a map packet (0x34). A new id beyond [`MAX_MAPS`] is ignored.
    pub fn apply(&mut self, packet: &Maps) {
        if !self.maps.contains_key(&packet.map_id) && self.maps.len() >= MAX_MAPS {
            return;
        }
        self.revisions += 1;
        let map = self.maps.entry(packet.map_id).or_default();
        map.revision = self.revisions;
        map.scale = packet.scale;
        map.icons.clone_from(&packet.icons);
        if let Some(patch) = &packet.columns {
            map.merge(patch);
        }
    }

    /// The map with this id.
    pub fn get(&self, map_id: i32) -> Option<&MapData> {
        self.maps.get(&map_id)
    }

    /// Every map by id.
    pub fn iter(&self) -> impl Iterator<Item = (i32, &MapData)> {
        self.maps.iter().map(|(&id, map)| (id, map))
    }

    /// The number of known maps.
    pub fn len(&self) -> usize {
        self.maps.len()
    }

    /// Whether no map is known.
    pub fn is_empty(&self) -> bool {
        self.maps.is_empty()
    }

    /// Forgets every map (the dimension changed).
    pub fn clear(&mut self) {
        self.maps.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(map_id: i32, columns: Option<MapColumns>) -> Maps {
        Maps {
            map_id,
            scale: 2,
            icons: vec![MapIcon {
                icon_type: 1,
                direction: 3,
                x: -5,
                z: 7,
            }],
            columns,
        }
    }

    fn patch(columns: u8, rows: u8, x: u8, z: u8, data: Vec<u8>) -> MapColumns {
        MapColumns {
            columns,
            rows,
            x,
            z,
            data,
        }
    }

    #[test]
    fn a_patch_is_merged_at_its_offset() {
        let mut maps = MapStore::default();
        maps.apply(&packet(3, Some(patch(2, 2, 10, 20, vec![1, 2, 3, 4]))));
        let map = maps.get(3).unwrap();
        assert_eq!(map.color_at(10, 20), Some(1));
        assert_eq!(map.color_at(11, 20), Some(2));
        assert_eq!(map.color_at(10, 21), Some(3));
        assert_eq!(map.color_at(11, 21), Some(4));
        assert_eq!(map.color_at(12, 20), Some(0));
        assert_eq!((map.scale, map.icons.len()), (2, 1));
    }

    #[test]
    fn a_second_patch_keeps_the_rest_of_the_buffer() {
        let mut maps = MapStore::default();
        maps.apply(&packet(1, Some(patch(1, 1, 0, 0, vec![9]))));
        maps.apply(&packet(1, Some(patch(1, 1, 127, 127, vec![8]))));
        let map = maps.get(1).unwrap();
        assert_eq!(map.color_at(0, 0), Some(9));
        assert_eq!(map.color_at(127, 127), Some(8));
        assert_eq!(map.colors().len(), MAP_SIZE * MAP_SIZE);
    }

    #[test]
    fn icons_only_updates_keep_the_colours() {
        let mut maps = MapStore::default();
        maps.apply(&packet(1, Some(patch(1, 1, 0, 0, vec![9]))));
        maps.apply(&packet(1, None));
        assert_eq!(maps.get(1).unwrap().color_at(0, 0), Some(9));
    }

    #[test]
    fn hostile_patches_are_dropped_without_panicking() {
        let mut maps = MapStore::default();
        let hostile = [
            patch(255, 255, 255, 255, vec![1; 10]),
            patch(2, 2, 127, 0, vec![1; 4]),
            patch(2, 2, 0, 127, vec![1; 4]),
            patch(4, 4, 0, 0, vec![1; 3]),
            patch(255, 255, 0, 0, vec![]),
        ];
        for bad in hostile {
            maps.apply(&packet(1, Some(bad)));
        }
        let map = maps.get(1).unwrap();
        assert!(map.colors().iter().all(|&c| c == 0));
        assert_eq!(map.icons.len(), 1);
    }

    #[test]
    fn a_zero_sized_patch_is_harmless_and_color_at_is_bounds_checked() {
        let mut maps = MapStore::default();
        maps.apply(&packet(1, Some(patch(0, 5, 0, 0, vec![]))));
        let map = maps.get(1).unwrap();
        assert_eq!(map.color_at(128, 0), None);
        assert_eq!(map.color_at(0, 128), None);
    }

    #[test]
    fn the_map_count_is_capped_and_clear_forgets_all() {
        let mut maps = MapStore::default();
        for id in 0..(MAX_MAPS as i32 + 10) {
            maps.apply(&packet(id, None));
        }
        assert_eq!(maps.len(), MAX_MAPS);
        maps.apply(&packet(0, None));
        assert_eq!(maps.len(), MAX_MAPS);
        maps.clear();
        assert!(maps.is_empty());
    }

    #[test]
    fn every_packet_gives_its_map_a_new_revision() {
        let mut maps = MapStore::default();
        maps.apply(&packet(1, None));
        let first = maps.get(1).unwrap().revision();
        maps.apply(&packet(2, None));
        maps.apply(&packet(1, None));
        let second = maps.get(1).unwrap().revision();
        assert_ne!(first, second);
        assert_ne!(maps.get(2).unwrap().revision(), second);
    }

    #[test]
    fn the_rgba_image_follows_the_colour_table() {
        let mut maps = MapStore::default();
        maps.apply(&packet(1, Some(patch(1, 1, 1, 0, vec![6]))));
        let image = maps.get(1).unwrap().rgba();
        assert_eq!(image.len(), PIXELS * 4);
        assert_eq!(&image[4..8], &[127, 178, 56, 255]);
        assert_eq!(&image[0..4], &[0, 0, 0, 16]);
    }

    #[test]
    fn a_snapshot_keeps_the_old_pixels_after_a_later_patch() {
        let mut maps = MapStore::default();
        maps.apply(&packet(1, Some(patch(1, 1, 0, 0, vec![9]))));
        let snapshot = maps.clone();
        maps.apply(&packet(1, Some(patch(1, 1, 0, 0, vec![8]))));
        assert_eq!(snapshot.get(1).unwrap().color_at(0, 0), Some(9));
        assert_eq!(maps.get(1).unwrap().color_at(0, 0), Some(8));
    }
}

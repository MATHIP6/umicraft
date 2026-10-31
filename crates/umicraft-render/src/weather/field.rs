// SPDX-License-Identifier: MIT OR Apache-2.0

//! The precipitation data the game thread publishes for the weather pass.
//!
//! The reference asks the world for the precipitation height, the biome and
//! the light of every column around the camera in `renderRainSnow`, on every
//! frame. Here the game thread samples them once per tick into this grid, so
//! the render thread never touches the world.

/// Columns sampled on each side of the centre. The widest precipitation
/// radius is 10 blocks; the rest covers the camera moving away from the
/// player (third person) and the movement within a tick.
pub const FIELD_RADIUS: i32 = 12;

/// Cells along one side of the grid.
const SIDE: usize = (FIELD_RADIUS * 2 + 1) as usize;

/// What the weather pass needs to know about one column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrecipitationCell {
    /// `World.getPrecipitationHeight`: the first block height rain reaches.
    pub height: i32,
    /// `BiomeGenBase.temperature` of the column, before the height term.
    pub temperature: f32,
    /// Sky light in `0..=15` at the precipitation height.
    pub sky_light: u8,
    /// Block light in `0..=15` at the precipitation height.
    pub block_light: u8,
}

/// A square grid of columns centred on the player, `None` where the biome
/// has no precipitation or the chunk is not loaded.
///
/// The default is an empty grid, which draws nothing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PrecipitationField {
    origin: [i32; 2],
    cells: Vec<Option<PrecipitationCell>>,
}

impl PrecipitationField {
    /// A grid of empty cells centred on the block column `x`, `z`.
    #[must_use]
    pub fn new(x: i32, z: i32) -> Self {
        Self {
            origin: [x - FIELD_RADIUS, z - FIELD_RADIUS],
            cells: vec![None; SIDE * SIDE],
        }
    }

    /// The index of a column, `None` outside the grid.
    fn index(&self, x: i32, z: i32) -> Option<usize> {
        let side = SIDE as i32;
        let (dx, dz) = (x - self.origin[0], z - self.origin[1]);
        if self.cells.is_empty() || !(0..side).contains(&dx) || !(0..side).contains(&dz) {
            return None;
        }
        Some((dz * side + dx) as usize)
    }

    /// Stores the cell of column `x`, `z`; a column outside the grid is ignored.
    pub fn set(&mut self, x: i32, z: i32, cell: PrecipitationCell) {
        if let Some(index) = self.index(x, z) {
            self.cells[index] = Some(cell);
        }
    }

    /// The cell of column `x`, `z`, if the grid holds one.
    #[must_use]
    pub fn get(&self, x: i32, z: i32) -> Option<&PrecipitationCell> {
        self.index(x, z)
            .and_then(|index| self.cells[index].as_ref())
    }

    /// True when no column has precipitation.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.iter().all(Option::is_none)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CELL: PrecipitationCell = PrecipitationCell {
        height: 64,
        temperature: 0.8,
        sky_light: 15,
        block_light: 0,
    };

    #[test]
    fn the_default_grid_is_empty_and_answers_nothing() {
        let field = PrecipitationField::default();
        assert!(field.is_empty());
        assert_eq!(field.get(0, 0), None);
    }

    #[test]
    fn cells_are_found_by_world_column() {
        let mut field = PrecipitationField::new(100, -50);
        field.set(100, -50, CELL);
        field.set(112, -62, PrecipitationCell { height: 70, ..CELL });
        assert_eq!(field.get(100, -50), Some(&CELL));
        assert_eq!(field.get(112, -62).map(|cell| cell.height), Some(70));
        assert_eq!(field.get(101, -50), None);
        assert!(!field.is_empty());
    }

    #[test]
    fn columns_beyond_the_radius_are_ignored() {
        let mut field = PrecipitationField::new(0, 0);
        field.set(13, 0, CELL);
        field.set(0, -13, CELL);
        assert!(field.is_empty());
        assert_eq!(field.get(13, 0), None);
    }
}

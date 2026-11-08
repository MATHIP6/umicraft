// SPDX-License-Identifier: MIT OR Apache-2.0

//! Which faces of a 16³ section see each other (F2-3d, ADR-0062).
//!
//! Two faces are *connected* when a path of non-opaque cells joins a cell on one
//! to a cell on the other. The renderer walks these connections from the camera
//! section to find the sections that can be seen at all (the behaviour of the
//! vanilla 1.8 `VisGraph`/`SetVisibility`; this code is original).
//!
//! Faces are numbered like [`Face`](crate::Face): west, east, down, up, north,
//! south, so the opposite of face `f` is `f ^ 1`.

use umicraft_world::ChunkPos;

use crate::BlockSource;

/// Cells per section side.
const SIDE: usize = 16;
/// Cells in a section.
const CELLS: usize = SIDE * SIDE * SIDE;
/// All six faces.
const ALL_FACES: u8 = 0b11_1111;

/// For each face, the set of faces it is connected to (a bit per face).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Connectivity([u8; 6]);

impl Connectivity {
    /// Every face sees every other: an empty section.
    pub const OPEN: Self = Self([ALL_FACES; 6]);
    /// No face sees any other: a section filled with opaque blocks.
    pub const CLOSED: Self = Self([0; 6]);

    /// Whether `from` and `to` (face numbers `0..6`) are connected.
    #[must_use]
    pub fn connects(self, from: usize, to: usize) -> bool {
        self.0[from] >> to & 1 != 0
    }

    /// The connectivity of section `section` (`0..16`) of column `pos`.
    ///
    /// Opaque cells are those of [`BlockState::is_opaque`](umicraft_world::BlockState::is_opaque):
    /// everything else, including water and glass, lets the view through.
    #[must_use]
    pub fn of_section<S: BlockSource + ?Sized>(source: &S, pos: ChunkPos, section: usize) -> Self {
        if source.section_is_empty(pos, section) {
            return Self::OPEN;
        }
        let (base_x, base_y, base_z) = (pos.x * 16, section as i32 * 16, pos.z * 16);
        let mut blocked = [false; CELLS];
        for y in 0..SIDE {
            for z in 0..SIDE {
                for x in 0..SIDE {
                    blocked[cell(x, y, z)] = source
                        .block(base_x + x as i32, base_y + y as i32, base_z + z as i32)
                        .is_opaque();
                }
            }
        }
        Self::from_blocked(&mut blocked)
    }

    /// Flood fills the open cells of `blocked` (`true` = opaque; the array is
    /// used as the visited set and left filled).
    fn from_blocked(blocked: &mut [bool; CELLS]) -> Self {
        let mut reach = [0_u8; 6];
        let mut stack = Vec::with_capacity(256);
        for start in 0..CELLS {
            if blocked[start] {
                continue;
            }
            let mut touched = 0_u8;
            blocked[start] = true;
            stack.push(start as u16);
            while let Some(index) = stack.pop() {
                let index = usize::from(index);
                let (x, y, z) = (index & 15, index >> 8, index >> 4 & 15);
                touched |= face_bits(x, y, z);
                for (dx, dy, dz) in NEIGHBOURS {
                    let (nx, ny, nz) = (
                        x.wrapping_add_signed(dx),
                        y.wrapping_add_signed(dy),
                        z.wrapping_add_signed(dz),
                    );
                    if nx < SIDE && ny < SIDE && nz < SIDE && !blocked[cell(nx, ny, nz)] {
                        blocked[cell(nx, ny, nz)] = true;
                        stack.push(cell(nx, ny, nz) as u16);
                    }
                }
            }
            for (face, faces) in reach.iter_mut().enumerate() {
                if touched >> face & 1 != 0 {
                    *faces |= touched;
                }
            }
        }
        Self(reach)
    }
}

const NEIGHBOURS: [(isize, isize, isize); 6] = [
    (-1, 0, 0),
    (1, 0, 0),
    (0, -1, 0),
    (0, 1, 0),
    (0, 0, -1),
    (0, 0, 1),
];

/// The index of a cell: `y * 256 + z * 16 + x`.
const fn cell(x: usize, y: usize, z: usize) -> usize {
    y << 8 | z << 4 | x
}

/// The faces of the section a cell lies on.
fn face_bits(x: usize, y: usize, z: usize) -> u8 {
    let edge = |value: usize, low: u8, high: u8| {
        (if value == 0 { low } else { 0 }) | (if value == SIDE - 1 { high } else { 0 })
    };
    edge(x, 1 << 0, 1 << 1) | edge(y, 1 << 2, 1 << 3) | edge(z, 1 << 4, 1 << 5)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_grid() -> Box<[bool; CELLS]> {
        Box::new([false; CELLS])
    }

    fn wall_x(blocked: &mut [bool; CELLS], x: usize) {
        for y in 0..SIDE {
            for z in 0..SIDE {
                blocked[cell(x, y, z)] = true;
            }
        }
    }

    #[test]
    fn an_empty_section_connects_every_pair() {
        assert_eq!(
            Connectivity::from_blocked(&mut open_grid()),
            Connectivity::OPEN
        );
    }

    #[test]
    fn a_full_section_connects_nothing() {
        assert_eq!(
            Connectivity::from_blocked(&mut Box::new([true; CELLS])),
            Connectivity::CLOSED
        );
    }

    #[test]
    fn a_wall_splits_west_from_east_but_keeps_the_rest() {
        let mut blocked = open_grid();
        wall_x(&mut blocked, 8);
        let split = Connectivity::from_blocked(&mut blocked);
        assert!(!split.connects(0, 1), "west and east are cut");
        assert!(split.connects(0, 3), "west still sees up");
        assert!(split.connects(1, 3), "east still sees up");
        assert!(split.connects(2, 4), "down sees north");
    }

    #[test]
    fn a_tunnel_joins_only_its_two_ends() {
        // Solid except a one-cell tunnel along x at y = z = 5.
        let mut blocked = Box::new([true; CELLS]);
        for x in 0..SIDE {
            blocked[cell(x, 5, 5)] = false;
        }
        let tunnel = Connectivity::from_blocked(&mut blocked);
        assert!(tunnel.connects(0, 1) && tunnel.connects(1, 0));
        assert!(!tunnel.connects(0, 3));
        assert!(!tunnel.connects(2, 3));
    }

    #[test]
    fn a_sealed_pocket_touches_no_face() {
        let mut blocked = Box::new([true; CELLS]);
        blocked[cell(8, 8, 8)] = false;
        assert_eq!(
            Connectivity::from_blocked(&mut blocked),
            Connectivity::CLOSED
        );
    }
}

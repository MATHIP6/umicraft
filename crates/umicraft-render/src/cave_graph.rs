// SPDX-License-Identifier: MIT OR Apache-2.0

//! Cave culling: which 16³ sections the camera can possibly see (F2-3d,
//! ADR-0062).
//!
//! A flood fill from the camera section. It crosses a face of a section only if
//! the section's [`Connectivity`] joins the face the fill entered by to that face
//! (the camera section may be left through any face). A section is reached per
//! face it can be entered by, so a section visible through a side door is not
//! lost because a first path was closed. Missing chunks are open.
//!
//! The fill is not limited by the frustum, which is more conservative than
//! vanilla's `RenderGlobal.setupTerrain`, and does not forbid turning back, which
//! only reveals more. The result is cached until the camera changes section or
//! a mesh is uploaded or removed.

use std::collections::VecDeque;

use umicraft_mesh::visibility::Connectivity;
use umicraft_world::ChunkPos;

use crate::chunk_pass::SECTIONS;

/// The six faces as steps: west, east, down, up, north, south (the order of
/// [`Connectivity`]).
const STEPS: [(i32, i32, i32); 6] = [
    (-1, 0, 0),
    (1, 0, 0),
    (0, -1, 0),
    (0, 1, 0),
    (0, 0, -1),
    (0, 0, 1),
];
/// "Entered from nowhere": the camera section.
const START: usize = 6;

/// What the fill was computed for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Key {
    camera: (i32, i32, i32),
    reach: i32,
    version: u64,
}

/// The cached result of the fill: a mask of visible sections per chunk of the
/// square of `2 * reach + 1` chunks around the camera.
#[derive(Default)]
pub(crate) struct CaveGraph {
    /// The visible sections of each chunk of the square, row by row.
    visible: Vec<u16>,
    /// The south-west chunk of the square.
    corner: (i32, i32),
    side: i32,
    key: Option<Key>,
    /// Faces each section was already entered by (a bit per face), to visit it
    /// once per face; indexed like [`CaveGraph::cell`].
    entered: Vec<u8>,
    queue: VecDeque<(usize, usize)>,
}

impl CaveGraph {
    /// Recomputes the visible sections for a camera at block `camera`, only when
    /// the camera section, `reach` (in chunks) or `version` (bumped when meshes
    /// change) changed. `connectivity` answers for a chunk or `None` when it has
    /// no mesh (treated as open).
    pub(crate) fn update<'a>(
        &mut self,
        camera: (f64, f64, f64),
        reach: i32,
        version: u64,
        connectivity: impl Fn(ChunkPos) -> Option<&'a [Connectivity; SECTIONS]>,
    ) {
        let section_of = |value: f64| (value / 16.0).floor() as i32;
        let start = (
            section_of(camera.0),
            section_of(camera.1).clamp(0, SECTIONS as i32 - 1),
            section_of(camera.2),
        );
        let key = Key {
            camera: start,
            reach,
            version,
        };
        if self.key != Some(key) {
            self.fill(start, reach, &connectivity);
            self.key = Some(key);
        }
    }

    /// The visible sections of chunk `pos` (bit `s` = section `s`); `0` outside
    /// the square or when nothing of it can be seen.
    pub(crate) fn mask(&self, pos: ChunkPos) -> u16 {
        let (dx, dz) = (pos.x - self.corner.0, pos.z - self.corner.1);
        if (0..self.side).contains(&dx) && (0..self.side).contains(&dz) {
            self.visible[(dz * self.side + dx) as usize]
        } else {
            0
        }
    }

    /// The index of section `sy` of the chunk at grid cell `(dx, dz)`.
    fn cell(&self, dx: i32, sy: i32, dz: i32) -> usize {
        ((dz * self.side + dx) as usize) * SECTIONS + sy as usize
    }

    fn fill<'a>(
        &mut self,
        start: (i32, i32, i32),
        reach: i32,
        connectivity: &impl Fn(ChunkPos) -> Option<&'a [Connectivity; SECTIONS]>,
    ) {
        self.side = 2 * reach + 1;
        self.corner = (start.0 - reach, start.2 - reach);
        let columns = (self.side * self.side) as usize;
        self.visible.clear();
        self.visible.resize(columns, 0);
        self.entered.clear();
        self.entered.resize(columns * SECTIONS, 0);
        self.queue.clear();
        // One lookup per chunk, not per section.
        let known: Vec<Option<&[Connectivity; SECTIONS]>> = (0..self.side * self.side)
            .map(|index| {
                let (dx, dz) = (index % self.side, index / self.side);
                connectivity(ChunkPos::new(self.corner.0 + dx, self.corner.1 + dz))
            })
            .collect();
        let first = self.cell(reach, start.1, reach);
        self.queue.push_back((first, START));
        while let Some((index, entry)) = self.queue.pop_front() {
            let (column, sy) = (index / SECTIONS, (index % SECTIONS) as i32);
            let (dx, dz) = ((column as i32) % self.side, (column as i32) / self.side);
            let here = known[column].map(|all| all[sy as usize]);
            if here.is_some() {
                self.visible[column] |= 1 << sy;
            }
            for (face, step) in STEPS.iter().enumerate() {
                let open = entry == START || here.is_none_or(|conn| conn.connects(entry, face));
                if !open {
                    continue;
                }
                let (nx, ny, nz) = (dx + step.0, sy + step.1, dz + step.2);
                let inside = (0..SECTIONS as i32).contains(&ny)
                    && (0..self.side).contains(&nx)
                    && (0..self.side).contains(&nz);
                if !inside {
                    continue;
                }
                let entered_by = face ^ 1;
                let next = self.cell(nx, ny, nz);
                if self.entered[next] >> entered_by & 1 == 0 {
                    self.entered[next] |= 1 << entered_by;
                    self.queue.push_back((next, entered_by));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// A world of chunks that are all-open except where `closed` says.
    fn world(closed: &[(i32, usize)]) -> HashMap<ChunkPos, [Connectivity; SECTIONS]> {
        let mut map = HashMap::new();
        for x in -4..=4 {
            for z in -4..=4 {
                let mut sections = [Connectivity::OPEN; SECTIONS];
                for &(cx, sy) in closed {
                    if cx == x {
                        sections[sy] = Connectivity::CLOSED;
                    }
                }
                map.insert(ChunkPos::new(x, z), sections);
            }
        }
        map
    }

    fn visible(closed: &[(i32, usize)], camera: (f64, f64, f64)) -> HashMap<ChunkPos, u16> {
        let map = world(closed);
        let mut graph = CaveGraph::default();
        graph.update(camera, 3, 1, |pos| map.get(&pos));
        map.keys()
            .map(|pos| (*pos, graph.mask(*pos)))
            .filter(|(_, mask)| *mask != 0)
            .collect()
    }

    #[test]
    fn an_open_world_shows_every_section_in_reach() {
        let seen = visible(&[], (8.0, 8.0, 8.0));
        assert_eq!(seen[&ChunkPos::new(3, 0)], 0xFFFF);
        assert!(!seen.contains_key(&ChunkPos::new(4, 0)), "beyond the reach");
    }

    #[test]
    fn a_closed_section_is_seen_but_hides_what_is_behind_it() {
        // Every section of the chunks at x = 1 is closed, for every z: the
        // fill cannot go around the wall within the reach of 3.
        let every_section: Vec<(i32, usize)> = (0..16).map(|sy| (1, sy)).collect();
        let seen = visible(&every_section, (8.0, 8.0, 8.0));
        // The closed column is itself reached (its faces are seen)...
        assert!(seen[&ChunkPos::new(1, 0)] != 0);
        // ...but nothing past it.
        assert!(!seen.contains_key(&ChunkPos::new(2, 0)));
        assert!(seen.contains_key(&ChunkPos::new(-1, 0)));
    }

    #[test]
    fn the_camera_section_can_be_left_through_any_face_even_if_closed() {
        let seen = visible(&[(0, 0)], (8.0, 8.0, 8.0));
        assert!(seen.contains_key(&ChunkPos::new(1, 0)));
        assert!(seen.contains_key(&ChunkPos::new(-1, 0)));
    }

    #[test]
    fn the_result_is_cached_until_the_inputs_change() {
        let map = world(&[]);
        let mut graph = CaveGraph::default();
        let calls = std::cell::Cell::new(0);
        let mut run = |camera: (f64, f64, f64), version| {
            graph.update(camera, 3, version, |pos| {
                calls.set(calls.get() + 1);
                map.get(&pos)
            });
        };
        run((8.0, 8.0, 8.0), 1);
        let first = calls.get();
        run((9.0, 9.0, 9.0), 1);
        assert_eq!(calls.get(), first, "same section, same version");
        run((8.0, 8.0, 8.0), 2);
        assert!(calls.get() > first, "a new version refills");
        let second = calls.get();
        run((24.0, 8.0, 8.0), 2);
        assert!(calls.get() > second, "a new camera section refills");
    }
}

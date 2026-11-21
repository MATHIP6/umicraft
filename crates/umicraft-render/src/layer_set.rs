// SPDX-License-Identifier: MIT OR Apache-2.0

//! The atlas layers a chunk mesh draws, as a bitset (Smart Animations, F6
//! phase 3).
//!
//! The renderer records the set of every uploaded mesh and, once per change of
//! the visible lists, joins the sets of the visible chunks. The client then
//! steps only the animated block textures that are in that union: a lava
//! texture nobody can see costs no upload. OptiFine's Smart Animations has the
//! same effect (behaviour only; its source is not used).

use umicraft_mesh::Vertex;

/// Vertices that form one quad, which share their layer.
const QUAD: usize = 4;

/// A set of texture-array layers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LayerSet {
    words: Vec<u64>,
}

impl LayerSet {
    /// The layers of the quads in the given vertex runs. Only the first vertex
    /// of each quad is read: the mesher gives all four the same layer.
    #[must_use]
    pub fn of_vertices(runs: &[&[Vertex]]) -> Self {
        let mut set = Self::default();
        for vertices in runs {
            for vertex in vertices.iter().step_by(QUAD) {
                set.insert(vertex.layer);
            }
        }
        set
    }

    /// Adds `layer`.
    pub fn insert(&mut self, layer: u32) {
        let word = layer as usize / 64;
        if self.words.len() <= word {
            self.words.resize(word + 1, 0);
        }
        self.words[word] |= 1 << (layer % 64);
    }

    /// Whether `layer` is in the set.
    #[must_use]
    pub fn contains(&self, layer: u32) -> bool {
        self.words
            .get(layer as usize / 64)
            .is_some_and(|word| word >> (layer % 64) & 1 == 1)
    }

    /// Adds every layer of `other`.
    pub fn union_with(&mut self, other: &Self) {
        if self.words.len() < other.words.len() {
            self.words.resize(other.words.len(), 0);
        }
        for (word, &bits) in self.words.iter_mut().zip(&other.words) {
            *word |= bits;
        }
    }

    /// Empties the set, keeping its capacity.
    pub fn clear(&mut self) {
        self.words.clear();
    }
}

#[cfg(test)]
mod tests {
    use bytemuck::Zeroable;

    use super::*;

    fn quad(layer: u32) -> [Vertex; QUAD] {
        let mut vertex = Vertex::zeroed();
        vertex.layer = layer;
        [vertex; QUAD]
    }

    #[test]
    fn a_set_holds_the_layers_inserted_across_word_boundaries() {
        let mut set = LayerSet::default();
        for layer in [0, 63, 64, 300] {
            set.insert(layer);
        }
        assert!([0, 63, 64, 300].into_iter().all(|l| set.contains(l)));
        assert!(!set.contains(1) && !set.contains(65) && !set.contains(10_000));
    }

    #[test]
    fn the_layers_of_a_mesh_come_from_its_quads_in_both_passes() {
        let opaque: Vec<Vertex> = [quad(3), quad(70)].concat();
        let transparent = quad(5);
        let set = LayerSet::of_vertices(&[&opaque, &transparent]);
        assert!(set.contains(3) && set.contains(70) && set.contains(5));
        assert!(!set.contains(4));
    }

    #[test]
    fn a_union_joins_sets_of_different_sizes_and_clear_empties_it() {
        let mut small = LayerSet::default();
        small.insert(1);
        let mut large = LayerSet::default();
        large.insert(200);
        small.union_with(&large);
        assert!(small.contains(1) && small.contains(200));
        small.clear();
        assert!(!small.contains(1) && !small.contains(200));
    }
}

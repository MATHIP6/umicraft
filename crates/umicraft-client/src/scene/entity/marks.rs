// SPDX-License-Identifier: MIT OR Apache-2.0

//! The entities the external mods marked this frame for the entity mask of their
//! post passes (API 1.4, ADR-0097), by id.

use std::collections::HashMap;

use umicraft_game::entity::EntityId;
use umicraft_modhost::Highlight;

/// How one marked entity goes into the mask.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Mark {
    /// Straight-alpha RGBA written into the mask.
    pub(crate) color: [f32; 4],
    /// Drawn whole instead of only where the world shows it.
    pub(crate) through_walls: bool,
}

/// The marks of a frame, kept from frame to frame for its map.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct EntityMarks(HashMap<EntityId, Mark>);

impl EntityMarks {
    /// Replaces the marks with `highlights`; for an entity marked twice the later
    /// mark wins, as the mods run in load order.
    pub(crate) fn set(&mut self, highlights: &[Highlight]) {
        self.0.clear();
        for highlight in highlights {
            self.0.insert(
                EntityId(highlight.entity),
                Mark {
                    color: highlight.color,
                    through_walls: highlight.through_walls,
                },
            );
        }
    }

    /// The mark of `id`, if the entity is marked.
    pub(crate) fn get(&self, id: EntityId) -> Option<Mark> {
        self.0.get(&id).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn highlight(entity: i32, red: f32, through_walls: bool) -> Highlight {
        Highlight {
            entity,
            color: [red, 0.0, 0.0, 1.0],
            through_walls,
        }
    }

    #[test]
    fn the_later_mark_of_an_entity_wins_and_a_new_frame_starts_empty() {
        let mut marks = EntityMarks::default();
        marks.set(&[
            highlight(1, 0.5, false),
            highlight(2, 1.0, true),
            highlight(1, 0.25, true),
        ]);
        assert_eq!(
            marks.get(EntityId(1)),
            Some(Mark {
                color: [0.25, 0.0, 0.0, 1.0],
                through_walls: true
            })
        );
        assert!(marks.get(EntityId(3)).is_none());
        marks.set(&[]);
        assert!(marks.get(EntityId(1)).is_none() && marks.get(EntityId(2)).is_none());
    }
}

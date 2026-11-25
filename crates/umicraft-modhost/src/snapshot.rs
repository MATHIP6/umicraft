// SPDX-License-Identifier: MIT OR Apache-2.0

//! The render-thread entity snapshot (API 1.1, ADR-0094).
//!
//! World queries answer on the game thread only. A HUD widget or a `FRAME`
//! intercept runs on the render thread, so the client builds, once per frame, an
//! [`EntitySnapshot`] of the entities nearest to the camera and lends it to that
//! thread with [`with_entity_snapshot`]; the `world_entities_snapshot` host function
//! answers from it.
//!
//! The snapshot owns its rows and the text of their strings and is meant to be kept
//! and refilled every frame: once its buffers have grown to their working size,
//! refilling it does not allocate.

use core::ops::Range;

use crate::ffi;
use crate::world::{EntityTexts, EntityView};

/// The most entities a snapshot holds (ADR-0094).
pub const MAX_SNAPSHOT_ENTITIES: usize = 256;

/// A byte range of the snapshot's text.
type Span = Range<u32>;

/// One entity of the snapshot: the numbers of an [`EntityView`] (its strings
/// empty) and where its strings are in the shared text.
#[derive(Debug, Clone, PartialEq)]
struct Row {
    facts: EntityView<'static>,
    /// `kind`, `name`, `team`, `display_name`.
    texts: [Span; 4],
}

/// An owned, reusable list of at most [`MAX_SNAPSHOT_ENTITIES`] entities.
///
/// A new snapshot is *unavailable*: the host answers `UNAVAILABLE` for it, as
/// there is no world to read. [`begin`](Self::begin) makes it available and
/// empty, [`push`](Self::push) fills it, and [`clear`](Self::clear) makes it
/// unavailable again (the session ended).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EntitySnapshot {
    rows: Vec<Row>,
    /// The local player (API 1.4), kept apart: the list never holds it.
    local: Option<Row>,
    text: String,
    available: bool,
}

impl EntitySnapshot {
    /// An unavailable, empty snapshot.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Empties the snapshot and marks it available: there is a world, and it may
    /// have no entity in range. Keeps the allocated capacity.
    pub fn begin(&mut self) {
        self.rows.clear();
        self.local = None;
        self.text.clear();
        self.available = true;
    }

    /// Empties the snapshot and marks it unavailable (no world or session).
    pub fn clear(&mut self) {
        self.begin();
        self.available = false;
    }

    /// Whether the host answers from this snapshot rather than `UNAVAILABLE`.
    #[must_use]
    pub fn is_available(&self) -> bool {
        self.available
    }

    /// The number of entities held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether no entity is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Whether [`push`](Self::push) would refuse another entity.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.rows.len() >= MAX_SNAPSHOT_ENTITIES
    }

    /// Copies `view` (and its strings) into the snapshot.
    ///
    /// Returns `false`, copying nothing, when the snapshot already holds
    /// [`MAX_SNAPSHOT_ENTITIES`] entities or its text would outgrow what a span can
    /// address.
    pub fn push(&mut self, view: &EntityView<'_>) -> bool {
        if self.is_full() {
            return false;
        }
        let Some(row) = self.row(view) else {
            return false;
        };
        self.rows.push(row);
        true
    }

    /// Copies the local player's `view` into the snapshot (API 1.4), replacing the
    /// previous one. Returns `false`, copying nothing, when the text would outgrow
    /// what a span can address.
    pub fn set_local(&mut self, view: &EntityView<'_>) -> bool {
        self.local = self.row(view);
        self.local.is_some()
    }

    /// The local player, when the client gave one since [`begin`](Self::begin).
    #[must_use]
    pub fn local(&self) -> Option<EntityView<'_>> {
        self.local.as_ref().map(|row| self.view(row))
    }

    /// The row of `view`, its strings appended to the text; nothing is appended
    /// when one does not fit.
    fn row(&mut self, view: &EntityView<'_>) -> Option<Row> {
        let rollback = self.text.len();
        let texts = view.texts();
        let spans = (|| {
            Some([
                self.append(texts.kind)?,
                self.append(texts.name)?,
                self.append(texts.team)?,
                self.append(texts.display_name)?,
            ])
        })();
        let Some(texts) = spans else {
            self.text.truncate(rollback);
            return None;
        };
        Some(Row {
            facts: view.with_text(EntityTexts::default()),
            texts,
        })
    }

    /// Appends `text` and returns where it landed, `None` past `u32::MAX` bytes.
    fn append(&mut self, text: &str) -> Option<Span> {
        let start = u32::try_from(self.text.len()).ok()?;
        let end = u32::try_from(self.text.len().checked_add(text.len())?).ok()?;
        self.text.push_str(text);
        Some(start..end)
    }

    /// The entity at `index`, in the order it was pushed.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<EntityView<'_>> {
        self.rows.get(index).map(|row| self.view(row))
    }

    /// The entities, in the order they were pushed.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = EntityView<'_>> {
        self.rows.iter().map(|row| self.view(row))
    }

    fn view(&self, row: &Row) -> EntityView<'_> {
        let text = |span: &Span| {
            self.text
                .get(span.start as usize..span.end as usize)
                .unwrap_or_default()
        };
        let [kind, name, team, display_name] = &row.texts;
        row.facts.with_text(EntityTexts {
            kind: text(kind),
            name: text(name),
            team: text(team),
            display_name: text(display_name),
        })
    }
}

/// Lends `snapshot` to the current thread while `f` runs, so the mod callbacks made
/// inside `f` can read it with `world_entities_snapshot`.
///
/// The client calls it on the render thread around the code that runs mod callbacks
/// (the `FRAME` event and the widget draws). The previous scope, if any, is
/// restored when `f` returns or unwinds; a nested scope hides the outer one
/// meanwhile. Outside any scope the host function answers `UNSUPPORTED`.
///
/// ```
/// use umicraft_modhost::{with_entity_snapshot, EntitySnapshot};
///
/// let mut snapshot = EntitySnapshot::new();
/// snapshot.begin();
/// let seen = with_entity_snapshot(&snapshot, || 42);
/// assert_eq!(seen, 42);
/// ```
pub fn with_entity_snapshot<R>(snapshot: &EntitySnapshot, f: impl FnOnce() -> R) -> R {
    ffi::lend_snapshot(snapshot, f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Effect, HeldItem};

    fn view(id: i32, name: &'static str) -> EntityView<'static> {
        EntityView {
            id,
            kind: "zombie",
            position: [f64::from(id), 64.0, 0.0],
            yaw: 1.0,
            pitch: 2.0,
            name,
            health: 20.0,
            max_health: 20.0,
            armor: 3.0,
            team: "red",
            width: 0.6,
            height: 1.95,
            velocity: [0.1, 0.0, 0.0],
            flags: 33,
            is_player: false,
            render_position: [f64::from(id) + 0.5, 64.0, 0.0],
            hurt_time: 4,
            held_item: Some(HeldItem { id: 276, damage: 0 }),
            display_name: "\u{a7}c[R] Ann",
            team_color: Some(0xff5555),
            ping: Some(42),
            uuid_version: 4,
            effects: [Effect {
                id: 1,
                amplifier: 1,
                hide_particles: false,
                duration: 600,
            }]
            .into_iter()
            .collect(),
            ..EntityView::default()
        }
    }

    #[test]
    fn the_local_player_is_kept_apart_from_the_list() {
        let mut snapshot = EntitySnapshot::new();
        snapshot.begin();
        assert_eq!(snapshot.local(), None);
        assert!(snapshot.set_local(&view(7, "Me")));
        assert!(snapshot.push(&view(1, "Ann")));
        assert_eq!(snapshot.local(), Some(view(7, "Me")));
        assert_eq!(snapshot.len(), 1);
        snapshot.begin();
        assert_eq!(snapshot.local(), None, "begin forgets it");
    }

    #[test]
    fn a_new_snapshot_is_unavailable_and_begin_makes_it_available() {
        let mut snapshot = EntitySnapshot::new();
        assert!(!snapshot.is_available() && snapshot.is_empty());
        snapshot.begin();
        assert!(snapshot.is_available() && snapshot.is_empty());
        snapshot.clear();
        assert!(!snapshot.is_available());
    }

    #[test]
    fn a_pushed_view_reads_back_with_its_strings() {
        let mut snapshot = EntitySnapshot::new();
        snapshot.begin();
        assert!(snapshot.push(&view(1, "Ann")));
        assert!(snapshot.push(&view(2, "")));
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot.get(0), Some(view(1, "Ann")));
        assert_eq!(snapshot.get(1), Some(view(2, "")));
        assert_eq!(snapshot.get(2), None);
        let ids: Vec<i32> = snapshot.iter().map(|entity| entity.id).collect();
        assert_eq!(ids, [1, 2]);
    }

    #[test]
    fn the_snapshot_holds_at_most_the_bound() {
        let mut snapshot = EntitySnapshot::new();
        snapshot.begin();
        for id in 0..MAX_SNAPSHOT_ENTITIES as i32 {
            assert!(snapshot.push(&view(id, "x")));
        }
        assert!(snapshot.is_full());
        assert!(!snapshot.push(&view(999, "overflow")));
        assert_eq!(snapshot.len(), MAX_SNAPSHOT_ENTITIES);
        assert_eq!(snapshot.get(MAX_SNAPSHOT_ENTITIES - 1).unwrap().id, 255);
    }

    #[test]
    fn begin_forgets_the_rows_and_keeps_the_capacity() {
        let mut snapshot = EntitySnapshot::new();
        snapshot.begin();
        for id in 0..10 {
            snapshot.push(&view(id, "name"));
        }
        let (rows, text) = (snapshot.rows.capacity(), snapshot.text.capacity());
        snapshot.begin();
        assert!(snapshot.is_empty());
        for id in 0..10 {
            snapshot.push(&view(id, "name"));
        }
        assert_eq!(
            (snapshot.rows.capacity(), snapshot.text.capacity()),
            (rows, text)
        );
    }
}

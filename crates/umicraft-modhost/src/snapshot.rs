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
use crate::world::EntityView;

/// The most entities a snapshot holds (ADR-0094).
pub const MAX_SNAPSHOT_ENTITIES: usize = 256;

/// A byte range of the snapshot's text.
type Span = Range<u32>;

/// One entity of the snapshot: the numbers of an [`EntityView`] and where its
/// strings are in the shared text.
#[derive(Debug, Clone, PartialEq)]
struct Row {
    id: i32,
    kind: Span,
    position: [f64; 3],
    yaw: f32,
    pitch: f32,
    name: Span,
    health: f32,
    max_health: f32,
    armor: f32,
    team: Span,
    width: f32,
    height: f32,
    velocity: [f64; 3],
    flags: u32,
    is_player: bool,
    render_position: [f64; 3],
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
        let rollback = self.text.len();
        let Some([kind, name, team]) = self.append_all([view.kind, view.name, view.team]) else {
            self.text.truncate(rollback);
            return false;
        };
        self.rows.push(Row {
            id: view.id,
            kind,
            position: view.position,
            yaw: view.yaw,
            pitch: view.pitch,
            name,
            health: view.health,
            max_health: view.max_health,
            armor: view.armor,
            team,
            width: view.width,
            height: view.height,
            velocity: view.velocity,
            flags: view.flags,
            is_player: view.is_player,
            render_position: view.render_position,
        });
        true
    }

    /// Appends the three strings of an entity and returns where each landed.
    fn append_all(&mut self, texts: [&str; 3]) -> Option<[Span; 3]> {
        let [kind, name, team] = texts;
        Some([self.append(kind)?, self.append(name)?, self.append(team)?])
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
        EntityView {
            id: row.id,
            kind: text(&row.kind),
            position: row.position,
            yaw: row.yaw,
            pitch: row.pitch,
            name: text(&row.name),
            health: row.health,
            max_health: row.max_health,
            armor: row.armor,
            team: text(&row.team),
            width: row.width,
            height: row.height,
            velocity: row.velocity,
            flags: row.flags,
            is_player: row.is_player,
            render_position: row.render_position,
        }
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
        }
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

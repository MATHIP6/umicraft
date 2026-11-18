// SPDX-License-Identifier: MIT OR Apache-2.0

//! The fights of the session, as running totals for the combo counter mod.
//!
//! The server never says "your hit landed". A hit counts when the entity that
//! was just attacked is hurt right after the attack (`Entity Status 2`, or the
//! `Animation` 1 older servers send): the same signal the vanilla client uses
//! to flash the target red.

/// How many ticks after an attack the target's hurt still counts as the answer
/// to it. Servers answer in the same tick or the next, so ten ticks (half a
/// second) leave room for lag without crediting an unrelated hit.
const CONFIRM_TICKS: u64 = 10;

/// The hits dealt and the hurts taken so far.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::game) struct CombatLog {
    hits: u32,
    hurts: u32,
    /// The last attack not yet answered: the target and the tick it was sent.
    pending: Option<(i32, u64)>,
}

impl CombatLog {
    /// Notes an attack on `target` at `tick`.
    pub(in crate::game) fn attacked(&mut self, target: i32, tick: u64) {
        self.pending = Some((target, tick));
    }

    /// Notes that `entity` was hurt at `tick`; counts a hit when it is the
    /// target of a recent attack. Returns whether it did.
    pub(in crate::game) fn entity_hurt(&mut self, entity: i32, tick: u64) -> bool {
        let confirmed = self.pending.is_some_and(|(target, at)| {
            target == entity && tick.saturating_sub(at) <= CONFIRM_TICKS
        });
        if confirmed {
            self.pending = None;
            self.hits = self.hits.saturating_add(1);
        }
        confirmed
    }

    /// Notes that the local player was hurt. The server can report one hurt in
    /// several ways (status, animation, health drop), so a hurt may count more
    /// than once: the combo only needs the total to change.
    pub(in crate::game) fn local_hurt(&mut self) {
        self.hurts = self.hurts.saturating_add(1);
    }

    /// The `(hits, hurts)` totals.
    pub(in crate::game) fn totals(&self) -> (u32, u32) {
        (self.hits, self.hurts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hurt_target_right_after_an_attack_is_a_hit() {
        let mut log = CombatLog::default();
        log.attacked(7, 100);
        assert!(log.entity_hurt(7, 101));
        assert_eq!(log.totals(), (1, 0));
    }

    #[test]
    fn one_attack_counts_once_even_if_the_hurt_is_reported_twice() {
        let mut log = CombatLog::default();
        log.attacked(7, 100);
        assert!(log.entity_hurt(7, 100));
        assert!(!log.entity_hurt(7, 100), "status and animation of one hit");
        assert_eq!(log.totals(), (1, 0));
    }

    #[test]
    fn another_entity_or_a_late_hurt_is_not_the_answer() {
        let mut log = CombatLog::default();
        log.attacked(7, 100);
        assert!(!log.entity_hurt(8, 101));
        assert!(!log.entity_hurt(7, 100 + CONFIRM_TICKS + 1));
        assert_eq!(log.totals(), (0, 0));
    }

    #[test]
    fn a_hurt_with_no_attack_is_not_a_hit() {
        let mut log = CombatLog::default();
        assert!(!log.entity_hurt(7, 5));
        assert_eq!(log.totals(), (0, 0));
    }

    #[test]
    fn the_hurts_of_the_player_are_counted() {
        let mut log = CombatLog::default();
        log.local_hurt();
        log.local_hurt();
        assert_eq!(log.totals(), (0, 2));
    }
}

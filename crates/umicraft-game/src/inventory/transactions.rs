// SPDX-License-Identifier: MIT OR Apache-2.0

//! Predicted clicks waiting for the server's `Confirm Transaction`.
//!
//! Every click is applied locally at once. The state from before the click is
//! kept until the server answers; a rejection puts it back instead of
//! leaving the player looking at items the server never moved.

use std::collections::VecDeque;

use umicraft_protocol::packets::play::ItemStack;

use super::player::PlayerInventory;

/// Most predicted clicks kept at once; older ones are assumed accepted.
const MAX_PENDING: usize = 64;

/// The window state a click can change.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// The player inventory.
    pub inventory: PlayerInventory,
    /// The open container's own slots (empty for the player window).
    pub own: Vec<Option<ItemStack>>,
    /// The cursor stack.
    pub cursor: Option<ItemStack>,
}

#[derive(Debug, Clone)]
struct Pending {
    window_id: u8,
    action_number: i16,
    before: Snapshot,
}

/// What a `Confirm Transaction` did to the queue.
#[derive(Debug, Clone, PartialEq)]
pub enum Confirmation {
    /// The click was accepted; its snapshot was dropped.
    Accepted,
    /// The click was rejected: restore this state. Later predictions were
    /// built on it and are dropped.
    Rejected(Box<Snapshot>),
    /// No pending click matches (already confirmed, or from before a resync).
    Unknown,
}

/// The queue of unconfirmed predicted clicks.
#[derive(Debug, Clone, Default)]
pub struct Transactions {
    pending: VecDeque<Pending>,
}

impl Transactions {
    /// Records a predicted click with the state from before it.
    pub fn record(&mut self, window_id: u8, action_number: i16, before: Snapshot) {
        if self.pending.len() == MAX_PENDING {
            self.pending.pop_front();
        }
        self.pending.push_back(Pending {
            window_id,
            action_number,
            before,
        });
    }

    /// Applies the server's answer to the queue.
    pub fn confirm(&mut self, window_id: u8, action_number: i16, accepted: bool) -> Confirmation {
        let Some(position) = self
            .pending
            .iter()
            .position(|p| p.window_id == window_id && p.action_number == action_number)
        else {
            return Confirmation::Unknown;
        };
        if accepted {
            self.pending.drain(..=position);
            return Confirmation::Accepted;
        }
        let rejected = self.pending.drain(position..).next();
        self.pending.clear();
        rejected.map_or(Confirmation::Unknown, |p| {
            Confirmation::Rejected(Box::new(p.before))
        })
    }

    /// Forgets every pending click (the server resent the window).
    pub fn clear(&mut self) {
        self.pending.clear();
    }

    /// Number of unconfirmed clicks.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Whether no click is waiting.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(cursor: i16) -> Snapshot {
        Snapshot {
            inventory: PlayerInventory::new(),
            own: Vec::new(),
            cursor: Some(ItemStack {
                item_id: cursor,
                stack_size: 1,
                damage: 0,
                nbt: None,
            }),
        }
    }

    #[test]
    fn an_accepted_click_drops_itself_and_the_older_ones() {
        let mut t = Transactions::default();
        t.record(0, 1, snapshot(1));
        t.record(0, 2, snapshot(2));
        t.record(0, 3, snapshot(3));
        assert_eq!(t.confirm(0, 2, true), Confirmation::Accepted);
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn a_rejected_click_returns_its_snapshot_and_empties_the_queue() {
        let mut t = Transactions::default();
        t.record(0, 1, snapshot(1));
        t.record(0, 2, snapshot(2));
        assert_eq!(
            t.confirm(0, 1, false),
            Confirmation::Rejected(Box::new(snapshot(1)))
        );
        assert!(t.is_empty());
    }

    #[test]
    fn an_unknown_transaction_is_reported() {
        let mut t = Transactions::default();
        t.record(1, 1, snapshot(1));
        assert_eq!(t.confirm(0, 1, true), Confirmation::Unknown);
        assert_eq!(t.confirm(1, 9, false), Confirmation::Unknown);
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn the_queue_is_bounded() {
        let mut t = Transactions::default();
        for n in 0..100 {
            t.record(0, n, snapshot(1));
        }
        assert_eq!(t.len(), MAX_PENDING);
    }
}

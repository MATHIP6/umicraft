// SPDX-License-Identifier: MIT OR Apache-2.0

//! The name of the held item that fades in above the hotbar when the
//! selection changes (MCP `GuiIngame.func_181551_a` / `renderSelectedItem`
//! and `GuiIngame.updateTick`).

/// Ticks the name stays after the held item changes.
const HIGHLIGHT_TICKS: u32 = 40;
/// Vertical offset of the name when the status rows are hidden (creative).
const NO_STATUS_ROWS_SHIFT: i32 = 14;

/// What makes two held stacks "the same" for the highlight
/// (`ItemStack.areItemStackTagsEqual` and the damage rule of `updateTick`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldIdentity {
    /// The item id.
    pub item_id: i16,
    /// The damage value (the metadata of items that are not damageable).
    pub damage: i16,
    /// Whether the item takes damage; a damageable item ignores `damage`.
    pub damageable: bool,
    /// The raw NBT of the stack.
    pub nbt: Option<Vec<u8>>,
}

impl HeldIdentity {
    fn same_as(&self, other: &Self) -> bool {
        self.item_id == other.item_id
            && self.nbt == other.nbt
            && (self.damageable || self.damage == other.damage)
    }
}

/// The highlight countdown (`remainingHighlightTicks`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeldNameTracker {
    remaining: u32,
    highlighted: Option<HeldIdentity>,
}

/// The name drawn above the hotbar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldNameView {
    /// The display name.
    pub text: String,
    /// Whether the name was set by the player (an anvil name is italic).
    pub italic: bool,
    /// The opacity, `1..=255`.
    pub alpha: u8,
    /// Pixels added to the base height `gui height - 59`.
    pub dy: i32,
}

impl HeldNameTracker {
    /// A tracker with nothing highlighted.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            remaining: 0,
            highlighted: None,
        }
    }

    /// One tick with the currently held stack (`GuiIngame.updateTick`): an
    /// empty hand clears the highlight, a different stack restarts it, the
    /// same stack counts it down.
    pub fn tick(&mut self, held: Option<HeldIdentity>) {
        match &held {
            None => self.remaining = 0,
            Some(now)
                if self
                    .highlighted
                    .as_ref()
                    .is_some_and(|old| now.same_as(old)) =>
            {
                self.remaining = self.remaining.saturating_sub(1);
            }
            Some(_) => self.remaining = HIGHLIGHT_TICKS,
        }
        self.highlighted = held;
    }

    /// Ticks left.
    #[must_use]
    pub fn remaining(&self) -> u32 {
        self.remaining
    }

    /// The name to draw, if the highlight is running.
    ///
    /// `alpha = (int)(remaining * 256 / 10)` capped at 255, so the name is
    /// opaque for the first 30 ticks and fades over the last 10 (the smallest
    /// alpha, 25, is above the 4 below which `FontRenderer.drawString` would
    /// draw the text opaque). `draws_status_rows` is `shouldDrawHUD` (survival
    /// and adventure): without the rows the name sits 14 pixels lower.
    #[must_use]
    pub fn view(
        &self,
        name: impl FnOnce() -> (String, bool),
        draws_status_rows: bool,
    ) -> Option<HeldNameView> {
        if self.remaining == 0 || self.highlighted.is_none() {
            return None;
        }
        let alpha = (self.remaining * 256 / 10).min(255);
        let (text, italic) = name();
        Some(HeldNameView {
            text,
            italic,
            alpha: alpha as u8,
            dy: if draws_status_rows {
                0
            } else {
                NO_STATUS_ROWS_SHIFT
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(item_id: i16, damage: i16, damageable: bool) -> Option<HeldIdentity> {
        Some(HeldIdentity {
            item_id,
            damage,
            damageable,
            nbt: None,
        })
    }

    fn name() -> (String, bool) {
        ("Stone".to_owned(), false)
    }

    #[test]
    fn a_new_stack_shows_the_name_for_forty_ticks() {
        let mut tracker = HeldNameTracker::new();
        tracker.tick(stack(1, 0, false));
        assert_eq!(tracker.remaining(), 40);
        for _ in 0..40 {
            tracker.tick(stack(1, 0, false));
        }
        assert_eq!(tracker.remaining(), 0);
        assert!(tracker.view(name, true).is_none());
    }

    #[test]
    fn the_name_is_opaque_then_fades_over_ten_ticks() {
        let mut tracker = HeldNameTracker::new();
        tracker.tick(stack(1, 0, false));
        let alpha = |tracker: &HeldNameTracker| tracker.view(name, true).map(|v| v.alpha);
        assert_eq!(alpha(&tracker), Some(255));
        for _ in 0..30 {
            tracker.tick(stack(1, 0, false));
        }
        // 10 ticks left: 10 * 256 / 10 = 256 -> 255; 9 -> 230; 5 -> 128.
        assert_eq!(alpha(&tracker), Some(255));
        tracker.tick(stack(1, 0, false));
        assert_eq!(alpha(&tracker), Some(230));
        for _ in 0..4 {
            tracker.tick(stack(1, 0, false));
        }
        assert_eq!(tracker.remaining(), 5);
        assert_eq!(alpha(&tracker), Some(128));
    }

    #[test]
    fn the_damage_of_a_damageable_item_does_not_restart_it() {
        let mut tracker = HeldNameTracker::new();
        tracker.tick(stack(276, 0, true));
        tracker.tick(stack(276, 0, true));
        tracker.tick(stack(276, 12, true));
        assert_eq!(tracker.remaining(), 38);
        // A block with another metadata is another item.
        tracker.tick(stack(35, 1, false));
        assert_eq!(tracker.remaining(), 40);
        tracker.tick(stack(35, 2, false));
        assert_eq!(tracker.remaining(), 40);
    }

    #[test]
    fn an_empty_hand_clears_the_name() {
        let mut tracker = HeldNameTracker::new();
        tracker.tick(stack(1, 0, false));
        tracker.tick(None);
        assert_eq!(tracker.remaining(), 0);
    }

    #[test]
    fn creative_draws_the_name_fourteen_pixels_lower() {
        let mut tracker = HeldNameTracker::new();
        tracker.tick(stack(1, 0, false));
        assert_eq!(tracker.view(name, true).map(|v| v.dy), Some(0));
        assert_eq!(tracker.view(name, false).map(|v| v.dy), Some(14));
    }
}

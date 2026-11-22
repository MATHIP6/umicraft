// SPDX-License-Identifier: MIT OR Apache-2.0

//! The book of the enchanting table screen (MCP `GuiEnchantment`, the fields
//! `field_147071_v` to `field_147082_x` stepped by `func_147068_g` in
//! `updateScreen`, once per game tick).
//!
//! The book opens while the table offers an enchantment and flips its pages
//! towards a new random target whenever the item in the first slot changes.
//! This is the screen's own book, not the one above the table in the world.

use umicraft_core::java_random::JavaRandom;
use umicraft_protocol::packets::play::ItemStack;

/// How fast the book opens or shuts per tick.
const OPEN_STEP: f32 = 0.2;
/// The page speed limit and the share of the gap taken per tick.
const MAX_FLIP_SPEED: f32 = 0.2;
const FLIP_PULL: f32 = 0.4;
/// How quickly the page speed follows the pull.
const FLIP_DAMPING: f32 = 0.9;

/// What one tick of the screen book needs to know.
#[derive(Debug, Clone, Copy)]
pub struct BookTick<'a> {
    /// The stack in the item slot.
    pub item: Option<&'a ItemStack>,
    /// Whether any row has a level cost (`enchantLevels[i] != 0`).
    pub offers: bool,
}

/// The pose of the screen book between two ticks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenBookPose {
    /// How open the book is, `0..=1` (`f2`).
    pub spread: f32,
    /// How far the right page is flipped, `0..=1` (`f3`).
    pub flip_right: f32,
    /// How far the left page is flipped, `0..=1` (`f4`).
    pub flip_left: f32,
}

/// The animation state of the book.
#[derive(Debug, Clone)]
pub struct ScreenBook {
    page: f32,
    page_prev: f32,
    open: f32,
    open_prev: f32,
    target: f32,
    speed: f32,
    item: Option<ItemStack>,
    random: JavaRandom,
}

impl ScreenBook {
    /// A shut book on page 0, as the screen opens. `seed` feeds the page
    /// targets (the reference uses an unseeded `new Random()`).
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            page: 0.0,
            page_prev: 0.0,
            open: 0.0,
            open_prev: 0.0,
            target: 0.0,
            speed: 0.0,
            item: None,
            random: JavaRandom::new(seed),
        }
    }

    /// One game tick (`func_147068_g`).
    pub fn tick(&mut self, tick: BookTick<'_>) {
        if tick.item != self.item.as_ref() {
            self.item = tick.item.cloned();
            loop {
                self.target += self.random.next_int(4) as f32 - self.random.next_int(4) as f32;
                if self.page > self.target + 1.0 || self.page < self.target - 1.0 {
                    break;
                }
            }
        }
        self.page_prev = self.page;
        self.open_prev = self.open;
        let step = if tick.offers { OPEN_STEP } else { -OPEN_STEP };
        self.open = (self.open + step).clamp(0.0, 1.0);
        let pull = ((self.target - self.page) * FLIP_PULL).clamp(-MAX_FLIP_SPEED, MAX_FLIP_SPEED);
        self.speed += (pull - self.speed) * FLIP_DAMPING;
        self.page += self.speed;
    }

    /// The pose `partial` of the way from the previous tick to this one
    /// (`drawGuiContainerBackgroundLayer`).
    #[must_use]
    pub fn pose(&self, partial: f32) -> ScreenBookPose {
        let page = self.page_prev + (self.page - self.page_prev) * partial;
        // `(f - (float) truncateDoubleToInt(f)) * 1.6F - 0.3F`, clamped.
        let flipped = |offset: f32| {
            let shifted = page + offset;
            ((shifted - shifted.trunc()) * 1.6 - 0.3).clamp(0.0, 1.0)
        };
        ScreenBookPose {
            spread: self.open_prev + (self.open - self.open_prev) * partial,
            flip_right: flipped(0.25),
            flip_left: flipped(0.75),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sword() -> ItemStack {
        ItemStack {
            item_id: 276,
            stack_size: 1,
            damage: 0,
            nbt: None,
        }
    }

    #[test]
    fn the_book_opens_in_five_ticks_and_shuts_again() {
        let mut book = ScreenBook::new(1);
        let item = sword();
        for _ in 0..5 {
            book.tick(BookTick {
                item: Some(&item),
                offers: true,
            });
        }
        assert!((book.pose(1.0).spread - 1.0).abs() < 1e-6);
        book.tick(BookTick {
            item: Some(&item),
            offers: false,
        });
        assert!((book.pose(1.0).spread - 0.8).abs() < 1e-6);
        assert!((book.pose(0.5).spread - 0.9).abs() < 1e-6);
    }

    #[test]
    fn a_new_item_sends_the_pages_at_least_one_page_away() {
        let mut book = ScreenBook::new(7);
        let item = sword();
        book.tick(BookTick {
            item: Some(&item),
            offers: true,
        });
        assert!((book.target - book.page_prev).abs() > 1.0);
        // The pages move towards the target, never faster than 0.2 a tick.
        assert!(book.page != 0.0 && book.speed.abs() <= MAX_FLIP_SPEED);
    }

    #[test]
    fn the_same_item_does_not_turn_pages() {
        let mut book = ScreenBook::new(3);
        for _ in 0..40 {
            book.tick(BookTick {
                item: None,
                offers: false,
            });
        }
        assert_eq!(
            book.pose(1.0).flip_right,
            ScreenBook::new(3).pose(1.0).flip_right
        );
        assert_eq!(book.pose(1.0).spread, 0.0);
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The tick state of an enchanting table book (MCP
//! `TileEntityEnchantmentTable.update`).
//!
//! Each tick the book opens by `0.1` while a player is within 3 blocks and
//! closes while none is, turns toward that player by 0.4 of the remaining
//! angle each tick, and flips pages: a random target is drawn and the flip
//! eases toward it. The renderer interpolates between the last two ticks.
//!
//! Deviations (accepted, like the hover phases of ADR-0028):
//!
//! - the only player the client knows at the render site is the viewer, so
//!   the nearest player is the viewer and other players are ignored;
//! - the reference draws from one shared `Random`; here each book has its
//!   own generator seeded from its position, so books flip independently and
//!   the same way every run.

use std::f32::consts::PI;

use glam::Vec3;
use umicraft_core::java_random::JavaRandom;

/// Eye height above the feet of the viewer: the camera is the eye, while
/// `getClosestPlayer` measures from the player's position (the feet).
const EYE_HEIGHT: f32 = 1.62;
/// Squared range in which a player opens the book (`getClosestPlayer(..., 3.0)`).
const RANGE_SQUARED: f32 = 9.0;

/// The animated fields of one table.
#[derive(Debug, Clone)]
pub(super) struct BookState {
    pub tick_count: u32,
    pub page_flip: f32,
    pub page_flip_prev: f32,
    pub spread: f32,
    pub spread_prev: f32,
    pub rotation: f32,
    pub rotation_prev: f32,
    flip_target: f32,
    flip_speed: f32,
    look_angle: f32,
    random: JavaRandom,
}

impl BookState {
    /// A shut book at rest with its own generator.
    pub(super) fn new(seed: u64) -> Self {
        Self {
            tick_count: 0,
            page_flip: 0.0,
            page_flip_prev: 0.0,
            spread: 0.0,
            spread_prev: 0.0,
            rotation: 0.0,
            rotation_prev: 0.0,
            flip_target: 0.0,
            flip_speed: 0.0,
            look_angle: 0.0,
            random: JavaRandom::new(seed),
        }
    }

    /// One game tick. `viewer` is the eye of the viewer and `table` the
    /// centre of the table block.
    pub(super) fn update(&mut self, table: Vec3, viewer: Vec3) {
        self.spread_prev = self.spread;
        self.rotation_prev = self.rotation;
        let feet = Vec3::new(viewer.x, viewer.y - EYE_HEIGHT, viewer.z);
        if feet.distance_squared(table) < RANGE_SQUARED {
            self.look_angle = (viewer.z - table.z).atan2(viewer.x - table.x);
            self.spread += 0.1;
            if self.spread < 0.5 || self.random.next_int(40) == 0 {
                self.draw_flip_target();
            }
        } else {
            self.look_angle += 0.02;
            self.spread -= 0.1;
        }
        self.rotation = wrap(self.rotation);
        self.look_angle = wrap(self.look_angle);
        self.rotation += wrap(self.look_angle - self.rotation) * 0.4;
        self.spread = self.spread.clamp(0.0, 1.0);
        self.tick_count += 1;
        self.page_flip_prev = self.page_flip;
        let wanted = ((self.flip_target - self.page_flip) * 0.4).clamp(-0.2, 0.2);
        self.flip_speed += (wanted - self.flip_speed) * 0.9;
        self.page_flip += self.flip_speed;
    }

    /// Moves the flip target by a random non-zero step.
    fn draw_flip_target(&mut self) {
        loop {
            let step = self.random.next_int(4) as i32 - self.random.next_int(4) as i32;
            if step != 0 {
                self.flip_target += step as f32;
                return;
            }
        }
    }
}

/// `angle` brought into `[-PI, PI)`, as the reference's loops do.
pub(super) fn wrap(angle: f32) -> f32 {
    let mut angle = angle;
    while angle >= PI {
        angle -= 2.0 * PI;
    }
    while angle < -PI {
        angle += 2.0 * PI;
    }
    angle
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: Vec3 = Vec3::new(0.5, 64.5, 0.5);

    fn near() -> Vec3 {
        // Two blocks east of the table, eyes at head height.
        Vec3::new(2.5, 64.0 + EYE_HEIGHT, 0.5)
    }

    #[test]
    fn a_near_player_opens_the_book_in_ten_ticks_and_a_far_one_shuts_it() {
        let mut book = BookState::new(1);
        for _ in 0..10 {
            book.update(TABLE, near());
        }
        assert!((book.spread - 1.0).abs() < 1e-5);
        for _ in 0..10 {
            book.update(TABLE, Vec3::new(50.0, 64.0, 50.0));
        }
        assert_eq!(book.spread, 0.0);
    }

    #[test]
    fn the_book_turns_toward_the_player_and_settles() {
        let mut book = BookState::new(1);
        for _ in 0..60 {
            book.update(TABLE, Vec3::new(0.5, 64.0 + EYE_HEIGHT, 2.5));
        }
        // The player is at +z: atan2(2, 0) is a quarter turn.
        assert!((book.rotation - PI / 2.0).abs() < 1e-3, "{}", book.rotation);
    }

    #[test]
    fn the_range_is_three_blocks_from_the_feet() {
        let mut book = BookState::new(1);
        let eye = |dx: f32| Vec3::new(0.5 + dx, 64.5 - 0.5 + EYE_HEIGHT, 0.5);
        book.update(TABLE, eye(2.9));
        assert!(book.spread > 0.0);
        let mut far = BookState::new(1);
        far.update(TABLE, eye(3.1));
        assert_eq!(far.spread, 0.0);
    }

    #[test]
    fn the_page_flip_stays_finite_and_moves_while_a_player_is_near() {
        let mut book = BookState::new(7);
        let mut moved = false;
        for _ in 0..400 {
            book.update(TABLE, near());
            assert!(book.page_flip.is_finite());
            moved |= book.page_flip != book.page_flip_prev;
        }
        assert!(moved);
    }

    #[test]
    fn wrap_keeps_angles_in_a_half_open_turn() {
        assert_eq!(wrap(PI), -PI);
        assert_eq!(wrap(-PI), -PI);
        assert!((wrap(3.5 * PI) + 0.5 * PI).abs() < 1e-5);
    }
}

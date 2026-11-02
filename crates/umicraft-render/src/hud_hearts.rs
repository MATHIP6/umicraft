// SPDX-License-Identifier: MIT OR Apache-2.0

//! The row of hearts above the hotbar (MCP `GuiIngame.renderPlayerStats`).
//!
//! The game thread says what the player has (health, absorption, effects, the
//! flash of a loss); [`layout`] turns that into the sprites to draw. It holds
//! no state and draws nothing, so the arithmetic is tested alone.

use umicraft_core::java_random::JavaRandom;

/// The potions that change the hearts (MCP `Potion`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeartsTint {
    /// No effect: red hearts.
    #[default]
    Normal,
    /// Poison: green hearts (`Potion.poison`, wins over wither).
    Poison,
    /// Wither: black hearts (`Potion.wither`).
    Wither,
}

/// What the hearts show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeartsView {
    /// The maximum health (`generic.maxHealth`), 20 by default.
    pub max_health: f32,
    /// The absorption hearts, in health points.
    pub absorption: f32,
    /// The health before the last change, in half hearts: the white outline.
    pub previous: i32,
    /// Whether the white flash of a loss or a gain is on.
    pub flash: bool,
    /// Poison or wither.
    pub tint: HeartsTint,
    /// Whether regeneration is active: one heart rises by two pixels.
    pub regeneration: bool,
    /// Whether the world is hardcore: the hearts have the hardcore frame.
    pub hardcore: bool,
    /// Ticks of the HUD: seeds the shake and moves the regeneration wave.
    pub tick: u32,
}

impl Default for HeartsView {
    fn default() -> Self {
        Self {
            max_health: 20.0,
            absorption: 0.0,
            previous: 0,
            flash: false,
            tint: HeartsTint::Normal,
            regeneration: false,
            hardcore: false,
            tick: 0,
        }
    }
}

/// One sprite of the row, in GUI pixels. The size is always 9 by 9.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeartSprite {
    /// Left edge, in GUI pixels from the left of the screen.
    pub x: i32,
    /// Top edge, in GUI pixels from the top of the screen.
    pub y: i32,
    /// Texture column in `icons.png`.
    pub u: u32,
    /// Texture row in `icons.png`.
    pub v: u32,
}

/// The sprites of the hearts and the top of the armour row.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HeartsLayout {
    /// The sprites in drawing order.
    pub sprites: Vec<HeartSprite>,
    /// The top edge of the armour row, above the topmost row of hearts.
    pub armor_y: i32,
}

/// Lays out the hearts of a player with `health` points on a screen
/// `gui_width` by `gui_height` GUI pixels.
#[must_use]
pub fn layout(view: &HeartsView, health: f32, gui_width: f32, gui_height: f32) -> HeartsLayout {
    let health_ceil = health.ceil() as i32;
    let left = (gui_width / 2.0) as i32 - 91;
    let base_y = gui_height as i32 - 39;
    let (max, absorption) = (view.max_health, view.absorption);
    let rows = ((max + absorption) / 2.0 / 10.0).ceil() as i32;
    let step = (10 - (rows - 2)).max(3);
    let armor_y = base_y - (rows - 1) * step - 10;
    // `rand.setSeed(updateCounter * 312871)` with an int product.
    let seed = (view.tick as i32).wrapping_mul(312_871);
    let mut rand = JavaRandom::new(i64::from(seed) as u64);
    let regeneration = view
        .regeneration
        .then(|| (view.tick % ((max + 5.0).ceil() as u32)) as i32);
    let tint = match view.tint {
        HeartsTint::Normal => 16,
        HeartsTint::Poison => 16 + 36,
        HeartsTint::Wither => 16 + 72,
    };
    let frame_v = if view.hardcore { 45 } else { 0 };
    let flash = u32::from(view.flash);
    let mut left_absorption = absorption;
    let mut sprites = Vec::new();
    for index in (0..((max + absorption) / 2.0).ceil() as i32).rev() {
        let row = ((index + 1) as f32 / 10.0).ceil() as i32 - 1;
        let x = left + index % 10 * 8;
        let mut y = base_y - row * step;
        if health_ceil <= 4 {
            y += rand.next_int(2) as i32;
        }
        if regeneration == Some(index) {
            y -= 2;
        }
        let at = |u: u32| HeartSprite {
            x,
            y,
            u,
            v: frame_v,
        };
        sprites.push(at(16 + flash * 9));
        if view.flash {
            if index * 2 + 1 < view.previous {
                sprites.push(at(tint + 54));
            }
            if index * 2 + 1 == view.previous {
                sprites.push(at(tint + 63));
            }
        }
        if left_absorption > 0.0 {
            let half = left_absorption == absorption && absorption % 2.0 == 1.0;
            sprites.push(at(tint + if half { 153 } else { 144 }));
            left_absorption -= 2.0;
        } else {
            if index * 2 + 1 < health_ceil {
                sprites.push(at(tint + 36));
            }
            if index * 2 + 1 == health_ceil {
                sprites.push(at(tint + 45));
            }
        }
    }
    HeartsLayout { sprites, armor_y }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: f32 = 320.0;
    const HEIGHT: f32 = 240.0;

    fn at(layout: &HeartsLayout, x: i32) -> Vec<(u32, u32)> {
        layout
            .sprites
            .iter()
            .filter(|sprite| sprite.x == x)
            .map(|sprite| (sprite.u, sprite.v))
            .collect()
    }

    #[test]
    fn full_health_is_ten_red_hearts_in_a_row_above_the_armour() {
        let layout = layout(&HeartsView::default(), 20.0, WIDTH, HEIGHT);
        // A container and a full heart for each of the ten.
        assert_eq!(layout.sprites.len(), 20);
        assert!(layout.sprites.iter().all(|sprite| sprite.y == 201));
        assert_eq!(layout.armor_y, 191);
        // Drawn from the last heart to the first, 8 pixels apart.
        assert_eq!(layout.sprites[0].x, 69 + 72);
        assert_eq!(at(&layout, 69), vec![(16, 0), (52, 0)]);
    }

    #[test]
    fn an_odd_health_ends_on_a_half_heart() {
        let layout = layout(&HeartsView::default(), 5.0, WIDTH, HEIGHT);
        assert_eq!(at(&layout, 69), vec![(16, 0), (52, 0)]);
        assert_eq!(at(&layout, 85), vec![(16, 0), (61, 0)]);
        assert_eq!(at(&layout, 93), vec![(16, 0)]);
    }

    #[test]
    fn poison_and_wither_change_the_hearts_and_poison_wins() {
        let poisoned = HeartsView {
            tint: HeartsTint::Poison,
            ..HeartsView::default()
        };
        assert_eq!(at(&layout(&poisoned, 2.0, WIDTH, HEIGHT), 69)[1], (88, 0));
        let withered = HeartsView {
            tint: HeartsTint::Wither,
            ..HeartsView::default()
        };
        assert_eq!(at(&layout(&withered, 2.0, WIDTH, HEIGHT), 69)[1], (124, 0));
    }

    #[test]
    fn the_flash_swaps_the_container_and_outlines_the_lost_health() {
        let view = HeartsView {
            flash: true,
            previous: 7,
            ..HeartsView::default()
        };
        let layout = layout(&view, 4.0, WIDTH, HEIGHT);
        // The third heart is fully lost: white container, white outline.
        assert_eq!(at(&layout, 85), vec![(25, 0), (70, 0)]);
        // The fourth was a half heart before, now empty.
        assert_eq!(at(&layout, 93), vec![(25, 0), (79, 0)]);
    }

    #[test]
    fn absorption_adds_a_row_and_lifts_the_armour() {
        let view = HeartsView {
            absorption: 4.0,
            ..HeartsView::default()
        };
        let layout = layout(&view, 20.0, WIDTH, HEIGHT);
        // Twelve hearts in two rows of ten and two, 10 pixels apart.
        assert_eq!(layout.sprites.len(), 24);
        assert_eq!(layout.armor_y, 201 - 10 - 10);
        let upper: Vec<_> = layout.sprites.iter().filter(|s| s.y == 191).collect();
        assert_eq!(upper.len(), 4);
        // The golden heart sits on the first two of the upper row.
        assert_eq!(upper[1].u, 160);
    }

    #[test]
    fn an_odd_absorption_ends_on_a_half_golden_heart() {
        let view = HeartsView {
            absorption: 1.0,
            ..HeartsView::default()
        };
        let layout = layout(&view, 20.0, WIDTH, HEIGHT);
        assert!(layout.sprites.iter().any(|sprite| sprite.u == 169));
    }

    #[test]
    fn regeneration_lifts_one_heart_that_walks_along_the_row() {
        let lifted = |tick| {
            let view = HeartsView {
                regeneration: true,
                tick,
                ..HeartsView::default()
            };
            let layout = layout(&view, 20.0, WIDTH, HEIGHT);
            layout.sprites.iter().find(|s| s.y == 199).map(|s| s.x)
        };
        assert_eq!(lifted(0), Some(69));
        assert_eq!(lifted(3), Some(69 + 24));
        // `ceil(20 + 5)` positions: the wave comes round.
        assert_eq!(lifted(25), Some(69));
        // Past the row of ten, no heart is lifted.
        assert_eq!(lifted(12), None);
    }

    #[test]
    fn hearts_shake_only_when_two_hearts_are_left() {
        let still = layout(&HeartsView::default(), 5.0, WIDTH, HEIGHT);
        assert!(still.sprites.iter().all(|sprite| sprite.y == 201));
        let shaking = layout(&HeartsView::default(), 4.0, WIDTH, HEIGHT);
        assert!(shaking.sprites.iter().any(|sprite| sprite.y == 202));
        // The shake follows the tick: another tick, another pattern.
        let other = layout(
            &HeartsView {
                tick: 1,
                ..HeartsView::default()
            },
            4.0,
            WIDTH,
            HEIGHT,
        );
        assert_ne!(shaking, other);
    }

    #[test]
    fn hardcore_uses_the_frame_row_of_the_texture() {
        let view = HeartsView {
            hardcore: true,
            ..HeartsView::default()
        };
        let layout = layout(&view, 20.0, WIDTH, HEIGHT);
        assert!(layout.sprites.iter().all(|sprite| sprite.v == 45));
    }
}

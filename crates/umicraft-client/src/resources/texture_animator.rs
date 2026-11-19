// SPDX-License-Identifier: MIT OR Apache-2.0

//! Drives the animated block textures (water, lava, fire, portal, ...).
//!
//! The reference client calls `TextureAtlasSprite.updateAnimation` once per
//! game tick (20 Hz), whatever the frame rate. The window feeds the elapsed
//! frame time to [`TextureAnimator::advance`], which steps every animation as
//! many whole ticks as have passed and hands each changed layer to a writer
//! (the renderer), so a frame that crosses no tick uploads nothing.

use std::time::Duration;

use umicraft_assets::TextureAnimation;

/// Length of a game tick.
const TICK: Duration = Duration::from_millis(50);

/// After a stall (window drag, breakpoint) the animations resume instead of
/// replaying every missed tick: at most this many ticks run per call.
const MAX_CATCH_UP_TICKS: u32 = 4;

/// The animated layers of the bound block array and their tick clock.
#[derive(Debug, Default)]
pub struct TextureAnimator {
    animations: Vec<TextureAnimation>,
    /// Frame time not yet turned into a tick.
    pending: Duration,
}

impl TextureAnimator {
    /// An animator for `animations`, each at its first step.
    #[must_use]
    pub fn new(animations: Vec<TextureAnimation>) -> Self {
        Self {
            animations,
            pending: Duration::ZERO,
        }
    }

    /// Number of animated layers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.animations.len()
    }

    /// Whether no layer is animated.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.animations.is_empty()
    }

    /// Runs the ticks that `elapsed` completes and calls `write(layer, pixels)`
    /// for every layer whose picture changed (once per tick at most).
    pub fn advance(&mut self, elapsed: Duration, mut write: impl FnMut(u32, &[u8])) {
        if self.animations.is_empty() {
            return;
        }
        self.pending += elapsed;
        let mut ticks = 0;
        while self.pending >= TICK {
            self.pending -= TICK;
            ticks += 1;
        }
        if ticks > MAX_CATCH_UP_TICKS {
            ticks = MAX_CATCH_UP_TICKS;
        }
        for _ in 0..ticks {
            for animation in &mut self.animations {
                let layer = animation.layer();
                if let Some(pixels) = animation.tick() {
                    write(layer, pixels);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two 1×1 frames that swap every tick (`frametime` 1).
    fn blinking(layer: u32) -> TextureAnimation {
        let frames = vec![vec![1, 1, 1, 255], vec![2, 2, 2, 255]];
        TextureAnimation::from_mcmeta(layer, frames, br#"{"animation": {}}"#)
            .expect("two frames animate")
    }

    fn run(animator: &mut TextureAnimator, elapsed: Duration) -> Vec<(u32, Vec<u8>)> {
        let mut writes = Vec::new();
        animator.advance(elapsed, |layer, pixels| {
            writes.push((layer, pixels.to_vec()))
        });
        writes
    }

    #[test]
    fn a_frame_shorter_than_a_tick_uploads_nothing() {
        let mut animator = TextureAnimator::new(vec![blinking(3)]);
        assert!(run(&mut animator, Duration::from_millis(16)).is_empty());
        assert!(run(&mut animator, Duration::from_millis(16)).is_empty());
        // The third frame completes the 50 ms tick.
        let writes = run(&mut animator, Duration::from_millis(20));
        assert_eq!(writes, [(3, vec![2, 2, 2, 255])]);
    }

    #[test]
    fn a_long_frame_runs_one_tick_per_50_ms() {
        let mut animator = TextureAnimator::new(vec![blinking(0)]);
        let writes = run(&mut animator, Duration::from_millis(100));
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[1].1, [1, 1, 1, 255]);
    }

    /// A stall must not replay seconds of animation in one frame.
    #[test]
    fn a_stall_is_capped_to_a_few_ticks() {
        let mut animator = TextureAnimator::new(vec![blinking(0)]);
        let writes = run(&mut animator, Duration::from_secs(10));
        assert_eq!(writes.len(), MAX_CATCH_UP_TICKS as usize);
        assert!(run(&mut animator, Duration::from_millis(10)).is_empty());
    }

    #[test]
    fn an_animator_without_layers_does_nothing() {
        let mut animator = TextureAnimator::default();
        assert!(animator.is_empty());
        assert!(run(&mut animator, Duration::from_secs(1)).is_empty());
    }
}

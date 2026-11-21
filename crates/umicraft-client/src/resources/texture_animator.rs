// SPDX-License-Identifier: MIT OR Apache-2.0

//! Drives the animated block textures (water, lava, fire, portal, ...).
//!
//! The reference client calls `TextureAtlasSprite.updateAnimation` once per
//! game tick (20 Hz), whatever the frame rate. The window feeds the elapsed
//! frame time to [`TextureAnimator::advance`], which steps every animation as
//! many whole ticks as have passed and hands each changed layer to a writer
//! (the renderer), so a frame that crosses no tick uploads nothing.
//!
//! The Animations page of the video options freezes whole groups of layers
//! (OptiFine `TextureMap.updateAnimations` / `isTerrainAnimationActive`). The
//! group of each layer is decided once, when the animator is built, so a frame
//! only tests one flag per animation.
//!
//! The custom animations of the packs (`anim/*.properties`, see
//! [`super::custom_animations`]) run on the same clock and are one more group:
//! the Animated Textures switch (OptiFine `Config.isAnimatedTextures` guards
//! `TextureAnimations.updateAnimations`).

use std::time::Duration;

use umicraft_assets::TextureAnimation;

use super::custom_animations::CustomAnimation;
use super::terrain::InstalledTerrain;

/// Length of a game tick.
const TICK: Duration = Duration::from_millis(50);

/// After a stall (window drag, breakpoint) the animations resume instead of
/// replaying every missed tick: at most this many ticks run per call.
const MAX_CATCH_UP_TICKS: u32 = 4;

/// The group of animated block textures one switch of the Animations page
/// governs. Compass and clock are dials, which always run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Group {
    Water,
    Lava,
    Fire,
    Portal,
    /// Every other animated block texture.
    Terrain,
}

/// The sprites that form the named groups; anything else is [`Group::Terrain`].
const NAMED_GROUPS: [(&str, Group); 7] = [
    ("blocks/water_still", Group::Water),
    ("blocks/water_flow", Group::Water),
    ("blocks/lava_still", Group::Lava),
    ("blocks/lava_flow", Group::Lava),
    ("blocks/fire_layer_0", Group::Fire),
    ("blocks/fire_layer_1", Group::Fire),
    ("blocks/portal", Group::Portal),
];

/// Which groups of animated textures keep playing. The default plays all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimationFilter {
    /// Water still and flowing.
    pub water: bool,
    /// Lava still and flowing.
    pub lava: bool,
    /// Both fire layers.
    pub fire: bool,
    /// The nether portal.
    pub portal: bool,
    /// Every other animated block texture.
    pub terrain: bool,
    /// The custom animations of the resource packs.
    pub custom: bool,
}

impl Default for AnimationFilter {
    fn default() -> Self {
        Self {
            water: true,
            lava: true,
            fire: true,
            portal: true,
            terrain: true,
            custom: true,
        }
    }
}

impl AnimationFilter {
    fn plays(self, group: Group) -> bool {
        match group {
            Group::Water => self.water,
            Group::Lava => self.lava,
            Group::Fire => self.fire,
            Group::Portal => self.portal,
            Group::Terrain => self.terrain,
        }
    }
}

/// One animated layer and the group it belongs to.
#[derive(Debug)]
struct Layer {
    animation: TextureAnimation,
    group: Group,
}

/// The animated layers of the bound block array and their tick clock.
#[derive(Debug, Default)]
pub struct TextureAnimator {
    animations: Vec<Layer>,
    custom: Vec<CustomAnimation>,
    filter: AnimationFilter,
    /// Frame time not yet turned into a tick.
    pending: Duration,
}

impl TextureAnimator {
    /// An animator for `animations`, each at its first step. `layer_of` gives
    /// the atlas layer of a sprite key (`blocks/water_still`), which tells the
    /// water, lava, fire and portal layers from the rest.
    #[must_use]
    pub fn new(
        animations: Vec<TextureAnimation>,
        filter: AnimationFilter,
        layer_of: impl Fn(&str) -> Option<u32>,
    ) -> Self {
        let named = NAMED_GROUPS.map(|(key, group)| (layer_of(key), group));
        let animations = animations
            .into_iter()
            .map(|animation| {
                let group = named
                    .iter()
                    .find(|(layer, _)| *layer == Some(animation.layer()))
                    .map_or(Group::Terrain, |&(_, group)| group);
                Layer { animation, group }
            })
            .collect();
        Self {
            animations,
            custom: Vec::new(),
            filter,
            pending: Duration::ZERO,
        }
    }

    /// This animator with the custom animations of the packs added.
    #[must_use]
    pub fn with_custom(mut self, custom: Vec<CustomAnimation>) -> Self {
        self.custom = custom;
        self
    }

    /// An animator for the animated layers of `terrain`; the procedural set
    /// has none, and then nothing is told apart.
    #[must_use]
    pub fn for_terrain(terrain: &InstalledTerrain, filter: AnimationFilter) -> Self {
        let mesher = terrain.mesher();
        let layer_of = |key: &str| mesher.model_context()?.atlas.layer(key);
        Self::new(terrain.animations().to_vec(), filter, layer_of)
    }

    /// Chooses which groups keep playing; a stopped one keeps its picture.
    pub fn set_filter(&mut self, filter: AnimationFilter) {
        self.filter = filter;
    }

    /// Whether no layer is animated.
    #[must_use]
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.animations.is_empty() && self.custom.is_empty()
    }

    /// Runs the ticks that `elapsed` completes and calls `write(layer, pixels)`
    /// for every layer whose picture changed (once per tick at most) and that
    /// `in_view` accepts.
    ///
    /// Smart Animations passes the layers drawn last frame as `in_view`. A layer
    /// out of view still steps, so every animation keeps its phase; it only
    /// skips the upload and shows the picture it has when it comes back until
    /// its next step.
    pub fn advance(
        &mut self,
        elapsed: Duration,
        in_view: impl Fn(u32) -> bool,
        mut write: impl FnMut(u32, &[u8]),
    ) {
        if self.animations.is_empty() && self.custom.is_empty() {
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
            for entry in &mut self.animations {
                if !self.filter.plays(entry.group) {
                    continue;
                }
                let layer = entry.animation.layer();
                if let Some(pixels) = entry.animation.tick() {
                    if in_view(layer) {
                        write(layer, pixels);
                    }
                }
            }
            if !self.filter.custom {
                continue;
            }
            for entry in &mut self.custom {
                let layer = entry.layer();
                if let Some(pixels) = entry.tick() {
                    if in_view(layer) {
                        write(layer, pixels);
                    }
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

    /// The layers of the named sprites in the tests: water 1, lava 2, fire 3.
    fn layers(key: &str) -> Option<u32> {
        match key {
            "blocks/water_still" => Some(1),
            "blocks/lava_flow" => Some(2),
            "blocks/fire_layer_1" => Some(3),
            _ => None,
        }
    }

    fn animator(layers: &[u32], filter: AnimationFilter) -> TextureAnimator {
        TextureAnimator::new(
            layers.iter().map(|&l| blinking(l)).collect(),
            filter,
            self::layers,
        )
    }

    fn run(animator: &mut TextureAnimator, elapsed: Duration) -> Vec<(u32, Vec<u8>)> {
        let mut writes = Vec::new();
        animator.advance(
            elapsed,
            |_| true,
            |layer, pixels| writes.push((layer, pixels.to_vec())),
        );
        writes
    }

    #[test]
    fn a_frame_shorter_than_a_tick_uploads_nothing() {
        let mut animator = animator(&[3], AnimationFilter::default());
        assert!(run(&mut animator, Duration::from_millis(16)).is_empty());
        assert!(run(&mut animator, Duration::from_millis(16)).is_empty());
        // The third frame completes the 50 ms tick.
        let writes = run(&mut animator, Duration::from_millis(20));
        assert_eq!(writes, [(3, vec![2, 2, 2, 255])]);
    }

    #[test]
    fn a_long_frame_runs_one_tick_per_50_ms() {
        let mut animator = animator(&[0], AnimationFilter::default());
        let writes = run(&mut animator, Duration::from_millis(100));
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[1].1, [1, 1, 1, 255]);
    }

    /// A stall must not replay seconds of animation in one frame.
    #[test]
    fn a_stall_is_capped_to_a_few_ticks() {
        let mut animator = animator(&[0], AnimationFilter::default());
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

    #[test]
    fn layers_are_grouped_by_sprite_name_and_the_rest_is_terrain() {
        let animator = animator(&[1, 2, 3, 9], AnimationFilter::default());
        let groups: Vec<Group> = animator.animations.iter().map(|l| l.group).collect();
        assert_eq!(
            groups,
            [Group::Water, Group::Lava, Group::Fire, Group::Terrain]
        );
    }

    #[test]
    fn a_stopped_group_keeps_its_picture_and_the_others_run() {
        let filter = AnimationFilter {
            water: false,
            terrain: false,
            ..AnimationFilter::default()
        };
        let mut animator = animator(&[1, 2, 9], filter);
        let writes = run(&mut animator, Duration::from_millis(100));
        assert!(writes.iter().all(|(layer, _)| *layer == 2));
        assert_eq!(writes.len(), 2);
    }

    #[test]
    fn a_layer_out_of_view_steps_but_uploads_nothing() {
        let mut animator = animator(&[1, 2], AnimationFilter::default());
        let mut writes = Vec::new();
        animator.advance(
            Duration::from_millis(100),
            |layer| layer == 2,
            |layer, _| writes.push(layer),
        );
        assert_eq!(writes, [2, 2]);
        // Layer 1 kept its phase: two steps later it shows the first frame
        // again, so the next tick is the second frame once more.
        let writes = run(&mut animator, Duration::from_millis(50));
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[0], (1, vec![2, 2, 2, 255]));
    }

    #[test]
    fn a_stopped_group_resumes_where_it_froze() {
        let off = AnimationFilter {
            water: false,
            ..AnimationFilter::default()
        };
        let mut animator = animator(&[1], off);
        assert!(run(&mut animator, Duration::from_millis(200)).is_empty());
        animator.set_filter(AnimationFilter::default());
        let writes = run(&mut animator, Duration::from_millis(50));
        // Still on its first step, so the first tick shows the second frame.
        assert_eq!(writes, [(1, vec![2, 2, 2, 255])]);
    }
}

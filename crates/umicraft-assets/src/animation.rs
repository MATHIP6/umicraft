// SPDX-License-Identifier: MIT OR Apache-2.0

//! Texture animations: the `.mcmeta` timeline and its per-tick state machine.
//!
//! A 1.8 animated texture is a vertical strip of square frames plus a
//! `<name>.png.mcmeta` file (`{"animation": {"frametime": 2, "frames": [...],
//! "interpolate": true}}`). The atlas keeps the first frame in the layer; a
//! [`TextureAnimation`] owns every frame (already scaled to the atlas edge) and
//! tells the renderer, once per game tick, which pixels the layer shows now.
//!
//! The timeline mirrors `TextureAtlasSprite.updateAnimation` and
//! `updateAnimationInterpolated` of the 1.8 reference (see `docs/sources.md`).

use serde_json::Value;

/// One entry of the animation timeline: a frame shown for a number of ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnimationStep {
    /// Index of the frame in the strip.
    pub(crate) frame: usize,
    /// How many ticks the frame stays, at least 1.
    pub(crate) ticks: u32,
}

/// The parsed `animation` section of a `.mcmeta` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AnimationMeta {
    /// Timeline in order; empty means "every frame once, `frametime` ticks each".
    pub(crate) steps: Vec<AnimationStep>,
    /// Default ticks per frame.
    pub(crate) frame_time: u32,
    /// Whether the frames blend into each other between two changes.
    pub(crate) interpolate: bool,
}

impl AnimationMeta {
    /// Parses a `.mcmeta` document.
    ///
    /// Returns `None` when the file is not JSON, has no `animation` section, or
    /// the section is invalid (a non-positive time, a non-integer index): the
    /// texture then stays on its first frame, like a texture without metadata.
    pub(crate) fn parse(bytes: &[u8]) -> Option<Self> {
        let document: Value = serde_json::from_slice(bytes).ok()?;
        let section = document.get("animation")?.as_object()?;
        let frame_time = match section.get("frametime") {
            None => 1,
            Some(value) => positive(value)?,
        };
        let mut steps = Vec::new();
        if let Some(frames) = section.get("frames") {
            for entry in frames.as_array()? {
                steps.push(match entry {
                    Value::Object(object) => AnimationStep {
                        frame: usize::try_from(object.get("index")?.as_u64()?).ok()?,
                        ticks: object.get("time").map_or(Some(frame_time), positive)?,
                    },
                    other => AnimationStep {
                        frame: usize::try_from(other.as_u64()?).ok()?,
                        ticks: frame_time,
                    },
                });
            }
        }
        let interpolate = section
            .get("interpolate")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Some(Self {
            steps,
            frame_time,
            interpolate,
        })
    }
}

impl AnimationMeta {
    /// The timeline of a strip of `frame_count` frames: the listed `frames`,
    /// or every frame in order for `frametime` ticks each.
    fn timeline(&self, frame_count: usize) -> Vec<AnimationStep> {
        if self.steps.is_empty() {
            (0..frame_count)
                .map(|frame| AnimationStep {
                    frame,
                    ticks: self.frame_time,
                })
                .collect()
        } else {
            self.steps.clone()
        }
    }
}

/// Which frame of an animated strip shows at a given tick, without the
/// pixels: for a model that maps the strip itself rather than reading the
/// atlas layer (the flames on a burning entity).
///
/// It follows the same `.mcmeta` timeline as [`TextureAnimation`], but never
/// blends frames (`interpolate` is ignored).
///
/// # Example
///
/// ```
/// use umicraft_assets::AnimationTimeline;
///
/// let timeline = AnimationTimeline::from_mcmeta(3, br#"{"animation": {"frames": [2, 0]}}"#)
///     .expect("an animation section");
/// assert_eq!([0, 1, 2].map(|tick| timeline.frame_at(tick)), [2, 0, 2]);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationTimeline {
    steps: Vec<AnimationStep>,
    /// Ticks of one loop of the timeline, at least 1.
    period: u32,
}

impl AnimationTimeline {
    /// The timeline of a strip of `frame_count` frames from the bytes of its
    /// `.mcmeta` file.
    ///
    /// Returns `None` when the metadata has no valid `animation` section or
    /// names no frame of the strip. Listed frames past the end of the strip
    /// are dropped.
    #[must_use]
    pub fn from_mcmeta(frame_count: usize, mcmeta: &[u8]) -> Option<Self> {
        let steps: Vec<AnimationStep> = AnimationMeta::parse(mcmeta)?
            .timeline(frame_count)
            .into_iter()
            .filter(|step| step.frame < frame_count)
            .collect();
        let period = steps
            .iter()
            .fold(0u32, |sum, step| sum.saturating_add(step.ticks));
        (period > 0).then_some(Self { steps, period })
    }

    /// Every one of `frame_count` frames in order, one tick each (a strip
    /// whose `.mcmeta` is `{"animation": {}}`). `None` for an empty strip.
    #[must_use]
    pub fn in_order(frame_count: usize) -> Option<Self> {
        Self::from_mcmeta(frame_count, br#"{"animation": {}}"#)
    }

    /// The index of the frame shown `tick` ticks after the first frame.
    #[must_use]
    pub fn frame_at(&self, tick: u32) -> usize {
        let mut left = tick % self.period;
        for step in &self.steps {
            if left < step.ticks {
                return step.frame;
            }
            left -= step.ticks;
        }
        // Unreachable: the ticks of the steps add up to `period`.
        self.steps.first().map_or(0, |step| step.frame)
    }
}

/// A strictly positive tick count that fits a `u32`.
fn positive(value: &Value) -> Option<u32> {
    u32::try_from(value.as_u64()?).ok().filter(|&n| n > 0)
}

/// An animated atlas layer: all its frames and its position on the timeline.
///
/// Cloning gives an independent copy restarted at the same position, so the
/// installed terrain can hand a fresh one to the window after a resource reload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureAnimation {
    layer: u32,
    /// RGBA8 frames, each exactly one atlas layer long.
    frames: Vec<Vec<u8>>,
    steps: Vec<AnimationStep>,
    interpolate: bool,
    /// Index into `steps` of the entry being shown.
    step: usize,
    /// Ticks the current entry has been shown.
    elapsed: u32,
    /// Scratch for the blended pixels, only used when interpolating.
    blended: Vec<u8>,
}

impl TextureAnimation {
    /// Builds the animation of `layer` from its decoded `frames`.
    ///
    /// Returns `None` when there is nothing to animate: fewer than two frames,
    /// or a timeline that never changes the frame. A missing `frames` list
    /// plays every frame in order.
    pub(crate) fn new(layer: u32, frames: Vec<Vec<u8>>, meta: &AnimationMeta) -> Option<Self> {
        if frames.len() < 2 {
            return None;
        }
        let steps = meta.timeline(frames.len());
        let first = steps.first()?.frame;
        if steps.iter().all(|step| step.frame == first) {
            return None;
        }
        let blended = if meta.interpolate {
            vec![0; frames.first()?.len()]
        } else {
            Vec::new()
        };
        Some(Self {
            layer,
            frames,
            steps,
            interpolate: meta.interpolate,
            step: 0,
            elapsed: 0,
            blended,
        })
    }

    /// Builds the animation of `layer` from its `frames` (RGBA8 layers of the
    /// atlas size) and the bytes of its `.mcmeta` file.
    ///
    /// Returns `None` when the metadata has no valid `animation` section or
    /// there is nothing to animate (see [`TextureAnimation::new`]).
    #[must_use]
    pub fn from_mcmeta(layer: u32, frames: Vec<Vec<u8>>, mcmeta: &[u8]) -> Option<Self> {
        Self::new(layer, frames, &AnimationMeta::parse(mcmeta)?)
    }

    /// The atlas layer this animation writes.
    #[must_use]
    pub fn layer(&self) -> u32 {
        self.layer
    }

    /// Advances one game tick and returns the new pixels of the layer when they
    /// changed (one RGBA8 layer), `None` when the layer keeps its picture.
    ///
    /// Mirrors `TextureAtlasSprite.updateAnimation`: the entry changes once it
    /// has been shown for its ticks; with `interpolate` the ticks in between
    /// show a blend of the current and the next frame.
    pub fn tick(&mut self) -> Option<&[u8]> {
        self.elapsed += 1;
        let current = self.steps[self.step];
        if self.elapsed >= current.ticks {
            self.step = (self.step + 1) % self.steps.len();
            self.elapsed = 0;
            let next = self.steps[self.step].frame;
            return (next != current.frame)
                .then(|| self.frames.get(next))
                .flatten()
                .map(Vec::as_slice);
        }
        if !self.interpolate {
            return None;
        }
        let next = self.steps[(self.step + 1) % self.steps.len()].frame;
        let (from, to) = (self.frames.get(current.frame)?, self.frames.get(next)?);
        if current.frame == next {
            return None;
        }
        // `d0` weighs the current frame; the alpha of the current frame is
        // kept as is (`updateAnimationInterpolated`), the colours truncate.
        let weight = 1.0 - f64::from(self.elapsed) / f64::from(current.ticks);
        for (index, ((out, &a), &b)) in self.blended.iter_mut().zip(from).zip(to).enumerate() {
            *out = if index % 4 == 3 {
                a
            } else {
                blend(a, b, weight)
            };
        }
        Some(&self.blended)
    }
}

/// `a * weight + b * (1 - weight)` truncated to a byte, like the reference's
/// `(int)` cast.
fn blend(a: u8, b: u8, weight: f64) -> u8 {
    (f64::from(a) * weight + f64::from(b) * (1.0 - weight)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One 1×1 RGBA frame.
    fn frame(rgba: [u8; 4]) -> Vec<u8> {
        rgba.to_vec()
    }

    fn meta(json: &str) -> AnimationMeta {
        AnimationMeta::parse(json.as_bytes()).expect("valid animation metadata")
    }

    #[test]
    fn the_default_timeline_is_every_frame_for_one_tick() {
        let parsed = meta(r#"{"animation": {}}"#);
        assert_eq!(parsed.frame_time, 1);
        assert!(parsed.steps.is_empty());
        assert!(!parsed.interpolate);
    }

    #[test]
    fn frames_may_be_indices_or_timed_objects() {
        let parsed =
            meta(r#"{"animation": {"frametime": 3, "frames": [0, {"index": 2, "time": 5}, 1]}}"#);
        let ticks: Vec<_> = parsed.steps.iter().map(|s| (s.frame, s.ticks)).collect();
        assert_eq!(ticks, [(0, 3), (2, 5), (1, 3)]);
    }

    #[test]
    fn invalid_metadata_is_ignored() {
        for json in [
            "not json",
            "{}",
            r#"{"animation": {"frametime": 0}}"#,
            r#"{"animation": {"frames": [{"time": 2}]}}"#,
            r#"{"animation": {"frames": [{"index": 0, "time": 0}]}}"#,
            r#"{"animation": {"frames": "all"}}"#,
        ] {
            assert_eq!(AnimationMeta::parse(json.as_bytes()), None, "{json}");
        }
    }

    #[test]
    fn a_timeline_follows_the_listed_frames_and_their_times() {
        let mcmeta =
            br#"{"animation": {"frametime": 2, "frames": [1, {"index": 0, "time": 1}, 9]}}"#;
        let timeline = AnimationTimeline::from_mcmeta(2, mcmeta).expect("animated");
        // Frame 9 is past the strip and dropped: the loop is 1, 1, 0.
        let frames: Vec<usize> = (0..6).map(|tick| timeline.frame_at(tick)).collect();
        assert_eq!(frames, [1, 1, 0, 1, 1, 0]);
    }

    #[test]
    fn a_timeline_without_frames_plays_the_strip_in_order() {
        let timeline = AnimationTimeline::in_order(16).expect("animated");
        assert_eq!(timeline.frame_at(5), 5);
        assert_eq!(timeline.frame_at(16 + 3), 3);
        assert!(AnimationTimeline::in_order(0).is_none());
        assert!(AnimationTimeline::from_mcmeta(4, b"{}").is_none());
    }

    #[test]
    fn a_frame_change_waits_for_the_frame_time() {
        let parsed = meta(r#"{"animation": {"frametime": 2}}"#);
        let mut animation = TextureAnimation::new(
            7,
            vec![
                frame([1, 0, 0, 255]),
                frame([2, 0, 0, 255]),
                frame([3, 0, 0, 255]),
            ],
            &parsed,
        )
        .expect("three frames animate");
        assert_eq!(animation.layer(), 7);
        assert_eq!(animation.tick(), None);
        assert_eq!(animation.tick(), Some(&[2, 0, 0, 255][..]));
        assert_eq!(animation.tick(), None);
        assert_eq!(animation.tick(), Some(&[3, 0, 0, 255][..]));
        assert_eq!(animation.tick(), None);
        // The timeline loops back to the first frame.
        assert_eq!(animation.tick(), Some(&[1, 0, 0, 255][..]));
    }

    #[test]
    fn a_custom_timeline_replays_frames_in_its_own_order() {
        let parsed = meta(
            r#"{"animation": {"frames": [{"index": 1, "time": 2}, {"index": 0, "time": 1}]}}"#,
        );
        let mut animation = TextureAnimation::new(
            0,
            vec![frame([1, 0, 0, 255]), frame([2, 0, 0, 255])],
            &parsed,
        )
        .expect("two frames animate");
        // The layer starts on frame 0 (the atlas upload), the first step shows
        // frame 1 for two ticks, then frame 0 for one.
        assert_eq!(animation.tick(), None);
        assert_eq!(animation.tick(), Some(&[1, 0, 0, 255][..]));
        assert_eq!(animation.tick(), Some(&[2, 0, 0, 255][..]));
    }

    #[test]
    fn a_still_texture_has_no_animation() {
        let parsed = meta(r#"{"animation": {}}"#);
        assert!(TextureAnimation::new(0, vec![frame([1, 2, 3, 4])], &parsed).is_none());
        let constant = meta(r#"{"animation": {"frames": [0, 0]}}"#);
        let frames = vec![frame([1, 2, 3, 4]), frame([5, 6, 7, 8])];
        assert!(TextureAnimation::new(0, frames, &constant).is_none());
    }

    /// Interpolation blends towards the next frame and keeps the alpha.
    #[test]
    fn interpolation_blends_the_colours_between_frame_changes() {
        let parsed = meta(r#"{"animation": {"frametime": 4, "interpolate": true}}"#);
        let mut animation = TextureAnimation::new(
            0,
            vec![frame([200, 100, 0, 255]), frame([0, 0, 100, 10])],
            &parsed,
        )
        .expect("two frames animate");
        // Tick 1 of 4: weight 0.75 on the current frame.
        assert_eq!(animation.tick(), Some(&[150, 75, 25, 255][..]));
        assert_eq!(animation.tick(), Some(&[100, 50, 50, 255][..]));
        assert_eq!(animation.tick(), Some(&[50, 25, 75, 255][..]));
        // Tick 4 changes the frame outright.
        assert_eq!(animation.tick(), Some(&[0, 0, 100, 10][..]));
    }
    /// A literal port of `TextureAtlasSprite.updateAnimation` and
    /// `updateAnimationInterpolated` (1.8.8): `frame_counter` indexes the
    /// timeline, `tick_counter` counts the ticks the entry has been shown.
    struct Vanilla {
        /// `(index, time)` of each listed frame; empty = every frame in order.
        listed: Vec<(usize, u32)>,
        frame_time: u32,
        interpolate: bool,
        textures: Vec<[u8; 4]>,
        frame_counter: usize,
        tick_counter: u32,
    }

    impl Vanilla {
        fn index(&self, counter: usize) -> usize {
            self.listed.get(counter).map_or(counter, |entry| entry.0)
        }

        fn time(&self, counter: usize) -> u32 {
            self.listed
                .get(counter)
                .map_or(self.frame_time, |entry| entry.1)
        }

        fn count(&self) -> usize {
            if self.listed.is_empty() {
                self.textures.len()
            } else {
                self.listed.len()
            }
        }

        /// The pixel the sprite uploads this tick, if it uploads one.
        fn update(&mut self) -> Option<[u8; 4]> {
            self.tick_counter += 1;
            if self.tick_counter >= self.time(self.frame_counter) {
                let before = self.index(self.frame_counter);
                self.frame_counter = (self.frame_counter + 1) % self.count();
                self.tick_counter = 0;
                let after = self.index(self.frame_counter);
                return (before != after && after < self.textures.len())
                    .then(|| self.textures[after]);
            }
            if !self.interpolate {
                return None;
            }
            let weight =
                1.0 - f64::from(self.tick_counter) / f64::from(self.time(self.frame_counter));
            let current = self.index(self.frame_counter);
            let next = self.index((self.frame_counter + 1) % self.count());
            if current == next || next >= self.textures.len() {
                return None;
            }
            let (a, b) = (self.textures[current], self.textures[next]);
            let mix =
                |i: usize| (f64::from(a[i]) * weight + f64::from(b[i]) * (1.0 - weight)) as u8;
            Some([mix(0), mix(1), mix(2), a[3]])
        }
    }

    /// The timeline of every `.mcmeta` shape of the 1.8 jar (default order,
    /// `frametime`, listed indices, timed objects, `interpolate`) gives the
    /// same upload on every tick as the reference, over several loops.
    #[test]
    fn the_timeline_matches_the_reference_tick_by_tick() {
        let textures: Vec<[u8; 4]> = (0..5u8)
            .map(|i| [40 * i, 200 - 30 * i, 7 * i, 255 - i])
            .collect();
        let cases = [
            (r#"{"animation": {}}"#, vec![], 1, false),
            (r#"{"animation": {"frametime": 3}}"#, vec![], 3, false),
            (
                r#"{"animation": {"frametime": 4, "interpolate": true}}"#,
                vec![],
                4,
                true,
            ),
            (
                r#"{"animation": {"frametime": 2, "frames": [0, 1, 2, 3, 4, 3, 2, 1]}}"#,
                vec![
                    (0, 2),
                    (1, 2),
                    (2, 2),
                    (3, 2),
                    (4, 2),
                    (3, 2),
                    (2, 2),
                    (1, 2),
                ],
                2,
                false,
            ),
            (
                r#"{"animation": {"frames": [{"index": 1, "time": 3}, {"index": 1, "time": 2}, {"index": 4, "time": 5}], "interpolate": true}}"#,
                vec![(1, 3), (1, 2), (4, 5)],
                1,
                true,
            ),
        ];
        for (json, listed, frame_time, interpolate) in cases {
            let frames: Vec<Vec<u8>> = textures.iter().map(|t| t.to_vec()).collect();
            let mut ours = TextureAnimation::from_mcmeta(0, frames, json.as_bytes())
                .expect("the timeline animates");
            let mut reference = Vanilla {
                listed,
                frame_time,
                interpolate,
                textures: textures.clone(),
                frame_counter: 0,
                tick_counter: 0,
            };
            for tick in 0..400 {
                let expected = reference.update();
                assert_eq!(
                    ours.tick().map(|p| [p[0], p[1], p[2], p[3]]),
                    expected,
                    "{json} tick {tick}"
                );
            }
        }
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Custom texture animations: a pack stacks the frames of an animation
//! vertically in one picture and a `.properties` file says which rectangle of
//! which texture they play into.
//!
//! ```text
//! from=./clock_frames.png   # the frames, one above the other
//! to=textures/blocks/stone.png
//! x=0
//! y=0
//! w=16
//! h=16
//! duration=4                # ticks per frame, 1 when absent
//! tile.0=0                  # optional play order: frame index per step
//! duration.0=10             # optional length of one step
//! ```
//!
//! [`AnimSpec::parse`] reads the file, [`AnimSpec::schedule`] fixes the play
//! order once the number of frames of the source picture is known, and
//! [`FrameClock`] steps through it one game tick at a time.
//!
//! Behaviour from OptiFine's `TextureAnimations.makeTextureAnimation` and
//! `TextureAnimation.nextFrame` (ADR-0088, `docs/sources.md`).

use thiserror::Error;

use crate::sky::resolve_path;
use crate::Properties;

/// Why an animation file was dropped.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AnimError {
    /// `from` or `to` is missing.
    #[error("source or target texture not specified")]
    MissingTexture,
    /// A coordinate is missing, not a number, or negative, or the rectangle
    /// is empty.
    #[error("invalid animation rectangle")]
    InvalidRectangle,
    /// The source picture is not a whole number of frames tall.
    #[error("source has {height} rows, which is not a whole number of {frame_height}-row frames")]
    PartialFrame {
        /// Rows of the source picture.
        height: u32,
        /// Rows of one frame (`h`).
        frame_height: u32,
    },
    /// The source picture holds no frame.
    #[error("source picture holds no frame")]
    NoFrames,
}

/// One step of the play order: which frame shows, and for how many ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    /// The index of the frame in the source picture, counted from the top.
    pub frame: u32,
    /// Ticks the step lasts; `0` counts as one.
    pub duration: u32,
}

/// What an animation file says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimSpec {
    /// Full virtual path of the source picture.
    pub from: String,
    /// Full virtual path of the target texture.
    pub to: String,
    /// Left edge of the target rectangle.
    pub x: u32,
    /// Top edge of the target rectangle.
    pub y: u32,
    /// Width of one frame and of the target rectangle.
    pub width: u32,
    /// Height of one frame and of the target rectangle.
    pub height: u32,
    /// The listed `tile.N` entries, in order; `None` when the file lists none,
    /// which plays every frame of the source once, in order.
    tiles: Option<Vec<u32>>,
    /// `duration`, the length of a step that has no `duration.N`.
    duration: u32,
    /// `duration.N` for each listed step.
    step_durations: Vec<Option<u32>>,
}

/// An integer the way OptiFine's `Config.parseInt` reads it: trimmed, and the
/// fallback when the text is absent or not a number.
fn int_or(text: Option<&str>, fallback: i64) -> i64 {
    text.and_then(|text| text.trim().parse().ok())
        .unwrap_or(fallback)
}

/// A coordinate: absent, malformed and negative values are all "invalid".
fn coordinate(props: &Properties, key: &str) -> Result<u32, AnimError> {
    u32::try_from(int_or(props.get(key), -1)).map_err(|_| AnimError::InvalidRectangle)
}

impl AnimSpec {
    /// Reads an animation file found at the full virtual `path`
    /// (`assets/minecraft/optifine/anim/x.properties`), which `from` and `to`
    /// may be relative to.
    ///
    /// # Errors
    ///
    /// [`AnimError::MissingTexture`] without `from` or `to`;
    /// [`AnimError::InvalidRectangle`] without four valid coordinates or with
    /// an empty frame.
    pub fn parse(props: &Properties, path: &str) -> Result<Self, AnimError> {
        let (Some(from), Some(to)) = (props.get("from"), props.get("to")) else {
            return Err(AnimError::MissingTexture);
        };
        let spec = Self {
            from: resolve_path(from, path),
            to: resolve_path(to, path),
            x: coordinate(props, "x")?,
            y: coordinate(props, "y")?,
            width: coordinate(props, "w")?,
            height: coordinate(props, "h")?,
            tiles: listed_tiles(props),
            duration: duration_of(props.get("duration"), 1),
            step_durations: Vec::new(),
        };
        if spec.width == 0 || spec.height == 0 {
            return Err(AnimError::InvalidRectangle);
        }
        let durations = spec
            .tiles
            .iter()
            .flatten()
            .enumerate()
            .map(|(step, _)| {
                props
                    .get(&format!("duration.{step}"))
                    .map(|text| duration_of(Some(text), spec.duration))
            })
            .collect();
        Ok(Self {
            step_durations: durations,
            ..spec
        })
    }

    /// The play order for a source picture of `source_height` rows: the listed
    /// `tile.N` steps, or every frame once.
    ///
    /// A step that names a frame the picture does not have is kept (the
    /// animation still waits its duration) and the caller skips the copy.
    ///
    /// # Errors
    ///
    /// [`AnimError::PartialFrame`] when the height is not a multiple of `h`,
    /// and [`AnimError::NoFrames`] when it is zero.
    pub fn schedule(&self, source_height: u32) -> Result<Vec<Step>, AnimError> {
        if !source_height.is_multiple_of(self.height) {
            return Err(AnimError::PartialFrame {
                height: source_height,
                frame_height: self.height,
            });
        }
        let frames = source_height / self.height;
        if frames == 0 {
            return Err(AnimError::NoFrames);
        }
        let steps = match &self.tiles {
            Some(tiles) => tiles
                .iter()
                .zip(&self.step_durations)
                .map(|(&frame, duration)| Step {
                    frame,
                    duration: duration.unwrap_or(self.duration),
                })
                .collect(),
            None => (0..frames)
                .map(|frame| Step {
                    frame,
                    duration: self.duration,
                })
                .collect(),
        };
        Ok(steps)
    }
}

/// The `tile.N` indices: they run from `tile.0` while consecutive keys exist,
/// and a value that is not a number falls back to its own step number
/// (`TextureAnimation`'s constructor).
fn listed_tiles(props: &Properties) -> Option<Vec<u32>> {
    props.get("tile.0")?;
    let tiles = (0..)
        .map_while(|step| {
            let text = props.get(&format!("tile.{step}"))?;
            Some(u32::try_from(int_or(Some(text), i64::from(step))).unwrap_or(step))
        })
        .collect();
    Some(tiles)
}

/// A duration in ticks; a negative one counts as zero.
fn duration_of(text: Option<&str>, fallback: u32) -> u32 {
    u32::try_from(int_or(text, i64::from(fallback)).max(0)).unwrap_or(fallback)
}

/// Steps through a schedule one game tick at a time.
///
/// Each step shows for its duration; when it ends the next one starts, and
/// the clock wraps after the last (`TextureAnimation.nextFrame`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameClock {
    steps: Vec<Step>,
    active: usize,
    counter: u32,
}

impl FrameClock {
    /// A clock at the start of `steps`.
    #[must_use]
    pub fn new(steps: Vec<Step>) -> Self {
        Self {
            steps,
            active: 0,
            counter: 0,
        }
    }

    /// The frame shown now; `0` for an empty schedule.
    #[must_use]
    pub fn current_frame(&self) -> u32 {
        self.steps.get(self.active).map_or(0, |step| step.frame)
    }

    /// Advances one tick. Returns the frame to show when the step changed,
    /// `None` when the picture stays.
    pub fn tick(&mut self) -> Option<u32> {
        let step = self.steps.get(self.active)?;
        self.counter += 1;
        if self.counter < step.duration {
            return None;
        }
        self.counter = 0;
        self.active = (self.active + 1) % self.steps.len();
        Some(self.current_frame())
    }
}

#[cfg(test)]
mod tests;

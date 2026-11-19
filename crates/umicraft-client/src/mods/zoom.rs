// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Zoom mod: while the key is held the field of view narrows, and it glides
//! there and back instead of jumping.
//!
//! The state is a multiplier of the field of view, kept as a logarithm so that going
//! in and out feels the same at every level, and moved towards its target by an
//! exponential that does not depend on the frame rate. The wheel changes the level
//! while the key is held. The window asks [`Zoom::step`] once per frame.

/// The zoom factor the wheel can reach, as the multiplier of the field of view.
const MIN_FACTOR: f32 = 1.2;
const MAX_FACTOR: f32 = 30.0;
/// How much one notch of the wheel changes the factor.
const WHEEL_STEP: f32 = 1.15;
/// How fast the glide closes the distance, per second: about a quarter of a second
/// to settle.
const GLIDE_RATE: f32 = 14.0;

/// What the Zoom mod is set to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoomSettings {
    /// How many times the view is magnified while the key is held.
    pub factor: f32,
    /// Whether the field of view glides, or jumps.
    pub smooth: bool,
    /// Whether the wheel changes the zoom while the key is held.
    pub scroll: bool,
    /// Whether the mouse turns slower in proportion to the zoom.
    pub slow_mouse: bool,
}

/// The zoom of the player.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Zoom {
    /// The natural logarithm of the multiplier of the field of view now: `0` is no
    /// zoom, below it the view is narrower.
    log_scale: f32,
    /// Notches of the wheel since the key went down.
    notches: f32,
}

impl Zoom {
    /// The factor wanted: the setting moved by the wheel, within bounds.
    fn target_factor(&self, settings: &ZoomSettings) -> f32 {
        let wheel = if settings.scroll {
            WHEEL_STEP.powf(self.notches)
        } else {
            1.0
        };
        (settings.factor * wheel).clamp(MIN_FACTOR, MAX_FACTOR)
    }

    /// Turns the wheel by `notches` (up is positive, it zooms in) while the key is
    /// held. Returns whether the wheel was taken, so it does not also scroll the
    /// hotbar.
    pub fn wheel(&mut self, notches: f32, held: bool, settings: Option<&ZoomSettings>) -> bool {
        let Some(settings) = settings.filter(|settings| settings.scroll && held) else {
            return false;
        };
        self.notches += notches;
        // Do not wind the wheel past the limits: it would take as many notches back.
        let wanted = settings.factor * WHEEL_STEP.powf(self.notches);
        if !(MIN_FACTOR..=MAX_FACTOR).contains(&wanted) {
            self.notches -= notches;
        }
        true
    }

    /// Moves the zoom by `seconds` towards what the key asks for and returns the
    /// multiplier to put on the field of view, `1.0` when not zooming.
    pub fn step(&mut self, seconds: f32, held: bool, settings: Option<&ZoomSettings>) -> f32 {
        let target = match settings.filter(|_| held) {
            Some(settings) => -self.target_factor(settings).ln(),
            None => {
                self.notches = 0.0;
                0.0
            }
        };
        let smooth = settings.is_none_or(|settings| settings.smooth);
        if smooth {
            // Independent of the frame rate: the same share of the distance per second.
            let share = 1.0 - (-GLIDE_RATE * seconds.clamp(0.0, 0.25)).exp();
            self.log_scale += (target - self.log_scale) * share;
            if (target - self.log_scale).abs() < 1e-4 {
                self.log_scale = target;
            }
        } else {
            self.log_scale = target;
        }
        self.log_scale.exp()
    }

    /// The multiplier of the speed of the mouse: the current field of view share when
    /// `slow_mouse` is on, so a narrow view is aimed with the same care.
    #[must_use]
    pub fn look_factor(&self, settings: Option<&ZoomSettings>) -> f32 {
        match settings {
            Some(settings) if settings.slow_mouse => self.log_scale.exp(),
            _ => 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTINGS: ZoomSettings = ZoomSettings {
        factor: 4.0,
        smooth: true,
        scroll: true,
        slow_mouse: true,
    };

    fn run(zoom: &mut Zoom, held: bool, frames: usize, seconds: f32) -> Vec<f32> {
        (0..frames)
            .map(|_| zoom.step(seconds, held, Some(&SETTINGS)))
            .collect()
    }

    #[test]
    fn without_the_key_nothing_changes() {
        let mut zoom = Zoom::default();
        assert_eq!(zoom.step(0.016, false, Some(&SETTINGS)), 1.0);
        assert_eq!(zoom.step(0.016, true, None), 1.0, "the mod is off");
    }

    #[test]
    fn the_zoom_glides_in_without_overshooting_and_arrives() {
        let mut zoom = Zoom::default();
        let path = run(&mut zoom, true, 120, 0.016);
        assert!(path.windows(2).all(|w| w[1] <= w[0] + 1e-6), "only narrows");
        assert!(
            path[0] < 1.0 && path[0] > 0.6,
            "it starts gently: {}",
            path[0]
        );
        assert!(
            path.iter().all(|&s| s >= 0.25 - 1e-4),
            "never past the factor"
        );
        assert!(
            (path[119] - 0.25).abs() < 1e-3,
            "and it arrives: {}",
            path[119]
        );
    }

    #[test]
    fn the_zoom_glides_out_again_when_the_key_is_let_go() {
        let mut zoom = Zoom::default();
        run(&mut zoom, true, 120, 0.016);
        let back = run(&mut zoom, false, 200, 0.016);
        assert!(back.windows(2).all(|w| w[1] >= w[0] - 1e-6), "only widens");
        assert_eq!(back[199], 1.0);
    }

    #[test]
    fn the_glide_does_not_depend_on_the_frame_rate() {
        let (mut fast, mut slow) = (Zoom::default(), Zoom::default());
        run(&mut fast, true, 60, 1.0 / 240.0);
        run(&mut slow, true, 15, 1.0 / 60.0);
        let (a, b) = (fast.log_scale, slow.log_scale);
        assert!(
            (a - b).abs() < 0.02,
            "{a} against {b} after a quarter of a second"
        );
    }

    #[test]
    fn without_smoothing_it_jumps() {
        let mut zoom = Zoom::default();
        let jump = ZoomSettings {
            smooth: false,
            ..SETTINGS
        };
        assert!((zoom.step(0.016, true, Some(&jump)) - 0.25).abs() < 1e-6);
        assert_eq!(zoom.step(0.016, false, Some(&jump)), 1.0);
    }

    #[test]
    fn the_wheel_changes_the_level_while_the_key_is_held_and_stops_at_the_ends() {
        let mut zoom = Zoom::default();
        assert!(!zoom.wheel(1.0, false, Some(&SETTINGS)), "the key is up");
        assert!(!zoom.wheel(1.0, true, None), "the mod is off");
        assert!(zoom.wheel(3.0, true, Some(&SETTINGS)));
        let level = run(&mut zoom, true, 200, 0.016)[199];
        assert!(level < 0.25, "three notches in: {level}");
        for _ in 0..200 {
            zoom.wheel(1.0, true, Some(&SETTINGS));
        }
        let most = run(&mut zoom, true, 200, 0.016)[199];
        assert!(
            (1.0 / MAX_FACTOR - 1e-4..=WHEEL_STEP / MAX_FACTOR).contains(&most),
            "within a notch of the limit: {most}"
        );
        zoom.step(0.016, false, Some(&SETTINGS));
        assert_eq!(zoom.notches, 0.0, "letting go forgets the wheel");
    }

    #[test]
    fn the_mouse_slows_with_the_zoom_when_asked() {
        let mut zoom = Zoom::default();
        run(&mut zoom, true, 200, 0.016);
        assert!((zoom.look_factor(Some(&SETTINGS)) - 0.25).abs() < 1e-3);
        let plain = ZoomSettings {
            slow_mouse: false,
            ..SETTINGS
        };
        assert_eq!(zoom.look_factor(Some(&plain)), 1.0);
        assert_eq!(zoom.look_factor(None), 1.0);
    }
}

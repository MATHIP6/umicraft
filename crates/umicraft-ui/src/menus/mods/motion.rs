// SPDX-License-Identifier: MIT OR Apache-2.0

//! The animations of the mods menu: the knob of a switch, the lighting of a
//! row, the slide of the tab line and the way the page comes
//! in.
//!
//! Things move like a mass on a spring, not along a fixed curve: they overshoot
//! their end and settle, and what moves fast can be stretched by its speed. The
//! UI keeps no clock, so the screen owns the state: [`Motion`] holds the
//! springs by name, [`tick`](Motion::tick) integrates them by one game tick and
//! the readers interpolate to the frame being drawn.

use std::collections::HashMap;

use crate::modern::HoverFade;

/// Seconds in one game tick.
const TICK: f32 = 0.05;
/// Integration steps per tick; the spring stays stable at its stiffest.
const SUBSTEPS: u32 = 4;
/// Ticks since a page came in after which its entrance is over.
const SETTLED: u32 = 100;
/// Ticks the panel takes to fade in.
const PANEL_TICKS: f32 = 5.0;
/// Ticks between one row starting to come in and the next.
const STAGGER: f32 = 1.5;
/// Ticks a row takes to come in.
const ROW_TICKS: f32 = 7.0;
/// How far below its place a row starts, in GUI units.
const RISE: f32 = 7.0;

/// The stiffness and damping of a glide: critically damped, so it never overshoots,
/// and settled in about a quarter of a second.
const GLIDE: (f32, f32) = (22.0, 1.0);

/// A mass on a spring heading for `target`.
#[derive(Debug, Clone, Copy)]
struct Spring {
    pos: f32,
    prev: f32,
    vel: f32,
    target: f32,
    /// Angular frequency, in radians per second: how stiff it is.
    omega: f32,
    /// Damping ratio: below `1.0` it overshoots, the lower the more.
    zeta: f32,
}

impl Spring {
    fn at_rest(value: f32, (omega, zeta): (f32, f32)) -> Self {
        Self {
            pos: value,
            prev: value,
            vel: 0.0,
            target: value,
            omega,
            zeta,
        }
    }

    fn tick(&mut self) {
        self.prev = self.pos;
        let dt = TICK / SUBSTEPS as f32;
        for _ in 0..SUBSTEPS {
            let pull = self.omega * self.omega * (self.target - self.pos);
            let drag = 2.0 * self.zeta * self.omega * self.vel;
            self.vel += (pull - drag) * dt;
            self.pos += self.vel * dt;
        }
        if (self.pos - self.target).abs() < 1e-3 && self.vel.abs() < 1e-2 {
            self.pos = self.target;
            self.vel = 0.0;
        }
    }

    /// The position `partial` of the way into the current tick.
    fn at(&self, partial: f32) -> f32 {
        self.prev + (self.pos - self.prev) * partial
    }
}

/// A value that fades to one end or the other.
#[derive(Debug, Clone, Copy)]
struct Fade {
    fade: HoverFade,
    target: bool,
}

/// The animated values of the menu, by name.
#[derive(Debug, Clone)]
pub(super) struct Motion {
    springs: HashMap<String, Spring>,
    fades: HashMap<String, Fade>,
    /// How far into the current tick the frame is.
    partial: f32,
    /// Ticks since the page came in.
    age: u32,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            springs: HashMap::new(),
            fades: HashMap::new(),
            partial: 0.0,
            age: SETTLED,
        }
    }
}

impl Motion {
    /// Sets how far into the current tick the frame being drawn is.
    pub(super) fn set_partial(&mut self, partial_tick: f32) {
        self.partial = partial_tick.clamp(0.0, 1.0);
    }

    /// Advances everything one tick.
    pub(super) fn tick(&mut self) {
        for spring in self.springs.values_mut() {
            spring.tick();
        }
        for slot in self.fades.values_mut() {
            slot.fade.tick(slot.target);
        }
        self.age = (self.age + 1).min(SETTLED);
    }

    /// Starts the entrance of a page: the panel fades in and its rows come up
    /// one after the other.
    pub(super) fn restart(&mut self) {
        self.age = 0;
    }

    /// Ends the entrance at once.
    pub(super) fn skip_entrance(&mut self) {
        self.age = SETTLED;
    }

    /// The position of `key` and its speed, per second, heading for `target`.
    /// A value seen for the first time starts there instead of travelling to it,
    /// so a page opens with its switches already where they are. It overshoots
    /// a little and settles.
    pub(super) fn spring(&mut self, key: &str, target: f32) -> (f32, f32) {
        let partial = self.partial;
        let spring = self
            .springs
            .entry(key.to_owned())
            .or_insert_with(|| Spring::at_rest(target, (14.0, 0.5)));
        spring.target = target;
        (spring.at(partial), spring.vel)
    }

    /// The position of `key` gliding to `target` without overshooting: a scrolled
    /// list eases to where the wheel sent it and stops there. With `instant` it is
    /// at `target` at once (the thumb of the scrollbar under the pointer).
    pub(super) fn glide(&mut self, key: &str, target: f32, instant: bool) -> f32 {
        let partial = self.partial;
        let spring = self
            .springs
            .entry(key.to_owned())
            .or_insert_with(|| Spring::at_rest(target, GLIDE));
        if instant {
            *spring = Spring::at_rest(target, GLIDE);
        }
        spring.target = target;
        spring.at(partial)
    }

    /// Forgets the glide of `key`, so the next one starts where it is asked to be.
    pub(super) fn settle(&mut self, key: &str) {
        self.springs.remove(key);
    }

    /// [`spring`](Self::spring) for something that is on or off, `1.0` or `0.0`.
    pub(super) fn level(&mut self, key: &str, on: bool) -> (f32, f32) {
        self.spring(key, f32::from(u8::from(on)))
    }

    /// A fade of `key` to `on`, eased, for something the pointer lights up.
    pub(super) fn hover(&mut self, key: &str, on: bool) -> f32 {
        let partial = self.partial;
        let slot = self.fades.entry(key.to_owned()).or_insert(Fade {
            fade: HoverFade::settled(f32::from(u8::from(on))),
            target: on,
        });
        slot.target = on;
        let t = slot.fade.level(partial);
        t * t * (3.0 - 2.0 * t)
    }

    /// How opaque the panel is while it fades in, `0.0` to `1.0`.
    pub(super) fn panel_opacity(&self) -> f32 {
        ((self.age as f32 + self.partial) / PANEL_TICKS).clamp(0.0, 1.0)
    }

    /// The entrance of row number `index`: its opacity (the panel's included)
    /// and how many GUI units below its place it still is. Rows start one after
    /// the other and decelerate into place.
    pub(super) fn row(&self, index: usize) -> (f32, f32) {
        let start = index.min(8) as f32 * STAGGER;
        let t = ((self.age as f32 + self.partial - start) / ROW_TICKS).clamp(0.0, 1.0);
        let ease = 1.0 - (1.0 - t).powi(3);
        (self.panel_opacity() * t, RISE * (1.0 - ease))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The position of `key` heading for `target` after each of `ticks` ticks.
    fn run(motion: &mut Motion, key: &str, target: f32, ticks: usize) -> Vec<f32> {
        motion.set_partial(1.0);
        motion.spring(key, target);
        (0..ticks)
            .map(|_| {
                motion.tick();
                motion.spring(key, target).0
            })
            .collect()
    }

    #[test]
    fn a_new_value_starts_where_it_is_asked_to_be() {
        let mut motion = Motion::default();
        assert_eq!(motion.level("a", true).0, 1.0);
        assert_eq!(motion.level("b", false).0, 0.0);
    }

    #[test]
    fn a_spring_overshoots_its_end_and_settles_on_it() {
        let mut motion = Motion::default();
        motion.spring("a", 0.0);
        let path = run(&mut motion, "a", 1.0, 40);
        let peak = path.iter().copied().fold(0.0, f32::max);
        assert!(peak > 1.02, "it overshoots: {peak}");
        assert!(peak < 1.3, "but not wildly: {peak}");
        assert_eq!(path.last().copied(), Some(1.0), "it settles: {path:?}");
        assert!(path[0] > 0.0 && path[0] < 1.0, "it takes time: {path:?}");
    }

    #[test]
    fn a_spring_is_fast_in_the_middle() {
        let mut motion = Motion::default();
        motion.spring("a", 0.0);
        let mut path = vec![0.0];
        path.extend(run(&mut motion, "a", 1.0, 10));
        let steps: Vec<f32> = path.windows(2).map(|pair| pair[1] - pair[0]).collect();
        let fastest = steps.iter().copied().fold(0.0, f32::max);
        assert!(fastest > steps[0] * 1.4, "it accelerates: {steps:?}");
    }

    #[test]
    fn the_frame_between_two_ticks_is_in_between() {
        let mut motion = Motion::default();
        motion.spring("a", 0.0);
        motion.spring("a", 1.0);
        motion.tick();
        motion.set_partial(0.0);
        let before = motion.spring("a", 1.0).0;
        motion.set_partial(1.0);
        let after = motion.spring("a", 1.0).0;
        motion.set_partial(0.5);
        let middle = motion.spring("a", 1.0).0;
        assert!(
            before < middle && middle < after,
            "{before} {middle} {after}"
        );
    }

    #[test]
    fn two_names_do_not_share_a_value() {
        let mut motion = Motion::default();
        motion.spring("a", 0.0);
        motion.spring("b", 0.0);
        run(&mut motion, "a", 1.0, 3);
        assert_eq!(motion.spring("b", 0.0).0, 0.0);
    }

    #[test]
    fn the_rows_come_in_one_after_the_other_and_rise_into_place() {
        let mut motion = Motion::default();
        motion.restart();
        motion.set_partial(0.0);
        let (first_opacity, first_rise) = motion.row(0);
        let (_, third_rise) = motion.row(2);
        assert_eq!(first_opacity, 0.0);
        assert!(first_rise > third_rise - 1e-3 || first_rise > 0.0);
        for _ in 0..4 {
            motion.tick();
        }
        let (a, rise_a) = motion.row(0);
        let (b, rise_b) = motion.row(5);
        assert!(a > b, "an earlier row is further along: {a} {b}");
        assert!(rise_a < rise_b, "and has risen more: {rise_a} {rise_b}");
        for _ in 0..30 {
            motion.tick();
        }
        assert_eq!(motion.row(5), (1.0, 0.0));
        assert_eq!(motion.panel_opacity(), 1.0);
    }

    #[test]
    fn the_hover_fades_instead_of_jumping() {
        let mut motion = Motion::default();
        motion.hover("a", false);
        motion.set_partial(1.0);
        motion.hover("a", true);
        motion.tick();
        let first = motion.hover("a", true);
        assert!(first > 0.0 && first < 1.0, "{first}");
    }

    #[test]
    fn a_glide_arrives_without_overshooting() {
        let mut motion = Motion::default();
        motion.set_partial(1.0);
        motion.glide("list", 0.0, false);
        let path: Vec<f32> = (0..30)
            .map(|_| {
                motion.tick();
                motion.glide("list", 100.0, false)
            })
            .collect();
        assert!(path.windows(2).all(|w| w[1] >= w[0] - 1e-4), "{path:?}");
        assert!(
            path.iter().all(|&p| p <= 100.0 + 1e-3),
            "never past the end"
        );
        assert!(
            path[2] > 5.0 && path[2] < 95.0,
            "it travels, it does not jump"
        );
        assert_eq!(path[29], 100.0, "and it arrives");
    }

    #[test]
    fn an_instant_glide_and_a_settled_one_start_at_their_target() {
        let mut motion = Motion::default();
        motion.set_partial(1.0);
        assert_eq!(motion.glide("list", 40.0, false), 40.0, "first sight");
        motion.glide("list", 80.0, false);
        assert_eq!(motion.glide("list", 80.0, true), 80.0, "instant");
        motion.glide("page", 10.0, false);
        motion.settle("page");
        assert_eq!(motion.glide("page", 0.0, false), 0.0, "forgotten");
    }
}

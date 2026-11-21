// SPDX-License-Identifier: MIT OR Apache-2.0

//! Dynamic Lights: the few light sources of a frame (a burning mob, a held
//! torch, a dropped glowstone) that raise the block light around them.
//!
//! The client collects up to [`MAX_DYNAMIC_LIGHTS`] [`DynamicLight`]s into a
//! [`DynamicLightSet`] and hands it to the renderer, which packs it into the
//! camera uniform; `dynamic_lights.wgsl` then lifts the block-light level of
//! every terrain fragment and entity vertex. The set is a fixed array, so the
//! per-frame path allocates nothing, and an empty set (the Off option) costs
//! the shaders one comparison.
//!
//! Behaviour from OptiFine's `DynamicLights` (ADR-0088, `docs/sources.md`):
//! a light reaches 7.5 blocks, its level falls off linearly with distance,
//! the highest light wins, and a light in water loses two levels and reaches
//! as if the distance were doubled in square.

use bytemuck::{Pod, Zeroable};
use glam::DVec3;
#[cfg(test)]
use glam::Vec3;

/// The most lights one frame carries. The nearest to the camera are kept.
pub const MAX_DYNAMIC_LIGHTS: usize = 16;

/// The reach of a light in blocks (`DynamicLights.MAX_DIST`).
pub const LIGHT_RADIUS: f32 = 7.5;

/// What a light in water loses (`DynamicLights.getLightLevel(BlockPos)` takes
/// two off when the water is not clear).
const WATER_LEVEL_LOSS: u8 = 2;

/// One light source.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DynamicLight {
    /// Where it shines from, in world coordinates (f64: the renderer turns it
    /// into a small offset from its origin).
    pub position: DVec3,
    /// The block-light level at the source, `1..=15`.
    pub level: u8,
    /// Whether the source is inside water.
    pub underwater: bool,
}

/// The lights of one frame: a fixed array that never allocates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DynamicLightSet {
    lights: [DynamicLight; MAX_DYNAMIC_LIGHTS],
    len: usize,
}

impl Default for DynamicLightSet {
    fn default() -> Self {
        Self {
            lights: [DynamicLight::default(); MAX_DYNAMIC_LIGHTS],
            len: 0,
        }
    }
}

impl DynamicLightSet {
    /// Removes every light.
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// The lights in the set.
    #[must_use]
    pub fn as_slice(&self) -> &[DynamicLight] {
        &self.lights[..self.len]
    }

    /// Whether the set holds no light.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Adds `light`. A light of level 0 is dropped. When the set is full, the
    /// light farthest from `eye` is dropped, whether it is the new one or one
    /// already in the set, so the lights near the camera survive.
    pub fn push(&mut self, light: DynamicLight, eye: DVec3) {
        if light.level == 0 {
            return;
        }
        if self.len < MAX_DYNAMIC_LIGHTS {
            self.lights[self.len] = light;
            self.len += 1;
            return;
        }
        let distance = |one: &DynamicLight| one.position.distance_squared(eye);
        let Some((farthest, _)) = self.lights[..self.len]
            .iter()
            .enumerate()
            .max_by(|a, b| distance(a.1).total_cmp(&distance(b.1)))
        else {
            return;
        };
        if distance(&light) < distance(&self.lights[farthest]) {
            self.lights[farthest] = light;
        }
    }
}

/// A light as the shader reads it (`DynamicLight` in `dynamic_lights.wgsl`).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct GpuLight {
    /// Relative to the render origin.
    position: [f32; 3],
    /// The level after the water loss.
    level: f32,
    /// `1`, or sqrt(2) in water: the reach shrinks as if the squared distance
    /// were doubled.
    distance_scale: f32,
    _pad: [f32; 3],
}

impl GpuLight {
    /// A light that shines on nothing, to fill the unused slots.
    pub(crate) const NONE: Self = Self {
        position: [0.0; 3],
        level: 0.0,
        distance_scale: 1.0,
        _pad: [0.0; 3],
    };

    /// The shader form of `light` for a camera whose render origin is
    /// `origin`.
    fn of(light: &DynamicLight, origin: DVec3) -> Self {
        let (level, distance_scale) = if light.underwater {
            (
                light.level.saturating_sub(WATER_LEVEL_LOSS),
                std::f32::consts::SQRT_2,
            )
        } else {
            (light.level, 1.0)
        };
        Self {
            position: (light.position - origin).as_vec3().to_array(),
            level: f32::from(level.min(15)),
            distance_scale,
            _pad: [0.0; 3],
        }
    }
}

/// The lights of `set` in shader form for a camera at `origin`, with how many
/// slots are used.
pub(crate) fn pack(set: &DynamicLightSet, origin: DVec3) -> ([GpuLight; MAX_DYNAMIC_LIGHTS], u32) {
    let mut packed = [GpuLight::NONE; MAX_DYNAMIC_LIGHTS];
    for (slot, light) in packed.iter_mut().zip(set.as_slice()) {
        *slot = GpuLight::of(light, origin);
    }
    (packed, set.len as u32)
}

#[cfg(test)]
/// The block-light level at `point` (relative to the render origin) once the
/// `lights` have lifted `block`: the Rust mirror of `dynamic_block_light` in
/// `dynamic_lights.wgsl`, which the tests pin to the reference formula.
#[must_use]
fn block_light_with(lights: &[GpuLight], block: f32, point: Vec3) -> f32 {
    lights.iter().fold(block, |best, light| {
        let reach = point.distance(Vec3::from(light.position)) * light.distance_scale;
        if reach <= LIGHT_RADIUS {
            best.max((1.0 - reach / LIGHT_RADIUS) * light.level)
        } else {
            best
        }
    })
}

#[cfg(test)]
/// The light level `light` alone gives a block at `point` (world
/// coordinates), `0.0..=15.0`: `DynamicLights.getLightLevel(BlockPos)` for one
/// source. The reference clamps the result to `0..=15`.
fn level_at(light: &DynamicLight, point: DVec3) -> f32 {
    let gpu = GpuLight::of(light, DVec3::ZERO);
    block_light_with(&[gpu], 0.0, point.as_vec3()).clamp(0.0, 15.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f64, level: u8) -> DynamicLight {
        DynamicLight {
            position: DVec3::new(x, 0.0, 0.0),
            level,
            underwater: false,
        }
    }

    #[test]
    fn the_level_falls_linearly_to_zero_at_seven_and_a_half_blocks() {
        let torch = at(0.0, 14);
        assert!((level_at(&torch, DVec3::ZERO) - 14.0).abs() < 1e-5);
        let half = level_at(&torch, DVec3::new(3.75, 0.0, 0.0));
        assert!((half - 7.0).abs() < 1e-5, "{half}");
        assert!(level_at(&torch, DVec3::new(7.5, 0.0, 0.0)).abs() < 1e-5);
        assert_eq!(level_at(&torch, DVec3::new(7.6, 0.0, 0.0)), 0.0);
    }

    #[test]
    fn the_distance_is_three_dimensional() {
        let torch = at(0.0, 15);
        let point = DVec3::new(3.0, 4.0, 0.0);
        let expected = (1.0 - 5.0 / 7.5) * 15.0;
        assert!((level_at(&torch, point) - expected as f32).abs() < 1e-5);
    }

    #[test]
    fn a_light_in_water_loses_two_levels_and_a_third_of_its_reach() {
        let wet = DynamicLight {
            underwater: true,
            ..at(0.0, 15)
        };
        assert!((level_at(&wet, DVec3::ZERO) - 13.0).abs() < 1e-5);
        // Doubling the squared distance puts the edge at 7.5 / sqrt(2) blocks.
        let edge = f64::from(LIGHT_RADIUS) / std::f64::consts::SQRT_2;
        assert!(level_at(&wet, DVec3::new(edge - 0.01, 0.0, 0.0)) > 0.0);
        assert_eq!(level_at(&wet, DVec3::new(edge + 0.01, 0.0, 0.0)), 0.0);
        let faint = DynamicLight {
            underwater: true,
            ..at(0.0, 2)
        };
        assert_eq!(level_at(&faint, DVec3::ZERO), 0.0);
    }

    #[test]
    fn the_brightest_light_wins_and_never_darkens_the_block_light() {
        let (lights, count) = pack(
            &{
                let mut set = DynamicLightSet::default();
                set.push(at(0.0, 8), DVec3::ZERO);
                set.push(at(1.0, 15), DVec3::ZERO);
                set
            },
            DVec3::ZERO,
        );
        let lights = &lights[..count as usize];
        let here = block_light_with(lights, 0.0, Vec3::ZERO);
        assert!((here - (1.0 - 1.0 / 7.5) * 15.0).abs() < 1e-4, "{here}");
        // A bright block light stays as it is.
        assert_eq!(block_light_with(lights, 15.0, Vec3::ZERO), 15.0);
        assert_eq!(block_light_with(&[], 6.0, Vec3::ZERO), 6.0);
    }

    #[test]
    fn positions_are_sent_relative_to_the_render_origin() {
        let mut set = DynamicLightSet::default();
        set.push(
            DynamicLight {
                position: DVec3::new(1_000_000.5, 64.0, -3.0),
                level: 10,
                underwater: false,
            },
            DVec3::ZERO,
        );
        let (packed, count) = pack(&set, DVec3::new(1_000_000.0, 60.0, 0.0));
        assert_eq!(count, 1);
        assert_eq!(packed[0].position, [0.5, 4.0, -3.0]);
    }

    #[test]
    fn an_empty_set_packs_to_zero_lights() {
        let (_, count) = pack(&DynamicLightSet::default(), DVec3::ZERO);
        assert_eq!(count, 0);
    }

    #[test]
    fn a_full_set_keeps_the_lights_nearest_the_eye() {
        let mut set = DynamicLightSet::default();
        for index in 0..MAX_DYNAMIC_LIGHTS {
            set.push(at(10.0 + index as f64, 15), DVec3::ZERO);
        }
        // Farther than all: dropped. Nearer: replaces the farthest, x = 25.
        set.push(at(100.0, 15), DVec3::ZERO);
        assert!(set.as_slice().iter().all(|light| light.position.x < 50.0));
        set.push(at(1.0, 15), DVec3::ZERO);
        let xs: Vec<f64> = set.as_slice().iter().map(|l| l.position.x).collect();
        assert!(xs.contains(&1.0));
        assert!(!xs.contains(&25.0));
        assert_eq!(set.as_slice().len(), MAX_DYNAMIC_LIGHTS);
    }

    #[test]
    fn a_light_without_level_is_dropped() {
        let mut set = DynamicLightSet::default();
        set.push(at(0.0, 0), DVec3::ZERO);
        assert!(set.is_empty());
        set.push(at(0.0, 3), DVec3::ZERO);
        set.clear();
        assert!(set.is_empty());
    }

    #[test]
    fn the_gpu_light_matches_the_wgsl_struct() {
        // `DynamicLight` in dynamic_lights.wgsl: vec3 + f32 at 0, f32 at 16,
        // 16-byte alignment, so a 32-byte stride.
        assert_eq!(std::mem::size_of::<GpuLight>(), 32);
        assert_eq!(std::mem::offset_of!(GpuLight, level), 12);
        assert_eq!(std::mem::offset_of!(GpuLight, distance_scale), 16);
    }

    #[test]
    fn the_shader_is_the_formula_of_this_module() {
        let shader = include_str!("dynamic_lights.wgsl");
        assert!(shader.contains("const DYNAMIC_LIGHT_RADIUS: f32 = 7.5;"));
        assert!(shader.contains("(1.0 - reach / DYNAMIC_LIGHT_RADIUS) * light.level"));
        assert!(shader.contains("distance(point, light.position) * light.distance_scale"));
    }
}

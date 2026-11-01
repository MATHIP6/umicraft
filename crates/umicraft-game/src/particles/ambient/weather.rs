// SPDX-License-Identifier: MIT OR Apache-2.0

//! What rain asks of the world beyond blocks, and the splashes it makes
//! (`EntityRenderer.addRainParticles`).

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;
use umicraft_world::fluid::Liquid;
use umicraft_world::{BlockPos, BlockState};

use super::emit::{emit, Requests};
use super::surface::is_open_air;
use crate::particles::{ParticleKind, ParticleSetting};
use crate::selection::selection_shape;
use crate::shapes::BlockSource;

/// How far around the viewer a drop can fall (`i = 10`).
const RAIN_RADIUS: u32 = 10;
/// [`RAIN_RADIUS`] as a block distance.
const RADIUS: i32 = RAIN_RADIUS as i32;
/// The seed multiplier of `random.setSeed(rendererUpdateCount * 312987231L)`.
const SEED_FACTOR: i64 = 312_987_231;
/// The highest block row of a column.
const TOP: i32 = 255;
/// The lift of a splash above the surface it hits (`(float)y + 0.1F`).
const SPLASH_LIFT: f32 = 0.1;

/// The weather and biome answers the reference reads from the world and the
/// biome tables, which the block accessor cannot give.
pub trait Weather {
    /// `World.canLightningStrike(pos)`: it rains, the sky is visible at `pos`
    /// and the biome can have rain.
    fn can_lightning_strike(&self, pos: BlockPos) -> bool;

    /// `biome.canSpawnLightningBolt() && biome.getFloatTemperature(pos) >= 0.15F`
    /// at `pos`: rain falls there as water and not as snow.
    fn rains_on_biome(&self, pos: BlockPos) -> bool;
}

/// A dry world: no rain anywhere.
#[derive(Debug, Clone, Copy, Default)]
pub struct Dry;

impl Weather for Dry {
    fn can_lightning_strike(&self, _pos: BlockPos) -> bool {
        false
    }

    fn rains_on_biome(&self, _pos: BlockPos) -> bool {
        false
    }
}

/// The inputs of one `addRainParticles` call.
#[derive(Debug, Clone, Copy)]
pub struct RainView {
    /// `World.getRainStrength(1.0F)`, 0 to 1.
    pub strength: f32,
    /// `GameSettings.fancyGraphics`; fast graphics halves the strength.
    pub fancy_graphics: bool,
    /// `GameSettings.particleSetting`.
    pub setting: ParticleSetting,
    /// `rendererUpdateCount`, which seeds the draws of the tick.
    pub update_count: i32,
}

/// The splash drops of one tick around `viewer` (MCP
/// `EntityRenderer.addRainParticles`, line 1509): up to `100 * strength^2`
/// random columns within 10 blocks, each making a droplet on what the rain
/// hits, or smoke on lava. The rain sound is not played.
///
/// The draws are seeded from `view.update_count` like the reference, so the
/// same tick always splashes the same columns. The height of a column is
/// scanned from the world top down to the lowest row that can matter.
pub fn rain_splashes<S, W>(
    world: &S,
    weather: &W,
    viewer: BlockPos,
    view: &RainView,
    out: &mut Requests,
) where
    S: BlockSource + ?Sized,
    W: Weather + ?Sized,
{
    let strength = if view.fancy_graphics {
        view.strength
    } else {
        view.strength / 2.0
    };
    let mut rng = JavaRandom::new((i64::from(view.update_count) * SEED_FACTOR) as u64);
    for _ in 0..drop_count(strength, view.setting) {
        let dx = rng.next_int(RAIN_RADIUS) as i32 - rng.next_int(RAIN_RADIUS) as i32;
        let dz = rng.next_int(RAIN_RADIUS) as i32 - rng.next_int(RAIN_RADIUS) as i32;
        let (x, z) = (viewer.x + dx, viewer.z + dz);
        let Some(height) = precipitation_height(world, x, z, viewer.y - RADIUS - 1) else {
            continue;
        };
        let surface = BlockPos::new(x, height, z);
        if height > viewer.y + RADIUS
            || height < viewer.y - RADIUS
            || !weather.rains_on_biome(surface)
        {
            continue;
        }
        let (across_x, across_z) = (rng.next_double(), rng.next_double());
        splash(world, surface, (across_x, across_z), out);
    }
}

/// `(int)(100.0F * f * f)`, thinned by the particle setting.
fn drop_count(strength: f32, setting: ParticleSetting) -> i32 {
    if strength == 0.0 {
        return 0;
    }
    let count = (100.0 * strength * strength) as i32;
    match setting {
        ParticleSetting::All => count,
        ParticleSetting::Decreased => count >> 1,
        ParticleSetting::Minimal => 0,
    }
}

/// `World.getPrecipitationHeight`: one above the highest block that stops a
/// drop, looking down from the world top to `lowest` (rows under it cannot be
/// splashed anyway). The reference never looks at row 0.
fn precipitation_height<S: BlockSource + ?Sized>(
    world: &S,
    x: i32,
    z: i32,
    lowest: i32,
) -> Option<i32> {
    (lowest.max(1)..=TOP)
        .rev()
        .find(|&y| !is_open_air(world.block(x, y, z)))
        .map(|y| y + 1)
}

/// The particle a drop makes where it lands on the block under `surface`.
fn splash<S: BlockSource + ?Sized>(
    world: &S,
    surface: BlockPos,
    (across_x, across_z): (f64, f64),
    out: &mut Requests,
) {
    let below = BlockPos::new(surface.x, surface.y - 1, surface.z);
    let state = world.block(below.x, below.y, below.z);
    let lift = |y: i32| f64::from(y as f32 + SPLASH_LIFT);
    if Liquid::Lava.contains(state) {
        // A lava block's `minY` is 0, so the smoke sits just over its top.
        let position = DVec3::new(
            f64::from(surface.x) + across_x,
            lift(surface.y),
            f64::from(surface.z) + across_z,
        );
        emit(out, ParticleKind::Smoke, position);
    } else if !state.is_air() {
        let position = DVec3::new(
            f64::from(below.x) + across_x,
            lift(below.y) + top_of(world, state, below),
            f64::from(below.z) + across_z,
        );
        emit(out, ParticleKind::Droplet, position);
    }
}

/// `Block.getBlockBoundsMaxY` after `setBlockBoundsBasedOnState`: the top of
/// the block's selection box, 1 for a liquid.
fn top_of<S: BlockSource + ?Sized>(world: &S, state: BlockState, pos: BlockPos) -> f64 {
    selection_shape(state, world, pos)
        .boxes()
        .iter()
        .map(|shape| shape.max.y)
        .reduce(f64::max)
        .unwrap_or(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::ambient::testing::{count_kind, World};

    struct Everywhere;

    impl Weather for Everywhere {
        fn can_lightning_strike(&self, _pos: BlockPos) -> bool {
            true
        }

        fn rains_on_biome(&self, _pos: BlockPos) -> bool {
            true
        }
    }

    fn view(strength: f32) -> RainView {
        RainView {
            strength,
            fancy_graphics: true,
            setting: ParticleSetting::All,
            update_count: 42,
        }
    }

    fn floor_world() -> World {
        let mut world = World::default();
        for x in -12..=12 {
            for z in -12..=12 {
                world.set(x, 63, z, BlockState::new(1, 0));
            }
        }
        world
    }

    fn splashes(world: &World, view: &RainView) -> Requests {
        let mut out = Vec::new();
        rain_splashes(world, &Everywhere, BlockPos::new(0, 64, 0), view, &mut out);
        out
    }

    #[test]
    fn the_drop_count_follows_strength_squared_and_the_setting() {
        assert_eq!(drop_count(1.0, ParticleSetting::All), 100);
        assert_eq!(drop_count(0.5, ParticleSetting::All), 25);
        assert_eq!(drop_count(1.0, ParticleSetting::Decreased), 50);
        assert_eq!(drop_count(1.0, ParticleSetting::Minimal), 0);
        assert_eq!(drop_count(0.0, ParticleSetting::All), 0);
    }

    #[test]
    fn fast_graphics_halve_the_strength() {
        let world = floor_world();
        let fancy = splashes(&world, &view(1.0)).len();
        let mut fast = view(1.0);
        fast.fancy_graphics = false;
        assert_eq!(fancy, 100);
        assert_eq!(splashes(&world, &fast).len(), 25);
    }

    #[test]
    fn drops_land_as_droplets_on_top_of_the_ground() {
        let out = splashes(&floor_world(), &view(1.0));
        assert!(out.iter().all(|r| r.kind == ParticleKind::Droplet));
        for request in &out {
            // Row 63 plus 0.1F (a float, hence the tolerance) plus the cube top of 1.
            assert!((request.position.y - 64.1).abs() < 1e-5);
            assert!(request.position.x.abs() < 20.0);
        }
    }

    #[test]
    fn the_same_tick_makes_the_same_splashes() {
        let world = floor_world();
        let first = splashes(&world, &view(1.0));
        let second = splashes(&world, &view(1.0));
        let positions = |out: &Requests| out.iter().map(|r| r.position).collect::<Vec<_>>();
        assert_eq!(positions(&first), positions(&second));
        let mut other = view(1.0);
        other.update_count = 43;
        assert_ne!(positions(&first), positions(&splashes(&world, &other)));
    }

    #[test]
    fn lava_smokes_instead_of_splashing() {
        let mut world = World::default();
        for x in -12..=12 {
            for z in -12..=12 {
                world.set(x, 63, z, BlockState::new(10, 0));
            }
        }
        let kinds: Vec<_> = splashes(&world, &view(1.0))
            .iter()
            .map(|r| r.kind)
            .collect();
        assert_eq!(count_kind(&kinds, ParticleKind::Smoke), 100);
    }

    #[test]
    fn a_roof_far_above_the_viewer_or_a_dry_biome_makes_no_splash() {
        let mut roofed = floor_world();
        for x in -12..=12 {
            for z in -12..=12 {
                roofed.set(x, 90, z, BlockState::new(1, 0));
            }
        }
        assert!(splashes(&roofed, &view(1.0)).is_empty());
        let mut out = Vec::new();
        rain_splashes(
            &floor_world(),
            &Dry,
            BlockPos::new(0, 64, 0),
            &view(1.0),
            &mut out,
        );
        assert!(out.is_empty());
    }
}

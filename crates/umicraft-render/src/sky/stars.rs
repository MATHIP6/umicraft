// SPDX-License-Identifier: MIT OR Apache-2.0

//! The star field (`RenderGlobal.renderStars`).
//!
//! 1500 candidate stars come from `new Random(10842)`; a candidate whose
//! random point lies outside the unit sphere (or too near its centre) is
//! dropped but still consumes its draws, so the survivors match the
//! reference one for one. Each star is a small square placed 100 blocks away
//! and turned to face the origin by the reference's explicit rotations.

use umicraft_core::java_random::JavaRandom;

use super::vertex::{quad, SkyVertex};

/// The seed of the reference star list.
const SEED: u64 = 10_842;
/// Candidate stars drawn.
const CANDIDATES: usize = 1500;
/// Distance of the star field from the eye.
const DISTANCE: f64 = 100.0;

/// The star quads as a triangle list, in the sky's unrotated frame.
pub(super) fn vertices() -> Vec<SkyVertex> {
    let mut random = JavaRandom::new(SEED);
    let mut vertices = Vec::new();
    for _ in 0..CANDIDATES {
        let x = f64::from(random.next_float() * 2.0 - 1.0);
        let y = f64::from(random.next_float() * 2.0 - 1.0);
        let z = f64::from(random.next_float() * 2.0 - 1.0);
        let half_size = f64::from(0.15 + random.next_float() * 0.1);
        let length_squared = x * x + y * y + z * z;
        if length_squared < 1.0 && length_squared > 0.01 {
            let scale = 1.0 / length_squared.sqrt();
            let direction = [x * scale, y * scale, z * scale];
            vertices.extend(star(
                direction,
                half_size,
                random.next_double() * std::f64::consts::TAU,
            ));
        }
    }
    vertices
}

/// One star: a square of half-size `half_size` around `direction * 100`,
/// spun by `spin` (a random angle `nextDouble * 2π`) about its own normal.
fn star(direction: [f64; 3], half_size: f64, spin: f64) -> [SkyVertex; 6] {
    let [x, y, z] = direction;
    let centre = [x * DISTANCE, y * DISTANCE, z * DISTANCE];
    let (azimuth_sin, azimuth_cos) = x.atan2(z).sin_cos();
    let (polar_sin, polar_cos) = x.hypot(z).atan2(y).sin_cos();
    let (spin_sin, spin_cos) = spin.sin_cos();

    let corner = |j: i32| {
        let a = f64::from((j & 2) - 1) * half_size;
        let b = f64::from(((j + 1) & 2) - 1) * half_size;
        let rotated_a = a * spin_cos - b * spin_sin;
        let rotated_b = b * spin_cos + a * spin_sin;
        let up = rotated_a * polar_sin;
        let depth = -rotated_a * polar_cos;
        let east = depth * azimuth_sin - rotated_b * azimuth_cos;
        let south = rotated_b * azimuth_sin + depth * azimuth_cos;
        SkyVertex::plain([
            (centre[0] + east) as f32,
            (centre[1] + up) as f32,
            (centre[2] + south) as f32,
        ])
    };
    quad([corner(0), corner(1), corner(2), corner(3)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_surviving_candidates_are_fixed_by_the_seed() {
        // 780 of the 1500 random points fall inside the sphere (computed
        // with an independent implementation of the LCG and the filter).
        assert_eq!(vertices().len(), 780 * 6);
    }

    #[test]
    fn the_first_star_matches_the_hand_computed_quad() {
        // Candidate 0 is rejected; candidate 1 is the first star. Its corners
        // were computed with an independent double-precision implementation
        // of `renderStars` over the Java LCG.
        let first = vertices();
        let expected = [
            [-53.425_28, 69.847_77, 47.613_8],
            [-53.224_716, 70.056_03, 47.532_39],
            [-53.068_456, 70.003_71, 47.783_52],
        ];
        for (vertex, want) in first.iter().take(3).zip(expected) {
            for axis in 0..3 {
                assert!(
                    (vertex.position[axis] - want[axis]).abs() < 2e-3,
                    "{:?} vs {want:?}",
                    vertex.position
                );
            }
        }
    }

    #[test]
    fn every_star_lies_100_blocks_from_the_eye() {
        for vertex in vertices().iter().step_by(6) {
            let [x, y, z] = vertex.position;
            let distance = (x * x + y * y + z * z).sqrt();
            assert!((distance - 100.0).abs() < 1.0, "{distance}");
        }
    }
}

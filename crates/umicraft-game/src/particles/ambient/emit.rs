// SPDX-License-Identifier: MIT OR Apache-2.0

//! Small helpers shared by the block families: how a spawn is queued and how
//! the reference adds a random float to a block coordinate.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use crate::particles::{ParticleKind, SpawnRequest};

/// The requests an ambient pass queues; the caller feeds them to
/// `ParticleEngine::spawn` with its viewer and setting.
pub type Requests = Vec<SpawnRequest<'static>>;

/// Queues `kind` at `position` with `velocity` (for some kinds a colour, see
/// the families in `behaviours`).
pub(super) fn emit_with(out: &mut Requests, kind: ParticleKind, position: DVec3, velocity: DVec3) {
    out.push(SpawnRequest {
        kind,
        arguments: &[],
        ignore_range: false,
        position,
        velocity,
    });
}

/// Queues `kind` at `position` without velocity (`0.0D, 0.0D, 0.0D` in the
/// reference).
pub(super) fn emit(out: &mut Requests, kind: ParticleKind, position: DVec3) {
    emit_with(out, kind, position, DVec3::ZERO);
}

/// `(double)((float)base + rand.nextFloat())`: the sum is a float, so it
/// rounds like one before it widens.
pub(super) fn float_add(base: i32, rng: &mut JavaRandom) -> f64 {
    f64::from(base as f32 + rng.next_float())
}

/// `(double)base + (double)rand.nextFloat()`: the sum is a double.
pub(super) fn double_add(base: i32, rng: &mut JavaRandom) -> f64 {
    f64::from(base) + f64::from(rng.next_float())
}

/// `Random.nextInt(2) * 2 - 1`: -1 or 1.
pub(super) fn random_sign(rng: &mut JavaRandom) -> i32 {
    rng.next_int(2) as i32 * 2 - 1
}

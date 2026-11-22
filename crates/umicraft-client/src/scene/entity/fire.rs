// SPDX-License-Identifier: MIT OR Apache-2.0

//! The flames on a burning entity (MCP `Render.renderEntityOnFire`).
//!
//! A stack of upright quads that face the camera, alternating the two fire
//! sprites of the block atlas, starting at the feet and climbing in steps of
//! 0.45 block while shrinking by a tenth each step.

use std::collections::HashMap;

use glam::Vec3;
use umicraft_assets::AnimationTimeline;
use umicraft_game::entity::EntityRenderSnapshot;
use umicraft_model::entity::TextureSlot;
use umicraft_render::EntityVertex;

/// Frames in the vanilla fire sprite strip (`fire_layer_0.png` is 16 x 512),
/// assumed when no strip was loaded (demos, tests).
const VANILLA_FIRE_FRAMES: usize = 32;
/// Full block and sky light: the fire does not depend on the light.
const FIRE_LIGHT: u32 = 0x0000_00FF;

/// How the flames map one fire sprite strip (`fire_layer_0` or `_1`).
///
/// The reference draws the flames with the atlas sprite, so they show the
/// frame the block animation shows: the strip of the pack, cut in square
/// frames, played in the order of its `.mcmeta`. A pack strip need not have
/// the 32 frames of the vanilla one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FireStrip {
    /// The frames of the strip and their order; `None` for a picture that is
    /// not animated, which is drawn whole, as the atlas keeps it.
    animation: Option<(usize, AnimationTimeline)>,
}

impl FireStrip {
    /// The strip of a `width` x `height` picture with its `.mcmeta` bytes, if
    /// any. Without metadata, or when the height is not a whole number of
    /// square frames, the picture is not animated
    /// (`TextureAtlasSprite.loadSprite`).
    #[must_use]
    pub(crate) fn new(width: u32, height: u32, mcmeta: Option<&[u8]>) -> Self {
        let frames = (width > 0 && height.is_multiple_of(width)).then(|| (height / width) as usize);
        let animation = frames.and_then(|frames| {
            let timeline = AnimationTimeline::from_mcmeta(frames, mcmeta?)?;
            Some((frames, timeline))
        });
        Self { animation }
    }

    /// The vertical texture span `(v_min, v_max)` of the frame shown `tick`
    /// ticks into the animation.
    fn v_span(&self, tick: u32) -> (f32, f32) {
        let Some((frames, timeline)) = &self.animation else {
            return (0.0, 1.0);
        };
        let frame = timeline.frame_at(tick) as f32;
        let frames = *frames as f32;
        (frame / frames, (frame + 1.0) / frames)
    }
}

impl Default for FireStrip {
    /// The vanilla strip: 32 frames, one tick each.
    fn default() -> Self {
        Self {
            animation: AnimationTimeline::in_order(VANILLA_FIRE_FRAMES)
                .map(|timeline| (VANILLA_FIRE_FRAMES, timeline)),
        }
    }
}

/// The two fire strips, indexed like [`TextureSlot::Fire`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FireStrips(pub(crate) [FireStrip; 2]);

/// Adds the flames of `ent` to `batches` when it is burning. `view_yaw` is
/// the yaw of the camera, in the convention of `umicraft_render::Camera`;
/// `strips` tells which frame of each sprite shows.
pub(super) fn add_fire(
    batches: &mut HashMap<TextureSlot, Vec<EntityVertex>>,
    ent: &EntityRenderSnapshot,
    view_yaw: f32,
    strips: &FireStrips,
) {
    if !ent.flags.on_fire() {
        return;
    }
    let forward = Vec3::new(view_yaw.sin(), 0.0, -view_yaw.cos());
    let right = Vec3::new(view_yaw.cos(), 0.0, view_yaw.sin());
    // One step of the animation per tick, as the atlas animation advances.
    let tick = ent.age.max(0.0) as u32;
    let spans = strips.0.each_ref().map(|strip| strip.v_span(tick));

    let size = ent.kind.width() * 1.4;
    let mut remaining = ent.kind.height() / size;
    let mut rise = 0.0_f32;
    let mut half = 0.5_f32;
    let mut depth = -0.3 + (remaining as i32) as f32 * 0.02;
    let mut index = 0u32;
    while remaining > 0.0 {
        let sprite = (index % 2) as usize;
        let slot = TextureSlot::Fire(sprite as u8);
        let (v_min, v_max) = spans[sprite];
        let (mut u_left, mut u_right) = (1.0, 0.0);
        if (index / 2).is_multiple_of(2) {
            (u_left, u_right) = (u_right, u_left);
        }
        // After `rotate(-viewY)` local +z points away from the camera and +x to
        // the left of the view: the quads start in front of the entity and step
        // away from the camera.
        let at = |x: f32, y: f32| {
            ent.position + (forward * depth - right * x) * size + Vec3::Y * ((y + rise) * size)
        };
        let corners = [
            (at(half, 0.0), [u_left, v_max]),
            (at(-half, 0.0), [u_right, v_max]),
            (at(-half, 1.4), [u_right, v_min]),
            (at(half, 1.4), [u_left, v_min]),
        ];
        let batch = batches.entry(slot).or_default();
        for corner in [0, 1, 2, 0, 2, 3] {
            let (position, uv) = corners[corner];
            batch.push(EntityVertex {
                position: position.to_array(),
                normal: [0.0, 1.0, 0.0],
                uv,
                light: FIRE_LIGHT,
            });
        }
        remaining -= 0.45;
        rise += 0.45;
        half *= 0.9;
        depth += 0.03;
        index += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::{EntityFlags, EntityId, EntityKind};

    fn burning(kind: EntityKind) -> EntityRenderSnapshot {
        let mut ent = EntityRenderSnapshot::new(EntityId(1), kind, Vec3::new(5.0, 64.0, 5.0));
        ent.flags = EntityFlags::new(0x01, 0x7F);
        ent
    }

    fn fire(ent: &EntityRenderSnapshot, view_yaw: f32) -> HashMap<TextureSlot, Vec<EntityVertex>> {
        let mut batches = HashMap::new();
        add_fire(&mut batches, ent, view_yaw, &FireStrips::default());
        batches
    }

    #[test]
    fn an_entity_that_is_not_burning_has_no_flames() {
        let ent = EntityRenderSnapshot::new(EntityId(1), EntityKind::Zombie, Vec3::ZERO);
        assert!(fire(&ent, 0.0).is_empty());
    }

    #[test]
    fn a_zombie_gets_six_quads_alternating_the_two_sprites() {
        // height 1.95 / (0.6 * 1.4) = 2.32 sprite heights: 6 steps of 0.45.
        let batches = fire(&burning(EntityKind::Zombie), 0.0);
        assert_eq!(batches[&TextureSlot::Fire(0)].len(), 3 * 6);
        assert_eq!(batches[&TextureSlot::Fire(1)].len(), 3 * 6);
    }

    #[test]
    fn the_flames_scale_with_the_width_and_climb_with_the_height() {
        let extent = |kind, axis: usize| -> f32 {
            let positions: Vec<f32> = fire(&burning(kind), 0.0)
                .values()
                .flatten()
                .map(|v| v.position[axis])
                .collect();
            positions.iter().cloned().fold(f32::MIN, f32::max)
                - positions.iter().cloned().fold(f32::MAX, f32::min)
        };
        let quads = |kind| -> usize { fire(&burning(kind), 0.0).values().map(Vec::len).sum() };
        assert!(quads(EntityKind::Zombie) > quads(EntityKind::Rabbit));
        // A giant is six times as wide: bigger flames, one step fewer (5 against 6).
        assert!(extent(EntityKind::Giant, 1) > 5.0 * extent(EntityKind::Zombie, 1));
        assert!(quads(EntityKind::Giant) < quads(EntityKind::Zombie));
    }

    #[test]
    fn the_quads_face_the_camera_and_start_at_the_feet() {
        for view_yaw in [0.0, 1.0, std::f32::consts::PI] {
            let ent = burning(EntityKind::Zombie);
            let batches = fire(&ent, view_yaw);
            let forward = Vec3::new(view_yaw.sin(), 0.0, -view_yaw.cos());
            for vertices in batches.values() {
                for quad in vertices.chunks(6) {
                    // Every vertex of a quad is at the same depth along the view.
                    let depth = |v: &EntityVertex| Vec3::from(v.position).dot(forward);
                    assert!(quad
                        .iter()
                        .all(|v| (depth(v) - depth(&quad[0])).abs() < 1e-4));
                }
            }
            let lowest = batches[&TextureSlot::Fire(0)]
                .iter()
                .map(|v| v.position[1])
                .fold(f32::MAX, f32::min);
            assert!((lowest - 64.0).abs() < 1e-4, "{lowest}");
            let highest = batches
                .values()
                .flatten()
                .map(|v| v.position[1])
                .fold(f32::MIN, f32::max);
            assert!(
                highest > 64.0 + 1.95,
                "the flames climb past the head: {highest}"
            );
        }
    }

    #[test]
    fn the_uv_picks_one_frame_of_the_strip() {
        let mut ent = burning(EntityKind::Zombie);
        ent.age = 5.4;
        let batches = fire(&ent, 0.0);
        let frames = VANILLA_FIRE_FRAMES as f32;
        assert_eq!(v_values(&batches, 0), [5.0 / frames, 6.0 / frames]);
    }

    /// The distinct `v` coordinates of the quads of sprite `sprite`.
    fn v_values(batches: &HashMap<TextureSlot, Vec<EntityVertex>>, sprite: u8) -> Vec<f32> {
        let mut vs: Vec<f32> = batches[&TextureSlot::Fire(sprite)]
            .iter()
            .map(|v| v.uv[1])
            .collect();
        vs.sort_by(f32::total_cmp);
        vs.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        vs
    }

    #[test]
    fn a_pack_strip_plays_its_own_frames_in_its_own_order() {
        // A 16 x 256 pack strip: 16 frames, listed backwards, two ticks each.
        let mcmeta = br#"{"animation": {"frametime": 2, "frames": [15, 14, 13]}}"#;
        let strip = FireStrip::new(16, 256, Some(mcmeta));
        let strips = FireStrips([strip.clone(), strip]);
        let mut ent = burning(EntityKind::Zombie);
        ent.age = 3.0;
        let mut batches = HashMap::new();
        add_fire(&mut batches, &ent, 0.0, &strips);
        // Tick 3 is the second frame of the list, 14 of 16.
        assert_eq!(v_values(&batches, 1), [14.0 / 16.0, 15.0 / 16.0]);
    }

    #[test]
    fn a_strip_without_animation_is_drawn_whole() {
        assert_eq!(FireStrip::new(16, 256, None).v_span(7), (0.0, 1.0));
        // Not a whole number of square frames.
        let odd = FireStrip::new(16, 250, Some(br#"{"animation": {}}"#));
        assert_eq!(odd.v_span(7), (0.0, 1.0));
    }
}

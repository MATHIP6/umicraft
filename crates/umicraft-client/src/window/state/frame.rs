// SPDX-License-Identifier: MIT OR Apache-2.0

//! What a frame gathers from the world view before drawing it; the buffers
//! are kept between frames so the hot path does not allocate.

use std::time::Instant;
use umicraft_render::HudState;

use crate::options::{self};
use crate::scene::nametag::PlacedTag;

/// What the last frame gathered from the world, kept to reuse its buffers.
pub(in crate::window) struct FrameScratch {
    /// Double-buffered HUD state on the render thread: synced from `WorldView`
    /// only when `hud_revision` or `options.chat` changes, avoiding per-frame
    /// heap clones of chat lines, tab list entries, and scoreboard rows.
    pub(in crate::window) hud: HudState,
    pub(in crate::window) hud_revision: u64,
    pub(in crate::window) hud_chat: Option<options::ChatOptions>,
    /// The entities of this frame, interpolated once: the tab list, the name tags and
    /// the scene all read them (perf 2, B4).
    pub(in crate::window) entities: Vec<umicraft_game::entity::EntityRenderSnapshot>,
    /// The Dynamic Lights of the entities, refreshed on OptiFine's schedule.
    pub(in crate::window) dynamic_lights: crate::scene::dynamic_lights::DynamicLightCollector,
    /// The particle quads of the frame, rebuilt in place.
    pub(in crate::window) particle_sprites: Vec<umicraft_render::ParticleSprite>,
    /// The name tags of the frame, drawn by the renderer in the world pass.
    pub(in crate::window) nametags: Vec<umicraft_render::Nametag>,
    /// Reusable screen-placed tag buffer so `place_tags_into` does not allocate per frame.
    pub(in crate::window) placed_tags: Vec<PlacedTag>,
    /// Reusable dynamic moving-blocks / flower-pot mesh buffer across frames.
    pub(in crate::window) moving_blocks_mesh: umicraft_mesh::Mesh,
    /// The pose of the hand in the last frame, to tell the motion blur when it moves.
    pub(in crate::window) last_hand: Option<umicraft_game::hand::HandPose>,
    /// The damage flash of the hearts in the tab list, and the instant its
    /// clock (milliseconds, and ticks of 50 ms) counts from.
    pub(in crate::window) tab_hearts: (umicraft_game::hud::tab_hearts::TabHearts, Instant),
}

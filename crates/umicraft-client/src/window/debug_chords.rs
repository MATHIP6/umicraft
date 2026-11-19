// SPDX-License-Identifier: MIT OR Apache-2.0

//! The F3 chords: a letter pressed while F3 is held.
//!
//! The set is the one of MCP `Minecraft.runTick` (1.8.9): A reloads the
//! chunks, B shows the hitboxes, D clears the chat, F steps the render
//! distance (Shift steps it down), H toggles the advanced item tooltips, S and
//! T reload the resource packs. G shows the chunk borders (a later vanilla
//! version's chord). P keeps its Umicraft meaning, the runtime profiler:
//! vanilla's pause on lost focus has no counterpart here.
//!
//! F3 alone toggles the text screen when it is released, so a chord can be
//! used without flashing the screen (the behaviour of later vanilla versions;
//! 1.8.9 toggled on press).

use umicraft_render::DebugLine;
use winit::keyboard::KeyCode;

use super::App;
use crate::debug_lines::{chunk_border_lines, hitbox_lines};
use crate::game::GameEvent;
use crate::options::RENDER_DISTANCE;

/// What a chord does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DebugChord {
    /// F3 + A: re-mesh every loaded chunk.
    ReloadChunks,
    /// F3 + B: toggle the entity hitboxes.
    Hitboxes,
    /// F3 + D: clear the chat history.
    ClearChat,
    /// F3 + F: raise the render distance by one chunk (lower with Shift).
    RenderDistance,
    /// F3 + G: toggle the chunk borders.
    ChunkBorders,
    /// F3 + H: toggle the advanced item tooltips.
    AdvancedTooltips,
    /// F3 + P: toggle the runtime profiler.
    Profiler,
    /// F3 + S or F3 + T: reload the resource packs.
    ReloadPacks,
}

impl DebugChord {
    /// The chord the letter `code` makes with F3, if any.
    pub(super) fn from_key(code: KeyCode) -> Option<Self> {
        Some(match code {
            KeyCode::KeyA => Self::ReloadChunks,
            KeyCode::KeyB => Self::Hitboxes,
            KeyCode::KeyD => Self::ClearChat,
            KeyCode::KeyF => Self::RenderDistance,
            KeyCode::KeyG => Self::ChunkBorders,
            KeyCode::KeyH => Self::AdvancedTooltips,
            KeyCode::KeyP => Self::Profiler,
            KeyCode::KeyS | KeyCode::KeyT => Self::ReloadPacks,
            _ => return None,
        })
    }
}

/// The render distance after F3 + F: one chunk up or (with `lower`) down,
/// kept in the range of the option (`GameSettings.setOptionValue`).
pub(super) fn stepped_render_distance(current: u8, lower: bool) -> u8 {
    let (min, max) = (*RENDER_DISTANCE.start(), *RENDER_DISTANCE.end());
    if lower {
        current.saturating_sub(1).max(min)
    } else {
        current.saturating_add(1).min(max)
    }
}

impl App {
    /// Whether F3 is held, which turns a letter into a chord.
    pub(super) fn f3_held(&self) -> bool {
        self.input.keys.contains(&KeyCode::F3)
    }

    /// Shows or hides the F3 text screen, which also drives the allocation
    /// counter the screen reports.
    pub(super) fn toggle_f3_screen(&mut self) {
        self.show_f3 = !self.show_f3;
        if self.show_f3 {
            umicraft_core::profiling::set_alloc_tracking(true);
            self.clock.last_alloc_snapshot = umicraft_core::profiling::AllocSnapshot::now();
        } else if !umicraft_core::profiling::is_enabled() {
            umicraft_core::profiling::set_alloc_tracking(false);
        }
    }

    /// Runs `chord`. F3 then no longer toggles the text screen when released.
    pub(super) fn run_debug_chord(&mut self, chord: DebugChord) {
        self.f3_chord_used = true;
        match chord {
            DebugChord::ReloadChunks => {
                self.send_event(GameEvent::ReMeshAll);
            }
            DebugChord::Hitboxes => self.show_hitboxes = !self.show_hitboxes,
            DebugChord::ClearChat => {
                self.send_event(GameEvent::ClearChat);
            }
            DebugChord::RenderDistance => self.step_render_distance(),
            DebugChord::ChunkBorders => self.show_chunk_borders = !self.show_chunk_borders,
            DebugChord::AdvancedTooltips => self.toggle_advanced_tooltips(),
            DebugChord::Profiler => {
                let enabled = umicraft_core::profiling::toggle();
                if !enabled && self.show_f3 {
                    umicraft_core::profiling::set_alloc_tracking(true);
                }
            }
            DebugChord::ReloadPacks => self.reload_resources(),
        }
    }

    /// The lines the toggled chords draw this frame (F3 + B and F3 + G).
    ///
    /// Empty without a world: the lines belong to a game, not to a menu.
    pub(super) fn debug_lines(&self) -> Vec<DebugLine> {
        if self.world.is_none() {
            return Vec::new();
        }
        let mut lines = Vec::new();
        if self.show_hitboxes {
            lines.extend(hitbox_lines(&self.frame.entities));
        }
        if self.show_chunk_borders {
            let eye = self.camera.eye();
            lines.extend(chunk_border_lines([eye.x, eye.y, eye.z]));
        }
        lines
    }

    /// F3 + F: changes the render distance and tells the server.
    fn step_render_distance(&mut self) {
        let shift = self.input.keys.contains(&KeyCode::ShiftLeft)
            || self.input.keys.contains(&KeyCode::ShiftRight);
        let video = &mut self.options.video;
        video.render_distance = stepped_render_distance(video.render_distance, shift);
        self.send_event(crate::options_effects::ServerSettings::of(&self.options).event());
        self.save_options();
    }

    /// F3 + H: flips the advanced item tooltips and keeps the choice.
    fn toggle_advanced_tooltips(&mut self) {
        let controls = &mut self.options.controls;
        controls.advanced_tooltips = !controls.advanced_tooltips;
        crate::tooltip_draw::set_advanced(controls.advanced_tooltips);
        self.save_options();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_chord_has_a_letter() {
        let letters = [
            (KeyCode::KeyA, DebugChord::ReloadChunks),
            (KeyCode::KeyB, DebugChord::Hitboxes),
            (KeyCode::KeyD, DebugChord::ClearChat),
            (KeyCode::KeyF, DebugChord::RenderDistance),
            (KeyCode::KeyG, DebugChord::ChunkBorders),
            (KeyCode::KeyH, DebugChord::AdvancedTooltips),
            (KeyCode::KeyP, DebugChord::Profiler),
            (KeyCode::KeyS, DebugChord::ReloadPacks),
            (KeyCode::KeyT, DebugChord::ReloadPacks),
        ];
        for (code, chord) in letters {
            assert_eq!(DebugChord::from_key(code), Some(chord));
        }
    }

    #[test]
    fn other_keys_are_not_chords() {
        assert_eq!(DebugChord::from_key(KeyCode::KeyW), None);
        assert_eq!(DebugChord::from_key(KeyCode::F1), None);
    }

    #[test]
    fn the_render_distance_steps_by_one_inside_its_range() {
        assert_eq!(stepped_render_distance(8, false), 9);
        assert_eq!(stepped_render_distance(8, true), 7);
    }

    #[test]
    fn the_render_distance_stops_at_the_ends_of_its_range() {
        assert_eq!(stepped_render_distance(*RENDER_DISTANCE.end(), false), 16);
        assert_eq!(stepped_render_distance(*RENDER_DISTANCE.start(), true), 2);
    }
}

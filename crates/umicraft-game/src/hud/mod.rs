// SPDX-License-Identifier: MIT OR Apache-2.0

//! The pure model of the HUD (ADR-0036): what the overlays show, decided
//! without a GPU or a font. The renderer measures and draws the result.

pub mod boss_bar;
pub mod sidebar;
pub mod tab_list;
pub mod team_names;
pub mod titles;

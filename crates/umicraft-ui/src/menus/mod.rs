// SPDX-License-Identifier: MIT OR Apache-2.0

//! The menu screens outside the game: title, server list, server editing,
//! confirmation, connecting, disconnected and statistics (ADR-0042).
//!
//! Each screen is a small state struct whose `show` draws one frame through
//! [`crate::Ui`] and returns what the player chose as an `Option` of its
//! outcome enum. The screens open no window, read no file and keep no clock
//! (callers drive animation with `tick`), so tests run them against a plain
//! [`crate::Ui`]. All texts are vanilla translation keys resolved through
//! [`umicraft_core::lang::Lang`]; positions are the MCP ones in GUI units
//! times the GUI scale.

pub mod achievements;
pub mod background;
mod common;
pub mod confirm;
pub mod connecting;
pub mod disconnected;
pub mod mods;
pub mod pack_mode;
pub mod panorama;
mod row_draw;
pub mod server_edit;
pub mod server_list;
mod server_row;
pub mod stats;
pub mod title;

pub use achievements::{
    achievements_done_rect, AchievementArt, AchievementNode, AchievementsData, AchievementsOutcome,
    AchievementsScreen, MapTile, Progress, Toast, TOAST_DURATION,
};
pub use achievements::{draw_toast, toast_offset};
pub use background::{
    draw_dirt, draw_list_background, draw_list_overlay, draw_menu_background, BrandArt, MenuArt,
};
pub use common::ScrollbarDrag;
pub use confirm::{confirm_layout, ConfirmLayout, ConfirmOutcome, ConfirmScreen};
pub use connecting::{connecting_cancel_rect, ConnectStage, ConnectingOutcome, ConnectingScreen};
pub use disconnected::{DisconnectedOutcome, DisconnectedScreen};
pub use mods::{mods_layout, ModsLayout, ModsOutcome, ModsScreen};
pub use pack_mode::ResourcePackMode;
pub use panorama::{
    panorama_rotation, PanoramaBackground, PanoramaRotation, PanoramaTextures, PanoramaView,
    CLIP_PLANES, FACE_TURNS_DEGREES, FIELD_OF_VIEW_DEGREES,
};
pub use server_edit::{
    server_edit_layout, EditMode, ServerEditLayout, ServerEditOutcome, ServerEditScreen,
};
pub use server_list::{
    latency_level, server_list_layout, RowStatus, ServerListLayout, ServerListOutcome,
    ServerListScreen, ServerRow,
};
pub use stats::{
    stats_done_rect, stats_tab_rect, MobRow, ObjectRow, StatCell, StatRow, StatsData, StatsOutcome,
    StatsScreen, StatsTab,
};
pub use title::{
    modern_title_layout, title_layout, ModernTitleLayout, TitleInfo, TitleLayout, TitleOutcome,
    TitleScreen, TitleStyle, LOGO_TEXT, SPLASHES, WORDMARK,
};

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The pure rules of the in-game HUD (MCP `GuiIngame`): what the status rows,
//! the experience bar, the held-item name and the vehicle bars show for a
//! given player state.
//!
//! Every module turns plain inputs into a plain view struct; nothing here
//! knows about textures or the GPU, so the vanilla numbers (icon counts,
//! wiggle offsets, fade alphas) are pinned by unit tests. The renderer
//! (`umicraft_render::hud::gui`) maps the views to `icons.png` sprites.
//!
//! - [`vitals`]: hearts, armour and food rows ([`VitalsView`]);
//! - [`flash`]: the damage blink and the hurt-resistance countdown it reads;
//! - [`air`]: the air bubbles;
//! - [`mount`]: the health of a ridden mob and the horse jump power;
//! - [`xp`]: the experience bar, its level and the jump bar that replaces it;
//! - [`held_name`]: the name that fades in above the hotbar.

pub mod air;
pub mod boss_bar;
pub mod flash;
pub mod held_name;
pub mod mount;
pub mod sidebar;
pub mod tab_list;
pub mod team_names;
pub mod titles;
pub mod vitals;
pub mod xp;

pub use air::{air_view, AirView};
pub use flash::{FlashFrame, HealthFlash, HurtResistance};
pub use held_name::{HeldIdentity, HeldNameTracker, HeldNameView};
pub use mount::{HorseJump, MountHeartView, Riding};
pub use vitals::{
    vitals_view, ArmorView, Fill, FoodIconView, HeartBody, HeartStyle, HeartView, VitalsInput,
    VitalsView,
};
pub use xp::{bottom_bar, BottomBar, LEVEL_COLOR};

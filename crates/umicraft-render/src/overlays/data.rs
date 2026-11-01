// SPDX-License-Identifier: MIT OR Apache-2.0

//! The plain data of the screen overlays, filled by the client each frame.
//!
//! The renderer never decides *whether* an overlay shows (that needs the
//! world, the gamemode and the inventory); it only draws what it is told.

/// The overlays of one frame; `None` and `false` draw nothing.
///
/// Layers are layers of the block atlas, the whole picture of a sprite.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScreenOverlays {
    /// The sprite of the opaque block the eye is inside
    /// (`ItemRenderer.func_178108_a`).
    pub in_wall: Option<u32>,
    /// The water seen from below (`renderWaterOverlayTexture`).
    pub underwater: Option<Underwater>,
    /// The `fire_layer_1` layer of a burning player (`renderFireInFirstPerson`).
    pub fire: Option<u32>,
    /// The pumpkin on the head (`GuiIngame.renderPumpkinOverlay`).
    pub pumpkin: bool,
    /// The purple veil of a nether portal (`GuiIngame.func_180474_b`).
    pub portal: Option<Portal>,
    /// The multiplier of the vignette (`renderVignette`); `None` when off.
    pub vignette: Option<[f32; 3]>,
}

/// What the underwater picture needs: the light and where the player looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Underwater {
    /// `EntityPlayer.getBrightness` at the eye, `0.0..=1.0`.
    pub brightness: f32,
    /// The yaw in degrees (`rotationYaw`), which scrolls the picture sideways.
    pub yaw_degrees: f32,
    /// The pitch in degrees (`rotationPitch`), which scrolls it vertically.
    pub pitch_degrees: f32,
}

/// The portal veil.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Portal {
    /// The atlas layer of the portal block.
    pub layer: u32,
    /// The opacity, `0.0..=1.0`.
    pub alpha: f32,
}

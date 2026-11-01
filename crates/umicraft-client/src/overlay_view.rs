// SPDX-License-Identifier: MIT OR Apache-2.0

//! From the published overlay inputs to the data the renderer draws (P8-f).
//!
//! The render thread adds what only it knows: the interpolation fraction, the
//! frame-rate-bound vignette smoothing, the yaw and pitch of the camera and
//! the atlas layers of the sprites. Everything here is pure.
//!
//! Sources (MCP 1.8): `ItemRenderer.renderOverlays`, `GuiIngame.
//! renderGameOverlay`, `renderVignette`, `func_180474_b` (portal) and
//! `EntityRenderer.renderHand` (where the world overlays are drawn).

use umicraft_mesh::ModelMeshContext;
use umicraft_render::{Portal, ScreenOverlays, Underwater};
use umicraft_world::BlockState;

use crate::overlay_state::OverlayInputs;
use crate::particle_view::block_sprite;

/// Atlas key of the flames drawn over a burning player.
const FIRE_SPRITE: &str = "blocks/fire_layer_1";
/// The portal block, whose sprite is the veil.
const PORTAL_BLOCK: BlockState = BlockState::new(90, 0);
/// Share of the gap the vignette brightness closes per frame
/// (`prevVignetteBrightness += (target - prev) * 0.01`).
const VIGNETTE_EASING: f32 = 0.01;

/// The atlas layers of the sprites the overlays can show; `None` when the
/// terrain has no such sprite (the procedural terrain, a lean resource pack).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpriteLayers {
    /// The sprite of the block the eye is inside.
    pub in_wall: Option<u32>,
    /// The flames of a burning player.
    pub fire: Option<u32>,
    /// The portal veil.
    pub portal: Option<u32>,
}

impl SpriteLayers {
    /// Looks the sprites up in `context`. `in_wall` is the block the eye is
    /// inside; its `particle` texture is the picture
    /// (`BlockModelShapes.getTexture`).
    #[must_use]
    pub fn find(context: Option<&ModelMeshContext>, in_wall: Option<BlockState>) -> Self {
        let Some(context) = context else {
            return Self::default();
        };
        Self {
            in_wall: in_wall.and_then(|state| block_sprite(context, state).map(|s| s.layer)),
            fire: context.atlas.layer(FIRE_SPRITE),
            portal: block_sprite(context, PORTAL_BLOCK).map(|sprite| sprite.layer),
        }
    }
}

/// What the render thread knows about the view this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewFacts {
    /// The eye is under water.
    pub underwater: bool,
    /// The light at the eye, `0.0..=1.0` (`EntityPlayer.getBrightness`).
    pub brightness: f32,
    /// The yaw in degrees, Minecraft's convention.
    pub yaw_degrees: f32,
    /// The pitch in degrees, positive looking down.
    pub pitch_degrees: f32,
    /// How far into the tick the frame is, `0.0..=1.0`.
    pub partial_tick: f32,
}

/// The vignette brightness, which eases towards the darkness around the
/// player once per drawn frame, as `GuiIngame.prevVignetteBrightness` does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VignetteShade {
    brightness: f32,
}

impl Default for VignetteShade {
    /// Starts fully bright, like the reference's `prevVignetteBrightness`.
    fn default() -> Self {
        Self { brightness: 1.0 }
    }
}

impl VignetteShade {
    /// Eases one frame towards the darkness at the eye (`1 - light`) and
    /// returns the colour factor: red-blue near the border, grey otherwise.
    pub fn step(&mut self, light: f32, border_danger: f32) -> [f32; 3] {
        let target = (1.0 - light).clamp(0.0, 1.0);
        self.brightness += (target - self.brightness) * VIGNETTE_EASING;
        if border_danger > 0.0 {
            [0.0, border_danger, border_danger]
        } else {
            [self.brightness; 3]
        }
    }
}

/// The opacity of the portal veil for a timer value above zero
/// (`func_180474_b`): the veil is faint at first and solid at the end.
#[must_use]
pub fn portal_alpha(time: f32) -> f32 {
    if time < 1.0 {
        let squared = time * time;
        squared * squared * 0.8 + 0.2
    } else {
        time
    }
}

/// Decides which overlays show and with what.
///
/// `vignette` is `Some` only on fancy graphics, where the reference draws it.
#[must_use]
pub fn compose(
    inputs: &OverlayInputs,
    view: &ViewFacts,
    layers: SpriteLayers,
    vignette: Option<[f32; 3]>,
) -> ScreenOverlays {
    // The eye overlays are part of the first-person render; a spectator
    // sees neither water nor fire (`!isSpectator`).
    let eyes = inputs.first_person;
    let player = eyes && !inputs.spectator;
    let portal_time = inputs.portal.previous
        + (inputs.portal.current - inputs.portal.previous) * view.partial_tick;
    ScreenOverlays {
        in_wall: layers.in_wall.filter(|_| eyes && inputs.in_wall.is_some()),
        underwater: (player && view.underwater).then_some(Underwater {
            brightness: view.brightness,
            yaw_degrees: view.yaw_degrees,
            pitch_degrees: view.pitch_degrees,
        }),
        fire: layers.fire.filter(|_| player && inputs.on_fire),
        pumpkin: eyes && inputs.pumpkin,
        portal: layers
            .portal
            .filter(|_| !inputs.nauseous && portal_time > 0.0)
            .map(|layer| Portal {
                layer,
                alpha: portal_alpha(portal_time),
            }),
        vignette,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay_state::PortalTime;

    fn view() -> ViewFacts {
        ViewFacts {
            underwater: true,
            brightness: 0.7,
            yaw_degrees: 30.0,
            pitch_degrees: -10.0,
            partial_tick: 0.5,
        }
    }

    fn everything() -> OverlayInputs {
        OverlayInputs {
            first_person: true,
            in_wall: Some(BlockState::new(1, 0)),
            on_fire: true,
            pumpkin: true,
            portal: PortalTime {
                previous: 0.5,
                current: 0.7,
            },
            ..OverlayInputs::default()
        }
    }

    const LAYERS: SpriteLayers = SpriteLayers {
        in_wall: Some(1),
        fire: Some(2),
        portal: Some(3),
    };

    #[test]
    fn a_player_in_first_person_sees_everything() {
        let overlays = compose(&everything(), &view(), LAYERS, Some([0.2; 3]));
        assert_eq!(overlays.in_wall, Some(1));
        assert_eq!(overlays.fire, Some(2));
        assert!(overlays.pumpkin);
        assert_eq!(overlays.vignette, Some([0.2; 3]));
        let water = overlays.underwater.expect("under water");
        assert_eq!(
            (water.brightness, water.yaw_degrees, water.pitch_degrees),
            (0.7, 30.0, -10.0)
        );
        // The timer is halfway between 0.5 and 0.7.
        let portal = overlays.portal.expect("portal");
        assert_eq!(portal.layer, 3);
        assert!((portal.alpha - portal_alpha(0.6)).abs() < 1e-6);
    }

    #[test]
    fn the_third_person_keeps_only_the_portal_and_the_vignette() {
        let inputs = OverlayInputs {
            first_person: false,
            ..everything()
        };
        let overlays = compose(&inputs, &view(), LAYERS, Some([0.2; 3]));
        assert_eq!(overlays.in_wall, None);
        assert_eq!(overlays.underwater, None);
        assert_eq!(overlays.fire, None);
        assert!(!overlays.pumpkin);
        assert!(overlays.portal.is_some() && overlays.vignette.is_some());
    }

    #[test]
    fn a_spectator_sees_neither_water_nor_fire_but_still_the_pumpkin() {
        let inputs = OverlayInputs {
            spectator: true,
            ..everything()
        };
        let overlays = compose(&inputs, &view(), LAYERS, None);
        assert_eq!(overlays.underwater, None);
        assert_eq!(overlays.fire, None);
        assert!(overlays.pumpkin);
        assert_eq!(overlays.vignette, None);
    }

    #[test]
    fn no_wall_block_means_no_wall_sprite() {
        let inputs = OverlayInputs {
            in_wall: None,
            ..everything()
        };
        assert_eq!(compose(&inputs, &view(), LAYERS, None).in_wall, None);
    }

    #[test]
    fn the_portal_needs_time_and_no_nausea() {
        let idle = OverlayInputs {
            portal: PortalTime::default(),
            ..everything()
        };
        assert_eq!(compose(&idle, &view(), LAYERS, None).portal, None);
        let nauseous = OverlayInputs {
            nauseous: true,
            ..everything()
        };
        assert_eq!(compose(&nauseous, &view(), LAYERS, None).portal, None);
    }

    #[test]
    fn a_missing_sprite_leaves_its_overlay_out() {
        let overlays = compose(&everything(), &view(), SpriteLayers::default(), None);
        assert_eq!(overlays.in_wall, None);
        assert_eq!(overlays.fire, None);
        assert_eq!(overlays.portal, None);
        assert!(overlays.underwater.is_some() && overlays.pumpkin);
    }

    #[test]
    fn the_portal_veil_starts_faint_and_ends_solid() {
        assert_eq!(portal_alpha(0.0), 0.2);
        assert!((portal_alpha(0.5) - (0.0625 * 0.8 + 0.2)).abs() < 1e-6);
        assert_eq!(portal_alpha(1.0), 1.0);
    }

    #[test]
    fn the_vignette_eases_towards_the_dark_by_one_percent_a_frame() {
        let mut shade = VignetteShade::default();
        // In the dark (light 0) the target is 1: no change from the start.
        assert_eq!(shade.step(0.0, 0.0), [1.0; 3]);
        // In full light the target is 0.
        let [first, ..] = shade.step(1.0, 0.0);
        assert!((first - 0.99).abs() < 1e-6);
        let [second, ..] = shade.step(1.0, 0.0);
        assert!((second - 0.99 * 0.99).abs() < 1e-6);
    }

    #[test]
    fn near_the_border_the_vignette_turns_red_blue_and_keeps_easing() {
        let mut shade = VignetteShade::default();
        assert_eq!(shade.step(1.0, 0.4), [0.0, 0.4, 0.4]);
        // The brightness still moved while the border colour showed.
        let [grey, ..] = shade.step(1.0, 0.0);
        assert!((grey - 0.99 * 0.99).abs() < 1e-6);
    }
}

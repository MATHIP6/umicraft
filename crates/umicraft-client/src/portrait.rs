// SPDX-License-Identifier: MIT OR Apache-2.0

//! The player model of the inventory screens, after
//! `GuiInventory.drawEntityOnScreen`.
//!
//! The model is drawn into the portrait block of the icon atlas
//! ([`umicraft_render::PORTRAIT_KEY`]) and shown as a 64x64 GUI pixel item
//! icon. This module holds the maths: how the mouse turns the body and head,
//! and the matrix that carries the model (feet at the origin, y up) into the
//! 16x16 box of the icon camera.

use glam::{Mat4, Vec3};
use umicraft_game::entity::EntityRenderSnapshot;
use umicraft_ui::Rect;
use umicraft_world::LightLevel;

/// Side of the portrait in GUI pixels (four 16 px icons wide).
const BLOCK: f32 = 64.0;
/// Where the feet stand in the block, from its top. The tallest pose (the
/// head tilted by the mouse) stays inside the block: 1.8 blocks at scale 30
/// is 54 pixels, leaving 4 above the head.
const FEET_Y: f32 = 58.0;
/// Pixels of the mouse offset that count as one unit in the `atan` of the
/// turn (`mouseX / 40`).
const MOUSE_SPAN: f32 = 40.0;

/// Where the model stands in a screen and how big it is, in the GUI pixels of
/// that screen (the `posX`, `posY` and `scale` of `drawEntityOnScreen`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Placement {
    /// Horizontal centre of the feet.
    pub x: f32,
    /// Vertical position of the feet.
    pub y: f32,
    /// Pixels per block.
    pub scale: f32,
    /// How far above the feet the pointer offset is measured: the reference
    /// compares the pointer with a point `posY - head` (50 in the survival
    /// inventory, 30 in the creative one).
    pub head: f32,
}

/// The survival inventory (`GuiInventory.drawScreen`: 51, 75, 30).
pub(crate) const INVENTORY: Placement = Placement {
    x: 51.0,
    y: 75.0,
    scale: 30.0,
    head: 50.0,
};

/// The Inventory tab of the creative screen (`GuiContainerCreative`: 43, 45, 20).
pub(crate) const CREATIVE: Placement = Placement {
    x: 43.0,
    y: 45.0,
    scale: 20.0,
    head: 30.0,
};

/// Where a screen put the portrait in the last frame, for the render step that
/// draws the model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Spot {
    /// Top-left of the GUI in physical pixels.
    pub origin: (f32, f32),
    /// The GUI scale.
    pub gui_scale: u32,
    /// Where the model stands.
    pub placement: Placement,
}

impl Spot {
    /// The portrait block as an item icon rectangle.
    pub(crate) fn rect(&self) -> Rect {
        self.placement.rect(self.origin, self.gui_scale)
    }

    /// `(model - pointer)` in GUI pixels, the vertical term measured from the
    /// head height of the placement (`j + 75 - 50 - mouseY`).
    pub(crate) fn mouse_offset(&self, pointer: (f32, f32)) -> (f32, f32) {
        let s = self.gui_scale as f32;
        (
            (self.origin.0 + self.placement.x * s - pointer.0) / s,
            (self.origin.1 + (self.placement.y - self.placement.head) * s - pointer.1) / s,
        )
    }
}

impl Placement {
    /// The screen rectangle of the portrait block for a GUI whose top-left
    /// corner is `origin` (physical pixels) at GUI scale `gui_scale`.
    pub(crate) fn rect(self, origin: (f32, f32), gui_scale: u32) -> Rect {
        let s = gui_scale as f32;
        Rect::new(
            origin.0 + (self.x - BLOCK / 2.0) * s,
            origin.1 + (self.y - FEET_Y) * s,
            BLOCK * s,
            BLOCK * s,
        )
    }

    /// Takes a vertex of the model at the world origin to the 16x16 box of
    /// the icon camera. `mouse` is the offset `(placement - pointer)` in GUI
    /// pixels; the whole body tilts with the vertical offset like the
    /// reference (`rotate(-atan(mouseY / 40) * 20, 1, 0, 0)`).
    pub(crate) fn matrix(self, mouse: (f32, f32)) -> Mat4 {
        let tilt = -(mouse.1 / MOUSE_SPAN).atan() * 20.0;
        // GUI y points down, the model's up.
        let fit = Mat4::from_translation(Vec3::new(BLOCK / 2.0, FEET_Y, 0.0))
            * Mat4::from_scale(Vec3::new(self.scale, -self.scale, self.scale))
            * Mat4::from_rotation_x(tilt.to_radians());
        Mat4::from_scale(Vec3::splat(16.0 / BLOCK)) * fit
    }
}

/// `body` standing at the origin and turned towards the pointer: the torso by
/// `atan(dx / 40) * 20` degrees, the head and yaw by twice that, the pitch by
/// `-atan(dy / 40) * 20`.
pub(crate) fn turned_towards(
    mut body: EntityRenderSnapshot,
    mouse: (f32, f32),
) -> EntityRenderSnapshot {
    // The reference multiplies the radian `atan` by 20 and 40 and feeds the
    // result to degree fields.
    let turn = (mouse.0 / MOUSE_SPAN).atan();
    body.position = Vec3::ZERO;
    body.body_yaw = turn * 20.0;
    body.yaw = turn * 40.0;
    body.head_yaw = body.yaw;
    body.pitch = -(mouse.1 / MOUSE_SPAN).atan() * 20.0;
    body.light = LightLevel::FULL_SKY;
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::{EntityId, EntityKind};

    fn body() -> EntityRenderSnapshot {
        EntityRenderSnapshot::new(EntityId(1), EntityKind::Player, Vec3::new(5.0, 64.0, 5.0))
    }

    #[test]
    fn the_mouse_offset_is_zero_at_the_head() {
        let spot = Spot {
            origin: (100.0, 50.0),
            gui_scale: 2,
            placement: INVENTORY,
        };
        let head = (100.0 + 51.0 * 2.0, 50.0 + 25.0 * 2.0);
        assert_eq!(spot.mouse_offset(head), (0.0, 0.0));
        assert_eq!(spot.mouse_offset((head.0 + 20.0, head.1)), (-10.0, 0.0));
        assert_eq!(spot.rect(), INVENTORY.rect((100.0, 50.0), 2));
    }

    #[test]
    fn the_block_is_centred_on_the_feet() {
        let rect = INVENTORY.rect((100.0, 50.0), 2);
        assert_eq!(
            rect,
            Rect::new(100.0 + 19.0 * 2.0, 50.0 + 17.0 * 2.0, 128.0, 128.0)
        );
    }

    #[test]
    fn feet_land_at_the_bottom_of_the_block_and_the_head_inside_it() {
        let m = INVENTORY.matrix((0.0, 0.0));
        let feet = m.transform_point3(Vec3::ZERO);
        let head = m.transform_point3(Vec3::new(0.0, 1.8, 0.0));
        // The 16x16 box is the block divided by four.
        assert!((feet.x - 8.0).abs() < 1e-4 && (feet.y - 58.0 / 4.0).abs() < 1e-4);
        assert!((head.y - (58.0 - 54.0) / 4.0).abs() < 1e-4);
        assert!(head.y > 0.0);
    }

    #[test]
    fn the_model_faces_the_viewer_and_the_right_hand_is_on_the_left() {
        let m = INVENTORY.matrix((0.0, 0.0));
        // Towards +z (the front of a player of yaw 0) is nearer: larger z.
        let front = m.transform_point3(Vec3::Z);
        assert!(front.z > m.transform_point3(Vec3::ZERO).z);
        // East (+x) is on the viewer's right.
        assert!(m.transform_point3(Vec3::X).x > m.transform_point3(Vec3::ZERO).x);
    }

    #[test]
    fn a_pointer_on_the_right_turns_the_player_to_the_right() {
        // dx = placement - pointer is negative when the pointer is on the right.
        let turned = turned_towards(body(), (-40.0, 0.0));
        assert!(
            turned.yaw < 0.0,
            "yaw 0 is south, negative turns to the east"
        );
        assert_eq!(turned.head_yaw, turned.yaw);
        assert!((turned.yaw - 2.0 * turned.body_yaw).abs() < 1e-4);
        assert_eq!(turned.position, Vec3::ZERO);
    }

    #[test]
    fn a_centred_pointer_leaves_the_player_straight() {
        let turned = turned_towards(body(), (0.0, 0.0));
        assert_eq!((turned.yaw, turned.body_yaw, turned.pitch), (0.0, 0.0, 0.0));
    }

    #[test]
    fn a_pointer_below_looks_down() {
        // dy = (placement - 50) - pointer is negative below the head.
        assert!(turned_towards(body(), (0.0, -40.0)).pitch > 0.0);
    }

    #[test]
    fn the_creative_head_height_is_thirty() {
        let spot = Spot {
            origin: (0.0, 0.0),
            gui_scale: 1,
            placement: CREATIVE,
        };
        assert_eq!(spot.mouse_offset((43.0, 15.0)), (0.0, 0.0));
    }
}

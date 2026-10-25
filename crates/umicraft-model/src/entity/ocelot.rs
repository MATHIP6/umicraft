// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ocelot / Cat box model matching MCP 1.8 `ModelOcelot.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Ocelot / Cat model (64×32 texture sheet) with head, body, 4 legs and jointed tail.
#[derive(Debug, Clone)]
pub struct OcelotModel {
    /// Head part with ears and muzzle.
    pub head: ModelPart,
    /// Torso / body.
    pub body: ModelPart,
    /// Back left leg.
    pub back_left_leg: ModelPart,
    /// Back right leg.
    pub back_right_leg: ModelPart,
    /// Front left leg.
    pub front_left_leg: ModelPart,
    /// Front right leg.
    pub front_right_leg: ModelPart,
    /// Upper tail segment.
    pub tail1: ModelPart,
    /// Lower tail segment.
    pub tail2: ModelPart,
}

impl OcelotModel {
    /// Creates a new ocelot model.
    #[must_use]
    pub fn new() -> Self {
        let tex = [64.0, 32.0];

        // Head with ears and muzzle
        let head = ModelPart::new([0.0, 9.0, -9.0])
            .with_box(ModelBox::new(
                [-2.5, -2.0, -2.5],
                [5.0, 4.0, 5.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.5, -2.0, -4.5],
                [3.0, 2.0, 2.0],
                [0.0, 24.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-2.0, 2.0, -0.5],
                [1.0, 1.0, 2.0],
                [0.0, 10.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [1.0, 2.0, -0.5],
                [1.0, 1.0, 2.0],
                [6.0, 10.0],
                tex,
                0.0,
            ));

        // Body: 4 wide, 6 high, 16 long, centered at Y = 7.0, Z = 0.0
        let body = ModelPart::new([0.0, 7.0, 0.0]).with_box(ModelBox::new(
            [-2.0, -3.0, -8.0],
            [4.0, 6.0, 16.0],
            [20.0, 0.0],
            tex,
            0.0,
        ));

        // Back legs (6 units high)
        let back_left_leg = ModelPart::new([1.1, 6.0, 5.0]).with_box(ModelBox::new(
            [-1.0, -6.0, -1.0],
            [2.0, 6.0, 2.0],
            [8.0, 13.0],
            tex,
            0.0,
        ));

        let back_right_leg = ModelPart::new([-1.1, 6.0, 5.0]).with_box(ModelBox::new(
            [-1.0, -6.0, -1.0],
            [2.0, 6.0, 2.0],
            [8.0, 13.0],
            tex,
            0.0,
        ));

        // Front legs (10 units high)
        let front_left_leg = ModelPart::new([1.2, 10.0, -5.0]).with_box(ModelBox::new(
            [-1.0, -10.0, -1.0],
            [2.0, 10.0, 2.0],
            [40.0, 0.0],
            tex,
            0.0,
        ));

        let front_right_leg = ModelPart::new([-1.2, 10.0, -5.0]).with_box(ModelBox::new(
            [-1.0, -10.0, -1.0],
            [2.0, 10.0, 2.0],
            [40.0, 0.0],
            tex,
            0.0,
        ));

        // Tail 1
        let mut tail1 = ModelPart::new([0.0, 9.0, 8.0]).with_box(ModelBox::new(
            [-0.5, 0.0, 0.0],
            [1.0, 8.0, 1.0],
            [0.0, 15.0],
            tex,
            0.0,
        ));
        tail1.rotation[0] = 0.9;

        // Tail 2
        let tail2 = ModelPart::new([0.0, 4.0, 14.0]).with_box(ModelBox::new(
            [-0.5, 0.0, 0.0],
            [1.0, 8.0, 1.0],
            [4.0, 15.0],
            tex,
            0.0,
        ));

        Self {
            head,
            body,
            back_left_leg,
            back_right_leg,
            front_left_leg,
            front_right_leg,
            tail1,
            tail2,
        }
    }

    /// Sets walk cycle animation angles.
    pub fn set_angles(
        &mut self,
        limb_angle: f32,
        limb_distance: f32,
        head_yaw: f32,
        head_pitch: f32,
    ) {
        self.head.rotation[1] = head_yaw;
        self.head.rotation[0] = head_pitch;

        let swing = (limb_angle * 0.6662).cos() * 1.0 * limb_distance;
        self.back_left_leg.rotation[0] = swing;
        self.back_right_leg.rotation[0] = -swing;
        self.front_left_leg.rotation[0] = -swing;
        self.front_right_leg.rotation[0] = swing;
    }

    /// Emits the ocelot mesh vertices transformed into world units.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let scale = 1.0 / 16.0;
        let root = Mat4::from_scale(Vec3::splat(scale));

        self.head.emit(root, &mut out);
        self.body.emit(root, &mut out);
        self.back_left_leg.emit(root, &mut out);
        self.back_right_leg.emit(root, &mut out);
        self.front_left_leg.emit(root, &mut out);
        self.front_right_leg.emit(root, &mut out);
        self.tail1.emit(root, &mut out);
        self.tail2.emit(root, &mut out);

        out
    }
}

impl Default for OcelotModel {
    fn default() -> Self {
        Self::new()
    }
}

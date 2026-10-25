// SPDX-License-Identifier: MIT OR Apache-2.0

//! Horse box model matching MCP 1.8 `ModelHorse.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Horse model (128×128 texture sheet) with jointed legs, neck, head, ears and tail.
#[derive(Debug, Clone)]
pub struct HorseModel {
    /// Torso / body.
    pub body: ModelPart,
    /// Angled neck with mane.
    pub neck: ModelPart,
    /// Head with muzzle and ears.
    pub head: ModelPart,
    /// Back left leg (thigh, shin, hoof).
    pub back_left_leg: ModelPart,
    /// Back right leg.
    pub back_right_leg: ModelPart,
    /// Front left leg.
    pub front_left_leg: ModelPart,
    /// Front right leg.
    pub front_right_leg: ModelPart,
    /// Tail.
    pub tail: ModelPart,
}

impl HorseModel {
    /// Creates a new horse model.
    #[must_use]
    pub fn new() -> Self {
        let tex = [128.0, 128.0];

        // In Umicraft: ground is at Y = 0.
        // Body: 10 wide, 10 high, 24 long. Pivot at Y = 13.0, Z = 0.0.
        let body = ModelPart::new([0.0, 13.0, 0.0]).with_box(ModelBox::new(
            [-5.0, -5.0, -12.0],
            [10.0, 10.0, 24.0],
            [0.0, 34.0],
            tex,
            0.0,
        ));

        // Neck angled upward and forward (pitch ~ 30 deg = FRAC_PI_6)
        let mut neck = ModelPart::new([0.0, 16.0, -9.0]).with_box(ModelBox::new(
            [-2.05, 0.0, -4.0],
            [4.0, 14.0, 8.0],
            [0.0, 12.0],
            tex,
            0.0,
        ));
        neck.rotation[0] = -std::f32::consts::FRAC_PI_6;

        // Head attached near the top of the neck
        let mut head = ModelPart::new([0.0, 26.0, -15.0])
            .with_box(ModelBox::new(
                [-2.5, 0.0, -3.5],
                [5.0, 5.0, 7.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-2.0, 0.0, -9.5],
                [4.0, 3.0, 6.0],
                [24.0, 18.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-2.0, 0.0, -8.5],
                [4.0, 2.0, 5.0],
                [24.0, 27.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [0.5, 5.0, 1.5],
                [2.0, 3.0, 1.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-2.5, 5.0, 1.5],
                [2.0, 3.0, 1.0],
                [0.0, 0.0],
                tex,
                0.0,
            ));
        head.rotation[0] = -std::f32::consts::FRAC_PI_6;

        // Back left leg
        let back_left_leg = ModelPart::new([4.0, 15.0, 9.0])
            .with_box(ModelBox::new(
                [-2.5, -7.0, -2.5],
                [4.0, 7.0, 5.0],
                [78.0, 29.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-2.0, -12.0, -1.5],
                [3.0, 5.0, 3.0],
                [78.0, 43.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-2.5, -15.0, -2.0],
                [4.0, 3.0, 4.0],
                [78.0, 51.0],
                tex,
                0.0,
            ));

        // Back right leg
        let back_right_leg = ModelPart::new([-4.0, 15.0, 9.0])
            .with_box(ModelBox::new(
                [-1.5, -7.0, -2.5],
                [4.0, 7.0, 5.0],
                [96.0, 29.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.0, -12.0, -1.5],
                [3.0, 5.0, 3.0],
                [96.0, 43.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.5, -15.0, -2.0],
                [4.0, 3.0, 4.0],
                [96.0, 51.0],
                tex,
                0.0,
            ));

        // Front left leg
        let front_left_leg = ModelPart::new([4.0, 15.0, -8.0])
            .with_box(ModelBox::new(
                [-1.9, -7.0, -2.1],
                [3.0, 7.0, 4.0],
                [44.0, 29.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.9, -12.0, -1.6],
                [3.0, 5.0, 3.0],
                [44.0, 41.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-2.4, -15.0, -2.1],
                [4.0, 3.0, 4.0],
                [44.0, 51.0],
                tex,
                0.0,
            ));

        // Front right leg
        let front_right_leg = ModelPart::new([-4.0, 15.0, -8.0])
            .with_box(ModelBox::new(
                [-1.1, -7.0, -2.1],
                [3.0, 7.0, 4.0],
                [60.0, 29.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.1, -12.0, -1.6],
                [3.0, 5.0, 3.0],
                [60.0, 41.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.6, -15.0, -2.1],
                [4.0, 3.0, 4.0],
                [60.0, 51.0],
                tex,
                0.0,
            ));

        // Tail
        let mut tail = ModelPart::new([0.0, 16.0, 12.0])
            .with_box(ModelBox::new(
                [-1.0, -2.0, 0.0],
                [2.0, 2.0, 3.0],
                [44.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.5, -6.0, 3.0],
                [3.0, 4.0, 7.0],
                [38.0, 7.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.5, -10.0, 7.0],
                [3.0, 4.0, 7.0],
                [24.0, 3.0],
                tex,
                0.0,
            ));
        tail.rotation[0] = 0.4;

        Self {
            body,
            neck,
            head,
            back_left_leg,
            back_right_leg,
            front_left_leg,
            front_right_leg,
            tail,
        }
    }

    /// Sets galloping/walking leg swing and head animation angles.
    pub fn set_angles(
        &mut self,
        limb_angle: f32,
        limb_distance: f32,
        head_yaw: f32,
        head_pitch: f32,
    ) {
        self.head.rotation[1] = head_yaw;
        self.head.rotation[0] = -std::f32::consts::FRAC_PI_6 + head_pitch;

        let swing = (limb_angle * 0.6662).cos() * 0.8 * limb_distance;
        self.back_left_leg.rotation[0] = swing;
        self.back_right_leg.rotation[0] = -swing;
        self.front_left_leg.rotation[0] = -swing;
        self.front_right_leg.rotation[0] = swing;
    }

    /// Emits the horse mesh vertices transformed into world units.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let scale = 1.0 / 16.0;
        let root = Mat4::from_scale(Vec3::splat(scale));

        self.body.emit(root, &mut out);
        self.neck.emit(root, &mut out);
        self.head.emit(root, &mut out);
        self.back_left_leg.emit(root, &mut out);
        self.back_right_leg.emit(root, &mut out);
        self.front_left_leg.emit(root, &mut out);
        self.front_right_leg.emit(root, &mut out);
        self.tail.emit(root, &mut out);

        out
    }
}

impl Default for HorseModel {
    fn default() -> Self {
        Self::new()
    }
}

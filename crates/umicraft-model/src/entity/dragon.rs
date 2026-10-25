// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ender Dragon box model matching MCP 1.8 `ModelDragon.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Ender Dragon boss model (256×256 texture sheet) with head, jaw, massive body, wings, legs and tail.
#[derive(Debug, Clone)]
pub struct DragonModel {
    /// Head part with horns and nostrils.
    pub head: ModelPart,
    /// Articulated lower jaw.
    pub jaw: ModelPart,
    /// Massive 24×24×64 body torso with dorsal spines.
    pub body: ModelPart,
    /// Left inner wing bone and membrane.
    pub left_wing: ModelPart,
    /// Left outer wing tip.
    pub left_wing_tip: ModelPart,
    /// Right inner wing bone and membrane.
    pub right_wing: ModelPart,
    /// Right outer wing tip.
    pub right_wing_tip: ModelPart,
    /// Left hind leg.
    pub rear_left_leg: ModelPart,
    /// Right hind leg.
    pub rear_right_leg: ModelPart,
    /// Left front leg.
    pub front_left_leg: ModelPart,
    /// Right front leg.
    pub front_right_leg: ModelPart,
    /// Tail segments.
    pub tail: Vec<ModelPart>,
}

impl DragonModel {
    /// Creates a new dragon model.
    #[must_use]
    pub fn new() -> Self {
        let tex = [256.0, 256.0];

        // Head centered at Z = -24.0, Y = 16.0
        let head = ModelPart::new([0.0, 16.0, -24.0])
            .with_box(ModelBox::new(
                [-8.0, 0.0, -8.0],
                [16.0, 16.0, 16.0],
                [112.0, 30.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-6.0, 0.0, -24.0],
                [12.0, 5.0, 16.0],
                [176.0, 44.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-5.0, 16.0, -2.0],
                [2.0, 4.0, 6.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [3.0, 16.0, -2.0],
                [2.0, 4.0, 6.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-5.0, 5.0, -22.0],
                [2.0, 2.0, 4.0],
                [112.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [3.0, 5.0, -22.0],
                [2.0, 2.0, 4.0],
                [112.0, 0.0],
                tex,
                0.0,
            ));

        // Lower jaw (12×4×16)
        let jaw = ModelPart::new([0.0, 12.0, -24.0]).with_box(ModelBox::new(
            [-6.0, -4.0, -24.0],
            [12.0, 4.0, 16.0],
            [176.0, 65.0],
            tex,
            0.0,
        ));

        // Body (24×24×64)
        let body = ModelPart::new([0.0, 16.0, 8.0])
            .with_box(ModelBox::new(
                [-12.0, -12.0, -32.0],
                [24.0, 24.0, 64.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.0, 12.0, -20.0],
                [2.0, 6.0, 12.0],
                [220.0, 53.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.0, 12.0, 0.0],
                [2.0, 6.0, 12.0],
                [220.0, 53.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.0, 12.0, 20.0],
                [2.0, 6.0, 12.0],
                [220.0, 53.0],
                tex,
                0.0,
            ));

        // Left Wing
        let left_wing = ModelPart::new([12.0, 20.0, 2.0])
            .with_box(ModelBox::new(
                [0.0, -4.0, -4.0],
                [56.0, 8.0, 8.0],
                [112.0, 88.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [0.0, -1.0, 2.0],
                [56.0, 2.0, 48.0],
                [0.0, 88.0],
                tex,
                0.0,
            ));

        let left_wing_tip = ModelPart::new([56.0, 0.0, 0.0])
            .with_box(ModelBox::new(
                [0.0, -2.0, -2.0],
                [56.0, 4.0, 4.0],
                [112.0, 136.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [0.0, -1.0, 2.0],
                [56.0, 2.0, 48.0],
                [0.0, 144.0],
                tex,
                0.0,
            ));

        // Right Wing (mirrored along X)
        let right_wing = ModelPart::new([-12.0, 20.0, 2.0])
            .with_box(ModelBox::new(
                [-56.0, -4.0, -4.0],
                [56.0, 8.0, 8.0],
                [112.0, 88.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-56.0, -1.0, 2.0],
                [56.0, 2.0, 48.0],
                [0.0, 88.0],
                tex,
                0.0,
            ));

        let right_wing_tip = ModelPart::new([-56.0, 0.0, 0.0])
            .with_box(ModelBox::new(
                [-56.0, -2.0, -2.0],
                [56.0, 4.0, 4.0],
                [112.0, 136.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-56.0, -1.0, 2.0],
                [56.0, 2.0, 48.0],
                [0.0, 144.0],
                tex,
                0.0,
            ));

        // Hind Legs
        let rear_left_leg = ModelPart::new([16.0, 16.0, 24.0]).with_box(ModelBox::new(
            [-8.0, -32.0, -8.0],
            [16.0, 32.0, 16.0],
            [0.0, 0.0],
            tex,
            0.0,
        ));

        let rear_right_leg = ModelPart::new([-16.0, 16.0, 24.0]).with_box(ModelBox::new(
            [-8.0, -32.0, -8.0],
            [16.0, 32.0, 16.0],
            [0.0, 0.0],
            tex,
            0.0,
        ));

        // Front Legs
        let front_left_leg = ModelPart::new([14.0, 16.0, -12.0]).with_box(ModelBox::new(
            [-6.0, -24.0, -6.0],
            [12.0, 24.0, 12.0],
            [112.0, 104.0],
            tex,
            0.0,
        ));

        let front_right_leg = ModelPart::new([-14.0, 16.0, -12.0]).with_box(ModelBox::new(
            [-6.0, -24.0, -6.0],
            [12.0, 24.0, 12.0],
            [112.0, 104.0],
            tex,
            0.0,
        ));

        // Tail (5 segments)
        let tail = (0..5)
            .map(|i| {
                let z = 44.0 + (i as f32) * 12.0;
                ModelPart::new([0.0, 16.0, z])
                    .with_box(ModelBox::new(
                        [-5.0, -5.0, -5.0],
                        [10.0, 10.0, 10.0],
                        [192.0, 104.0],
                        tex,
                        0.0,
                    ))
                    .with_box(ModelBox::new(
                        [-1.0, 5.0, -3.0],
                        [2.0, 4.0, 6.0],
                        [48.0, 0.0],
                        tex,
                        0.0,
                    ))
            })
            .collect();

        Self {
            head,
            jaw,
            body,
            left_wing,
            left_wing_tip,
            right_wing,
            right_wing_tip,
            rear_left_leg,
            rear_right_leg,
            front_left_leg,
            front_right_leg,
            tail,
        }
    }

    /// Sets wing flapping and tail undulation animation angles.
    pub fn set_angles(&mut self, anim_time: f32) {
        let flap = (anim_time * 0.8).cos() * 0.35;
        self.left_wing.rotation[2] = -flap;
        self.left_wing_tip.rotation[2] = -flap * 0.5;
        self.right_wing.rotation[2] = flap;
        self.right_wing_tip.rotation[2] = flap * 0.5;

        // Tail swaying
        for (i, seg) in self.tail.iter_mut().enumerate() {
            seg.rotation[1] = (anim_time * 0.5 + i as f32 * 0.4).sin() * 0.15;
        }
    }

    /// Emits the dragon mesh vertices transformed into world units.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let scale = 1.0 / 16.0;
        let root = Mat4::from_scale(Vec3::splat(scale));

        self.head.emit(root, &mut out);
        self.jaw.emit(root, &mut out);
        self.body.emit(root, &mut out);

        let lw_t = self.left_wing.transform(root);
        self.left_wing.emit(root, &mut out);
        self.left_wing_tip.emit(lw_t, &mut out);

        let rw_t = self.right_wing.transform(root);
        self.right_wing.emit(root, &mut out);
        self.right_wing_tip.emit(rw_t, &mut out);

        self.rear_left_leg.emit(root, &mut out);
        self.rear_right_leg.emit(root, &mut out);
        self.front_left_leg.emit(root, &mut out);
        self.front_right_leg.emit(root, &mut out);
        for seg in &self.tail {
            seg.emit(root, &mut out);
        }

        out
    }
}

impl Default for DragonModel {
    fn default() -> Self {
        Self::new()
    }
}

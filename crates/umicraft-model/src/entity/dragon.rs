// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ender Dragon box model matching MCP 1.8 `ModelDragon.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Ender Dragon boss model (256×256 texture sheet) with head, jaw, neck, body, wings, articulated legs and tail.
#[derive(Debug, Clone)]
pub struct DragonModel {
    /// Head part with horns and nostrils.
    pub head: ModelPart,
    /// Articulated lower jaw.
    pub jaw: ModelPart,
    /// Neck segments connecting body to head.
    pub neck: Vec<ModelPart>,
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

        // Head centered at Z = -34.0, Y = 20.0 (MCP ModelDragon.java:71-79)
        let head = ModelPart::new([0.0, 20.0, -34.0])
            .with_box(ModelBox::new(
                [-8.0, -8.0, -10.0],
                [16.0, 16.0, 16.0],
                [112.0, 30.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-6.0, -1.0, -26.0],
                [12.0, 5.0, 16.0],
                [176.0, 44.0],
                tex,
                0.0,
            ))
            // Horns (scales)
            .with_box(ModelBox::new(
                [-5.0, 8.0, -6.0],
                [2.0, 4.0, 6.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [3.0, 8.0, -6.0],
                [2.0, 4.0, 6.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            // Nostrils
            .with_box(ModelBox::new(
                [-5.0, 1.0, -24.0],
                [2.0, 2.0, 4.0],
                [112.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [3.0, 1.0, -24.0],
                [2.0, 2.0, 4.0],
                [112.0, 0.0],
                tex,
                0.0,
            ));

        // Lower jaw (12×4×16) child of head at [0, -4, -8]
        let jaw = ModelPart::new([0.0, 16.0, -34.0]).with_box(ModelBox::new(
            [-6.0, -4.0, -26.0],
            [12.0, 4.0, 16.0],
            [176.0, 65.0],
            tex,
            0.0,
        ));

        // Neck (5 segments connecting body at Z = -14 to head at Z = -34)
        let neck = (0..5)
            .map(|i| {
                let z = -14.0 - (i as f32) * 4.0;
                ModelPart::new([0.0, 20.0, z])
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

        // Body (24×24×64) centered at Z = 16.0
        let body = ModelPart::new([0.0, 20.0, 16.0])
            .with_box(ModelBox::new(
                [-12.0, -12.0, -32.0],
                [24.0, 24.0, 64.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.0, 12.0, -26.0],
                [2.0, 6.0, 12.0],
                [220.0, 53.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.0, 12.0, -6.0],
                [2.0, 6.0, 12.0],
                [220.0, 53.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.0, 12.0, 14.0],
                [2.0, 6.0, 12.0],
                [220.0, 53.0],
                tex,
                0.0,
            ));

        // Left Wing (MCP wing at [12, 5, 2], bone + skin membrane)
        let left_wing = ModelPart::new([12.0, 21.0, 10.0])
            .with_box(ModelBox::new(
                [0.0, -4.0, -4.0],
                [56.0, 8.0, 8.0],
                [112.0, 88.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [0.0, 0.0, 2.0],
                [56.0, 0.1, 56.0],
                [-56.0, 88.0],
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
                [0.0, 0.0, 2.0],
                [56.0, 0.1, 56.0],
                [-56.0, 144.0],
                tex,
                0.0,
            ));

        // Right Wing (mirrored along X)
        let right_wing = ModelPart::new([-12.0, 21.0, 10.0])
            .with_box(ModelBox::new(
                [-56.0, -4.0, -4.0],
                [56.0, 8.0, 8.0],
                [112.0, 88.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-56.0, 0.0, 2.0],
                [56.0, 0.1, 56.0],
                [-56.0, 88.0],
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
                [-56.0, 0.0, 2.0],
                [56.0, 0.1, 56.0],
                [-56.0, 144.0],
                tex,
                0.0,
            ));

        // Front Legs (thigh, shin, foot)
        let front_left_leg = ModelPart::new([12.0, 16.0, 2.0])
            .with_box(ModelBox::new(
                [-4.0, -20.0, -4.0],
                [8.0, 20.0, 8.0],
                [112.0, 104.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-3.0, -38.0, -3.0],
                [6.0, 20.0, 6.0],
                [226.0, 138.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-4.0, -42.0, -12.0],
                [8.0, 4.0, 16.0],
                [144.0, 104.0],
                tex,
                0.0,
            ));

        let front_right_leg = ModelPart::new([-12.0, 16.0, 2.0])
            .with_box(ModelBox::new(
                [-4.0, -20.0, -4.0],
                [8.0, 20.0, 8.0],
                [112.0, 104.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-3.0, -38.0, -3.0],
                [6.0, 20.0, 6.0],
                [226.0, 138.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-4.0, -42.0, -12.0],
                [8.0, 4.0, 16.0],
                [144.0, 104.0],
                tex,
                0.0,
            ));

        // Hind Legs (thigh, shin, foot)
        let rear_left_leg = ModelPart::new([16.0, 16.0, 36.0])
            .with_box(ModelBox::new(
                [-8.0, -24.0, -8.0],
                [16.0, 24.0, 16.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-6.0, -42.0, -6.0],
                [12.0, 20.0, 12.0],
                [196.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-9.0, -46.0, -18.0],
                [18.0, 6.0, 24.0],
                [112.0, 0.0],
                tex,
                0.0,
            ));

        let rear_right_leg = ModelPart::new([-16.0, 16.0, 36.0])
            .with_box(ModelBox::new(
                [-8.0, -24.0, -8.0],
                [16.0, 24.0, 16.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-6.0, -42.0, -6.0],
                [12.0, 20.0, 12.0],
                [196.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-9.0, -46.0, -18.0],
                [18.0, 6.0, 24.0],
                [112.0, 0.0],
                tex,
                0.0,
            ));

        // Tail (12 segments trailing behind body)
        let tail = (0..12)
            .map(|i| {
                let z = 48.0 + (i as f32) * 9.0;
                ModelPart::new([0.0, 20.0, z])
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
            neck,
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

        // Jaw opening slightly with flapping
        self.jaw.rotation[0] = ((anim_time * 0.8).sin() + 1.0) * 0.1;

        // Tail swaying
        for (i, seg) in self.tail.iter_mut().enumerate() {
            seg.rotation[1] = (anim_time * 0.5 + i as f32 * 0.3).sin() * 0.12;
        }

        // Neck swaying
        for (i, seg) in self.neck.iter_mut().enumerate() {
            seg.rotation[1] = (anim_time * 0.5 - i as f32 * 0.2).sin() * 0.08;
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
        for seg in &self.neck {
            seg.emit(root, &mut out);
        }
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

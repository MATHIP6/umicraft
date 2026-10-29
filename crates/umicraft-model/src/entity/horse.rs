// SPDX-License-Identifier: MIT OR Apache-2.0

//! Horse box model matching MCP 1.8 `ModelHorse.java`.

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Horse model (128×128 texture sheet) with jointed legs, neck, head, ears and tail.
#[derive(Debug, Clone)]
pub struct HorseModel {
    /// Torso / body.
    pub body: ModelPart,
    /// Head, neck, mane, muzzle, jaw, and ears sharing the shoulder pivot (MCP `ModelHorse.java`).
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
    /// Whether the head wears the long, tilted ears of a donkey or a mule
    /// (MCP `muleLeftEar` / `muleRightEar`) instead of the horse ears.
    mule_ears: bool,
}

impl HorseModel {
    /// Creates a new horse model matching MCP `ModelHorse.java`.
    #[must_use]
    pub fn new() -> Self {
        let tex = [128.0, 128.0];

        // In Umicraft: ground is at Y = 0. (Y_umicraft = 24.0 - Y_mcp)
        // Body: MCP pivot (0, 11, 9), box [-5, -8, -19, 10, 10, 24]
        let body = ModelPart::new([0.0, 13.0, 9.0]).with_box(ModelBox::new(
            [-5.0, -2.0, -19.0],
            [10.0, 10.0, 24.0],
            [0.0, 34.0],
            tex,
            0.0,
        ));

        // In MCP ModelHorse.java, head, neck, mane, muzzle, jaw, and ears all share
        // the rotation point (0.0F, 4.0F, -10.0F) and rest pitch 0.5235988F (+30 deg).
        // Y_umicraft = 24 - 4 = 20.0, Z = -10.0.
        let mut head = ModelPart::new([0.0, 20.0, -10.0])
            // Neck (MCP: [-2.05, -9.8, -2.0, 4, 14, 8], UV: [0, 12])
            .with_box(ModelBox::new(
                [-2.05, -4.2, -2.0],
                [4.0, 14.0, 8.0],
                [0.0, 12.0],
                tex,
                0.0,
            ))
            // Mane (MCP: [-1.0, -11.5, 5.0, 2, 16, 4], UV: [58, 0])
            .with_box(ModelBox::new(
                [-1.0, -4.5, 5.0],
                [2.0, 16.0, 4.0],
                [58.0, 0.0],
                tex,
                0.0,
            ))
            // Head (MCP: [-2.5, -10.0, -1.5, 5, 5, 7], UV: [0, 0])
            .with_box(ModelBox::new(
                [-2.5, 5.0, -1.5],
                [5.0, 5.0, 7.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            // Muzzle (MCP: [-2.0, -10.0, -7.0, 4, 3, 6], UV: [24, 18])
            .with_box(ModelBox::new(
                [-2.0, 7.0, -7.0],
                [4.0, 3.0, 6.0],
                [24.0, 18.0],
                tex,
                0.0,
            ))
            // Jaw (MCP: [-2.0, -7.0, -6.5, 4, 2, 5], UV: [24, 27])
            .with_box(ModelBox::new(
                [-2.0, 5.0, -6.5],
                [4.0, 2.0, 5.0],
                [24.0, 27.0],
                tex,
                0.0,
            ))
            // Left ear (MCP: [0.45, -12.0, 4.0, 2, 3, 1], UV: [0, 0])
            .with_box(ModelBox::new(
                [0.45, 9.0, 4.0],
                [2.0, 3.0, 1.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            // Right ear (MCP: [-2.45, -12.0, 4.0, 2, 3, 1], UV: [0, 0])
            .with_box(ModelBox::new(
                [-2.45, 9.0, 4.0],
                [2.0, 3.0, 1.0],
                [0.0, 0.0],
                tex,
                0.0,
            ));
        head.rotation[0] = -std::f32::consts::FRAC_PI_6;

        // Back left leg (MCP: pivot (4, 9, 11), thigh + shin + hoof)
        let back_left_leg = ModelPart::new([4.0, 15.0, 11.0])
            .with_box(ModelBox::new(
                [-2.5, -7.0, -2.5],
                [4.0, 9.0, 5.0],
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
                [-2.5, -15.1, -2.0],
                [4.0, 3.0, 4.0],
                [78.0, 51.0],
                tex,
                0.0,
            ));

        // Back right leg
        let back_right_leg = ModelPart::new([-4.0, 15.0, 11.0])
            .with_box(ModelBox::new(
                [-1.5, -7.0, -2.5],
                [4.0, 9.0, 5.0],
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
                [-1.5, -15.1, -2.0],
                [4.0, 3.0, 4.0],
                [96.0, 51.0],
                tex,
                0.0,
            ));

        // Front left leg (MCP: pivot (4, 9, -8))
        let front_left_leg = ModelPart::new([4.0, 15.0, -8.0])
            .with_box(ModelBox::new(
                [-1.9, -7.0, -2.1],
                [3.0, 8.0, 4.0],
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
                [-2.4, -15.1, -2.1],
                [4.0, 3.0, 4.0],
                [44.0, 51.0],
                tex,
                0.0,
            ));

        // Front right leg
        let front_right_leg = ModelPart::new([-4.0, 15.0, -8.0])
            .with_box(ModelBox::new(
                [-1.1, -7.0, -2.1],
                [3.0, 8.0, 4.0],
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
                [-1.6, -15.1, -2.1],
                [4.0, 3.0, 4.0],
                [60.0, 51.0],
                tex,
                0.0,
            ));

        // Tail (MCP: pivot (0, 3, 14), rot -1.134464 rad)
        let mut tail = ModelPart::new([0.0, 21.0, 14.0])
            .with_box(ModelBox::new(
                [-1.0, -1.0, 0.0],
                [2.0, 2.0, 3.0],
                [44.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.5, -2.0, 3.0],
                [3.0, 4.0, 7.0],
                [38.0, 7.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.5, 0.5, 9.0],
                [3.0, 4.0, 7.0],
                [24.0, 3.0],
                tex,
                0.0,
            ));
        tail.rotation[0] = 1.134464;

        Self {
            body,
            head,
            back_left_leg,
            back_right_leg,
            front_left_leg,
            front_right_leg,
            tail,
            mule_ears: false,
        }
    }

    /// The model of a donkey or a mule: a horse with long ears.
    #[must_use]
    pub fn mule() -> Self {
        let mut model = Self::new();
        // The two horse ears are the last boxes of the head.
        let kept = model.head.boxes.len() - 2;
        model.head.boxes.truncate(kept);
        model.mule_ears = true;
        model
    }

    /// The two long ears, which tilt outwards and follow the head (MCP
    /// `ModelHorse` `muleLeftEar`, `muleRightEar`: a box of 2 x 7 x 1 and a
    /// roll of 15 degrees, negated here with the y axis).
    fn mule_ear_parts(&self) -> [ModelPart; 2] {
        let ear = |x: f32, roll: f32| {
            let mut part = ModelPart::new(self.head.pivot).with_box(ModelBox::new(
                [x, 9.0, 4.0],
                [2.0, 7.0, 1.0],
                [0.0, 12.0],
                [128.0, 128.0],
                0.0,
            ));
            part.rotation = [self.head.rotation[0], self.head.rotation[1], roll];
            part
        };
        [ear(-2.0, -0.261_799_4), ear(0.0, 0.261_799_4)]
    }

    /// Sets galloping/walking leg swing and head animation angles.
    pub fn set_angles(
        &mut self,
        limb_angle: f32,
        limb_distance: f32,
        head_yaw: f32,
        head_pitch: f32,
    ) {
        self.head.rotation[1] = head_yaw.to_radians();
        self.head.rotation[0] = -std::f32::consts::FRAC_PI_6 - head_pitch.to_radians();

        let swing = (limb_angle * 0.6662).cos() * 0.8 * limb_distance;
        self.back_left_leg.rotation[0] = swing;
        self.back_right_leg.rotation[0] = -swing;
        self.front_left_leg.rotation[0] = -swing;
        self.front_right_leg.rotation[0] = swing;
    }

    /// The saddle, bridle and chests, in the resting pose (MCP `ModelHorse`
    /// `horseSaddle*`, `horseFaceRopes`, `horse*FaceMetal`, `mule*Chest`). The
    /// bridle follows the head; the saddle ropes do not swing and the reins
    /// (drawn only for a ridden horse) are left out. Adults only: the caller
    /// skips a foal.
    #[must_use]
    pub fn emit_gear(&self, saddled: bool, chested: bool) -> Vec<ModelVertex> {
        let tex = [128.0, 128.0];
        let part = |pivot: [f32; 3], origin: [f32; 3], size: [f32; 3], uv: [f32; 2], grow: f32| {
            ModelPart::new(pivot).with_box(ModelBox::new(origin, size, uv, tex, grow))
        };
        let mut out = Vec::new();
        let root = Mat4::from_scale(Vec3::splat(1.0 / 16.0));
        if saddled {
            // The vanilla y axis points down: a box at `y` with height `h`
            // starts at `-(y + h)` here, and the pivot is `24 - y`.
            let mut bridle = [
                part(
                    [0.0, 20.0, -10.0],
                    [-2.5, 5.1, -7.0],
                    [5.0, 5.0, 12.0],
                    [80.0, 12.0],
                    0.2,
                ),
                part(
                    [0.0, 20.0, -10.0],
                    [1.5, 6.0, -4.0],
                    [1.0, 2.0, 2.0],
                    [74.0, 13.0],
                    0.0,
                ),
                part(
                    [0.0, 20.0, -10.0],
                    [-2.5, 6.0, -4.0],
                    [1.0, 2.0, 2.0],
                    [74.0, 13.0],
                    0.0,
                ),
            ];
            for piece in &mut bridle {
                piece.rotation = self.head.rotation;
            }
            let saddle = [
                part(
                    [0.0, 22.0, 2.0],
                    [-5.0, -1.0, -3.0],
                    [10.0, 1.0, 8.0],
                    [80.0, 0.0],
                    0.0,
                ),
                part(
                    [0.0, 22.0, 2.0],
                    [-1.5, 0.0, -3.0],
                    [3.0, 1.0, 2.0],
                    [106.0, 9.0],
                    0.0,
                ),
                part(
                    [0.0, 22.0, 2.0],
                    [-4.0, 0.0, 3.0],
                    [8.0, 1.0, 2.0],
                    [80.0, 9.0],
                    0.0,
                ),
                part(
                    [5.0, 21.0, 2.0],
                    [-0.5, -6.0, -0.5],
                    [1.0, 6.0, 1.0],
                    [70.0, 0.0],
                    0.0,
                ),
                part(
                    [5.0, 21.0, 2.0],
                    [-0.5, -8.0, -1.0],
                    [1.0, 2.0, 2.0],
                    [74.0, 0.0],
                    0.0,
                ),
                part(
                    [-5.0, 21.0, 2.0],
                    [-0.5, -6.0, -0.5],
                    [1.0, 6.0, 1.0],
                    [80.0, 0.0],
                    0.0,
                ),
                part(
                    [-5.0, 21.0, 2.0],
                    [-0.5, -8.0, -1.0],
                    [1.0, 2.0, 2.0],
                    [74.0, 4.0],
                    0.0,
                ),
            ];
            for piece in bridle.iter().chain(&saddle) {
                piece.emit(root, &mut out);
            }
        }
        if chested {
            let side = |x: f32, uv: [f32; 2]| {
                let mut chest = part([x, 21.0, 10.0], [-3.0, -8.0, 0.0], [8.0, 8.0, 3.0], uv, 0.0);
                chest.rotation[1] = std::f32::consts::FRAC_PI_2;
                chest
            };
            side(-7.5, [0.0, 34.0]).emit(root, &mut out);
            side(4.5, [0.0, 47.0]).emit(root, &mut out);
        }
        out
    }

    /// Emits the horse mesh vertices transformed into world units.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let scale = 1.0 / 16.0;
        let root = Mat4::from_scale(Vec3::splat(scale));

        self.body.emit(root, &mut out);
        self.head.emit(root, &mut out);
        if self.mule_ears {
            for ear in self.mule_ear_parts() {
                ear.emit(root, &mut out);
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gear_adds_one_box_per_vanilla_part() {
        let model = HorseModel::new();
        assert!(model.emit_gear(false, false).is_empty());
        // Bridle (3), saddle (3), ropes and metal (4): 10 boxes of 36 vertices.
        assert_eq!(model.emit_gear(true, false).len(), 10 * 36);
        assert_eq!(model.emit_gear(false, true).len(), 2 * 36);
        assert_eq!(model.emit_gear(true, true).len(), 12 * 36);
    }

    #[test]
    fn the_bridle_turns_with_the_head_and_the_saddle_does_not() {
        let mut model = HorseModel::new();
        model.set_angles(0.0, 0.0, 0.0, 0.0);
        let straight = model.emit_gear(true, false);
        model.set_angles(0.0, 0.0, 90.0, 0.0);
        let turned = model.emit_gear(true, false);
        // The first three boxes are the bridle, the rest the saddle.
        assert_ne!(straight[..3 * 36], turned[..3 * 36]);
        assert_eq!(straight[3 * 36..], turned[3 * 36..]);
    }

    #[test]
    fn a_mule_swaps_the_horse_ears_for_two_long_ones() {
        let horse = HorseModel::new();
        let mule = HorseModel::mule();
        // Two boxes out, two boxes in: the vertex count stays the same.
        assert_eq!(mule.emit_mesh().len(), horse.emit_mesh().len());
        assert_eq!(mule.head.boxes.len() + 2, horse.head.boxes.len());
        let top = |model: &HorseModel| {
            model
                .emit_mesh()
                .iter()
                .map(|v| v.position[1])
                .fold(f32::MIN, f32::max)
        };
        assert!(top(&mule) > top(&horse), "the long ears reach higher");
    }
}

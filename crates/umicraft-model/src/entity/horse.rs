// SPDX-License-Identifier: MIT OR Apache-2.0

//! Horse box model matching MCP 1.8 `ModelHorse.java`, with its animation
//! (`setLivingAnimations`: walking legs, grazing, rearing, open mouth, wagging
//! tail, saddle and reins) and its foal scaling (`render`).
//!
//! Every part is a sibling with its own pivot, as in the reference. The
//! numbers below are the reference's, in its y-down pixel space; [`part`]
//! turns a pivot, a rotation and a box into the y-up space of this crate
//! (pivot `(x, 24 - y, z)`, rotation `(-x, y, -z)`, box origin `-(y + h)`).

use std::f32::consts::{FRAC_PI_2, FRAC_PI_3, FRAC_PI_6, PI};

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Height of the ground below the origin of the reference, in blocks.
const GROUND: f32 = 1.5;
/// Rest pitch of the head, 30 degrees.
const HEAD_PITCH: f32 = FRAC_PI_6;

/// A box of the reference: origin, size, texture offset and growth.
type Cuboid = ([f32; 3], [f32; 3], [f32; 2], f32);
/// The box of a leg, a shin or a hoof: origin, size and texture offset.
type LegBox = ([f32; 3], [f32; 3], [f32; 2]);

/// What the horse is doing: the inputs of `ModelHorse.setLivingAnimations`
/// and `render`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HorseLook {
    /// Limb swing position (`limbSwing`).
    pub limb_swing: f32,
    /// Limb swing amount (`limbSwingAmount`).
    pub limb_amount: f32,
    /// Head yaw relative to the body, in degrees (limited to 20 either way).
    pub head_yaw: f32,
    /// Pitch of the head, in degrees.
    pub head_pitch: f32,
    /// How far the head is down grazing, `0..=1`.
    pub grazing: f32,
    /// How far the horse stands on its hind legs, `0..=1`.
    pub rearing: f32,
    /// How far the mouth is open, `0..=1`.
    pub mouth: f32,
    /// Whether the tail wags.
    pub tail_swinging: bool,
    /// Ticks the horse has existed, plus the sub-tick fraction.
    pub age: f32,
    /// False for a foal, which is drawn part by part at `size`.
    pub adult: bool,
    /// The size of a foal, `0.5` when born to `1` when grown up.
    pub size: f32,
    /// Whether it wears a saddle (an adult shows it).
    pub saddled: bool,
    /// Whether something sits on it: the reins show.
    pub ridden: bool,
    /// Whether it carries chests (donkeys and mules).
    pub chested: bool,
}

impl Default for HorseLook {
    fn default() -> Self {
        Self {
            limb_swing: 0.0,
            limb_amount: 0.0,
            head_yaw: 0.0,
            head_pitch: 0.0,
            grazing: 0.0,
            rearing: 0.0,
            mouth: 0.0,
            tail_swinging: false,
            age: 0.0,
            adult: true,
            size: 1.0,
            saddled: false,
            ridden: false,
            chested: false,
        }
    }
}

/// Horse model (128x128 texture sheet).
#[derive(Debug, Clone, Default)]
pub struct HorseModel {
    /// Whether the head wears the long, tilted ears of a donkey or a mule
    /// (MCP `muleLeftEar` / `muleRightEar`) instead of the horse ears.
    mule_ears: bool,
    look: HorseLook,
}

/// A part of the reference model: its pivot and rotation (y-down), and its
/// boxes in the reference's space.
fn part(pivot: [f32; 3], rotation: [f32; 3], boxes: &[Cuboid]) -> ModelPart {
    let mut part = ModelPart::new([pivot[0], 24.0 - pivot[1], pivot[2]]);
    part.rotation = [-rotation[0], rotation[1], -rotation[2]];
    for &(origin, size, uv, grow) in boxes {
        part = part.with_box(ModelBox::new(
            [origin[0], -(origin[1] + size[1]), origin[2]],
            size,
            uv,
            [128.0, 128.0],
            grow,
        ));
    }
    part
}

/// Moves and scales vertices the way `GlStateManager.scale` then `translate`
/// do around the origin of the reference, which is `GROUND` above the feet
/// with the y axis down: `p' = scale * (p + offset)`.
fn scaled(vertices: &mut [ModelVertex], scale: [f32; 3], offset: [f32; 3]) {
    for vertex in vertices {
        let [x, y, z] = vertex.position;
        let down = GROUND - y;
        vertex.position = [
            scale[0] * (x + offset[0]),
            GROUND - scale[1] * (down + offset[1]),
            scale[2] * (z + offset[2]),
        ];
    }
}

impl HorseModel {
    /// Creates a new horse model matching MCP `ModelHorse.java`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The model of a donkey or a mule: a horse with long ears.
    #[must_use]
    pub fn mule() -> Self {
        Self {
            mule_ears: true,
            ..Self::default()
        }
    }

    /// Sets the pose to draw.
    pub fn set_look(&mut self, look: HorseLook) {
        self.look = look;
    }

    /// Emits the horse in the pose set, without gear unless the look has it.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        self.emit()
    }

    /// Emits the horse mesh vertices transformed into world units
    /// (`ModelHorse.render` after `setLivingAnimations`).
    #[must_use]
    pub fn emit(&self) -> Vec<ModelVertex> {
        let look = &self.look;
        let root = Mat4::from_scale(Vec3::splat(1.0 / 16.0));
        let rig = Rig::of(look);
        let (adult, size) = (look.adult, look.size);
        let emit_all = |parts: &[ModelPart], out: &mut Vec<ModelVertex>| {
            for part in parts {
                part.emit(root, out);
            }
        };
        let mut out = Vec::new();

        if adult && look.saddled {
            emit_all(&rig.saddle(look.ridden), &mut out);
        }

        let mut legs = Vec::new();
        emit_all(&rig.legs(), &mut legs);
        let mut body = Vec::new();
        emit_all(&rig.body(), &mut body);
        let mut head = Vec::new();
        emit_all(&rig.ears(self.mule_ears), &mut head);
        rig.head().emit_family(root, &mut head);

        if !adult {
            scaled(
                &mut legs,
                [size, 0.5 + size * 0.5, size],
                [0.0, 0.95 * (1.0 - size), 0.0],
            );
            scaled(&mut body, [size; 3], [0.0, 1.35 * (1.0 - size), 0.0]);
            let head_size = 0.5 + size * size * 0.5;
            let grazing = look.grazing;
            let offset = if grazing <= 0.0 {
                [0.0, 1.35 * (1.0 - size), 0.0]
            } else {
                [
                    0.0,
                    0.9 * (1.0 - size) * grazing + 1.35 * (1.0 - size) * (1.0 - grazing),
                    0.15 * (1.0 - size) * grazing,
                ]
            };
            scaled(&mut head, [head_size; 3], offset);
        }
        out.extend(legs);
        out.extend(body);
        out.extend(head);
        if adult && look.chested {
            emit_all(&rig.chests(), &mut out);
        }
        out
    }
}

/// The head and its two children, the muzzle and the jaw, which turn with it.
struct HeadFamily {
    head: ModelPart,
    muzzle: ModelPart,
    jaw: ModelPart,
}

impl HeadFamily {
    fn emit_family(&self, root: Mat4, out: &mut Vec<ModelVertex>) {
        self.head.emit(root, out);
        let head = self.head.transform(root);
        self.muzzle.emit(head, out);
        self.jaw.emit(head, out);
    }
}

/// Pivot (`x`, `y`, `z`) and pitch of a leg and of its shin and hoof.
#[derive(Clone, Copy)]
struct LegRig {
    x: f32,
    leg: [f32; 3],
    shin: [f32; 3],
}

/// Every pivot and rotation `setLivingAnimations` computes, in the reference's
/// y-down space.
struct Rig {
    body_pitch: f32,
    /// Pivot `[y, z]` of the head and of everything that follows it.
    head: [f32; 2],
    /// Pitch and yaw of the head.
    head_angle: [f32; 2],
    /// Pitch of the reins (the pitch of the look, without the rest pitch).
    rein_pitch: f32,
    mouth: f32,
    tail_y: f32,
    /// Pitch of the base and of the middle of the tail, and of its tip.
    tail_pitch: [f32; 2],
    tail_yaw: f32,
    legs: [LegRig; 4],
    chest: [f32; 2],
    chest_left: [f32; 2],
    chest_swing: f32,
    saddle: [f32; 2],
    /// Pitch of the saddle ropes, and the roll of the left and right ones.
    rope_angle: [f32; 3],
}

impl Rig {
    fn of(look: &HorseLook) -> Self {
        let swing = look.limb_swing;
        let amount = look.limb_amount;
        let yaw = look.head_yaw.clamp(-20.0, 20.0).to_radians();
        let mut pitch = look.head_pitch.to_radians();
        if amount > 0.2 {
            pitch += (swing * 0.4).cos() * 0.15 * amount;
        }
        let (grazing, rearing) = (look.grazing, look.rearing);
        let standing = 1.0 - rearing;
        let calm = 1.0 - rearing.max(grazing);
        let step = (swing * 0.6662 + PI).cos();
        let stride = step * 0.8 * amount;

        // Where the head hangs and how it turns.
        let head_x =
            rearing * (0.261_799_4 + pitch) + grazing * 2.18166 + calm * (HEAD_PITCH + pitch);
        let head_yaw = (rearing + calm) * yaw;
        let head_py = rearing * -6.0 + grazing * 11.0 + calm * 4.0;
        let head_pz = -rearing + grazing * -10.0 + calm * -10.0;

        // The legs.
        let hind = 0.261_799_4 * rearing;
        let wave = (look.age * 0.6 + PI).cos();
        let half = step * 0.5 * amount;
        let front_py = -2.0 * rearing + 9.0 * standing;
        let front_pz = -2.0 * rearing + -8.0 * standing;
        let shin_at = |leg_y: f32, leg_z: f32, angle: f32| {
            [
                leg_y + (FRAC_PI_2 + angle).sin() * 7.0,
                leg_z + (3.0 * FRAC_PI_2 + angle).cos() * 7.0,
            ]
        };
        let hind_left = hind - half * standing;
        let hind_right = hind + half * standing;
        let front_left = (-FRAC_PI_3 + wave) * rearing + stride * standing;
        let front_right = (-FRAC_PI_3 - wave) * rearing - stride * standing;
        let [bl_y, bl_z] = shin_at(9.0, 11.0, hind_left);
        let [br_y, br_z] = shin_at(9.0, 11.0, hind_right);
        let [fl_y, fl_z] = shin_at(front_py, front_pz, front_left);
        let [fr_y, fr_z] = shin_at(front_py, front_pz, front_right);
        let bent = -0.087_266_46 * rearing;
        let legs = [
            LegRig {
                x: 4.0,
                leg: [9.0, 11.0, hind_left],
                shin: [bl_y, bl_z, bent + (-half - half.max(0.0)) * standing],
            },
            LegRig {
                x: -4.0,
                leg: [9.0, 11.0, hind_right],
                shin: [br_y, br_z, bent + (half - (-half).max(0.0)) * standing],
            },
            LegRig {
                x: 4.0,
                leg: [front_py, front_pz, front_left],
                shin: [
                    fl_y,
                    fl_z,
                    (front_left + PI * (0.2 + wave * 0.2).max(0.0)) * rearing
                        + (stride + half.max(0.0)) * standing,
                ],
            },
            LegRig {
                x: -4.0,
                leg: [front_py, front_pz, front_right],
                shin: [
                    fr_y,
                    fr_z,
                    (front_right + PI * (0.2 - wave * 0.2).max(0.0)) * rearing
                        + (-stride + (-half).max(0.0)) * standing,
                ],
            },
        ];

        // The tail.
        let mut tail_base = (-1.3089 + amount * 1.5).min(0.0);
        let mut tail_yaw = 0.0;
        if look.tail_swinging {
            tail_yaw = (look.age * 0.7).cos();
            tail_base = 0.0;
        }

        let chest = [
            rearing * 5.5 + standing * 3.0,
            rearing * 15.0 + standing * 10.0,
        ];
        Self {
            body_pitch: rearing * -45.0_f32.to_radians(),
            head: [head_py, head_pz],
            head_angle: [head_x, head_yaw],
            rein_pitch: pitch,
            mouth: look.mouth,
            tail_y: rearing * 9.0 + standing * 3.0,
            tail_pitch: [tail_base, -0.2618 + tail_base],
            tail_yaw,
            legs,
            chest,
            // The left chest follows the right one only while saddled.
            chest_left: if look.saddled { chest } else { [3.0, 10.0] },
            chest_swing: stride / 5.0,
            saddle: [
                rearing * 0.5 + standing * 2.0,
                rearing * 11.0 + standing * 2.0,
            ],
            rope_angle: if look.ridden {
                [-FRAC_PI_3, 0.0, 0.0]
            } else {
                [stride / 3.0, stride / 5.0, -stride / 5.0]
            },
        }
    }

    fn legs(&self) -> Vec<ModelPart> {
        // The leg, shin and hoof boxes of the back left, back right, front
        // left and front right legs: `(origin, size, uv)`.
        const BOXES: [[LegBox; 3]; 4] = [
            [
                ([-2.5, -2.0, -2.5], [4.0, 9.0, 5.0], [78.0, 29.0]),
                ([-2.0, 0.0, -1.5], [3.0, 5.0, 3.0], [78.0, 43.0]),
                ([-2.5, 5.1, -2.0], [4.0, 3.0, 4.0], [78.0, 51.0]),
            ],
            [
                ([-1.5, -2.0, -2.5], [4.0, 9.0, 5.0], [96.0, 29.0]),
                ([-1.0, 0.0, -1.5], [3.0, 5.0, 3.0], [96.0, 43.0]),
                ([-1.5, 5.1, -2.0], [4.0, 3.0, 4.0], [96.0, 51.0]),
            ],
            [
                ([-1.9, -1.0, -2.1], [3.0, 8.0, 4.0], [44.0, 29.0]),
                ([-1.9, 0.0, -1.6], [3.0, 5.0, 3.0], [44.0, 41.0]),
                ([-2.4, 5.1, -2.1], [4.0, 3.0, 4.0], [44.0, 51.0]),
            ],
            [
                ([-1.1, -1.0, -2.1], [3.0, 8.0, 4.0], [60.0, 29.0]),
                ([-1.1, 0.0, -1.6], [3.0, 5.0, 3.0], [60.0, 41.0]),
                ([-1.6, 5.1, -2.1], [4.0, 3.0, 4.0], [60.0, 51.0]),
            ],
        ];
        let one = |(origin, size, uv): LegBox| [(origin, size, uv, 0.0)];
        let mut parts = Vec::new();
        for (leg, [leg_box, shin_box, hoof_box]) in self.legs.iter().zip(BOXES) {
            parts.push(part(
                [leg.x, leg.leg[0], leg.leg[1]],
                [leg.leg[2], 0.0, 0.0],
                &one(leg_box),
            ));
            // The hoof follows the shin: same pivot, same pitch.
            for lower in [shin_box, hoof_box] {
                parts.push(part(
                    [leg.x, leg.shin[0], leg.shin[1]],
                    [leg.shin[2], 0.0, 0.0],
                    &one(lower),
                ));
            }
        }
        parts
    }

    fn body(&self) -> Vec<ModelPart> {
        let tail = |pitch: f32, boxes: &[Cuboid]| {
            part([0.0, self.tail_y, 14.0], [pitch, self.tail_yaw, 0.0], boxes)
        };
        let [base, tip] = self.tail_pitch;
        vec![
            part(
                [0.0, 11.0, 9.0],
                [self.body_pitch, 0.0, 0.0],
                &[([-5.0, -8.0, -19.0], [10.0, 10.0, 24.0], [0.0, 34.0], 0.0)],
            ),
            tail(
                base,
                &[([-1.0, -1.0, 0.0], [2.0, 2.0, 3.0], [44.0, 0.0], 0.0)],
            ),
            tail(
                base,
                &[([-1.5, -2.0, 3.0], [3.0, 4.0, 7.0], [38.0, 7.0], 0.0)],
            ),
            tail(
                tip,
                &[([-1.5, -4.5, 9.0], [3.0, 4.0, 7.0], [24.0, 3.0], 0.0)],
            ),
            self.on_head(
                &[([-2.05, -9.8, -2.0], [4.0, 14.0, 8.0], [0.0, 12.0], 0.0)],
                0.0,
            ),
            self.on_head(
                &[([-1.0, -11.5, 5.0], [2.0, 16.0, 4.0], [58.0, 0.0], 0.0)],
                0.0,
            ),
        ]
    }

    /// A part that sits on the pivot of the head, with its pitch and yaw.
    fn on_head(&self, boxes: &[Cuboid], roll: f32) -> ModelPart {
        part(
            [0.0, self.head[0], self.head[1]],
            [self.head_angle[0], self.head_angle[1], roll],
            boxes,
        )
    }

    fn ears(&self, mule: bool) -> Vec<ModelPart> {
        let ear = |origin: [f32; 3], size: [f32; 3], uv: [f32; 2], roll: f32| {
            self.on_head(&[(origin, size, uv, 0.0)], roll)
        };
        if mule {
            vec![
                ear(
                    [-2.0, -16.0, 4.0],
                    [2.0, 7.0, 1.0],
                    [0.0, 12.0],
                    0.261_799_4,
                ),
                ear(
                    [0.0, -16.0, 4.0],
                    [2.0, 7.0, 1.0],
                    [0.0, 12.0],
                    -0.261_799_4,
                ),
            ]
        } else {
            vec![
                ear([0.45, -12.0, 4.0], [2.0, 3.0, 1.0], [0.0, 0.0], 0.0),
                ear([-2.45, -12.0, 4.0], [2.0, 3.0, 1.0], [0.0, 0.0], 0.0),
            ]
        }
    }

    fn head(&self) -> HeadFamily {
        let open = self.mouth;
        // The muzzle and the jaw hang from the head: their pivots are offsets
        // in its frame (y down in the reference, up here).
        let child = |pivot: [f32; 3], pitch: f32, boxes: &[Cuboid]| {
            let mut child = part([0.0; 3], [pitch, 0.0, 0.0], boxes);
            child.pivot = [pivot[0], -pivot[1], pivot[2]];
            child
        };
        HeadFamily {
            head: self.on_head(
                &[([-2.5, -10.0, -1.5], [5.0, 5.0, 7.0], [0.0, 0.0], 0.0)],
                0.0,
            ),
            muzzle: child(
                [0.0, 0.02, 0.02 - open],
                -0.094_247_78 * open,
                &[([-2.0, -10.0, -7.0], [4.0, 3.0, 6.0], [24.0, 18.0], 0.0)],
            ),
            jaw: child(
                [0.0, 0.0, open],
                0.157_079_64 * open,
                &[([-2.0, -7.0, -6.5], [4.0, 2.0, 5.0], [24.0, 27.0], 0.0)],
            ),
        }
    }

    /// The bridle, the saddle and, when ridden, the reins.
    fn saddle(&self, ridden: bool) -> Vec<ModelPart> {
        let [py, pz] = self.saddle;
        let back = |x: f32, angle: [f32; 3], boxes: &[Cuboid]| part([x, py, pz], angle, boxes);
        let flat = [self.body_pitch, 0.0, 0.0];
        let [rope_pitch, left_roll, right_roll] = self.rope_angle;
        let left = [rope_pitch, 0.0, left_roll];
        let right = [rope_pitch, 0.0, right_roll];
        let mut parts = vec![
            self.on_head(
                &[([-2.5, -10.1, -7.0], [5.0, 5.0, 12.0], [80.0, 12.0], 0.2)],
                0.0,
            ),
            back(
                0.0,
                flat,
                &[([-5.0, 0.0, -3.0], [10.0, 1.0, 8.0], [80.0, 0.0], 0.0)],
            ),
            back(
                0.0,
                flat,
                &[([-1.5, -1.0, -3.0], [3.0, 1.0, 2.0], [106.0, 9.0], 0.0)],
            ),
            back(
                0.0,
                flat,
                &[([-4.0, -1.0, 3.0], [8.0, 1.0, 2.0], [80.0, 9.0], 0.0)],
            ),
            back(
                5.0,
                left,
                &[([-0.5, 0.0, -0.5], [1.0, 6.0, 1.0], [70.0, 0.0], 0.0)],
            ),
            back(
                5.0,
                left,
                &[([-0.5, 6.0, -1.0], [1.0, 2.0, 2.0], [74.0, 0.0], 0.0)],
            ),
            back(
                -5.0,
                right,
                &[([-0.5, 0.0, -0.5], [1.0, 6.0, 1.0], [80.0, 0.0], 0.0)],
            ),
            back(
                -5.0,
                right,
                &[([-0.5, 6.0, -1.0], [1.0, 2.0, 2.0], [74.0, 4.0], 0.0)],
            ),
            self.on_head(
                &[([1.5, -8.0, -4.0], [1.0, 2.0, 2.0], [74.0, 13.0], 0.0)],
                0.0,
            ),
            self.on_head(
                &[([-2.5, -8.0, -4.0], [1.0, 2.0, 2.0], [74.0, 13.0], 0.0)],
                0.0,
            ),
        ];
        if ridden {
            let rein = |x: f32, uv: [f32; 2]| {
                part(
                    [0.0, self.head[0], self.head[1]],
                    [self.rein_pitch, self.head_angle[1], 0.0],
                    &[([x, -6.0, -6.0], [0.0, 3.0, 16.0], uv, 0.0)],
                )
            };
            parts.push(rein(2.6, [44.0, 10.0]));
            parts.push(rein(-2.6, [44.0, 5.0]));
        }
        parts
    }

    fn chests(&self) -> Vec<ModelPart> {
        let chest = |x: f32, at: [f32; 2], swing: f32, uv: [f32; 2]| {
            part(
                [x, at[0], at[1]],
                [swing, FRAC_PI_2, 0.0],
                &[([-3.0, 0.0, 0.0], [8.0, 8.0, 3.0], uv, 0.0)],
            )
        };
        vec![
            chest(-7.5, self.chest_left, self.chest_swing, [0.0, 34.0]),
            chest(4.5, self.chest, -self.chest_swing, [0.0, 47.0]),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mesh(look: HorseLook) -> Vec<ModelVertex> {
        let mut model = HorseModel::new();
        model.set_look(look);
        model.emit()
    }

    fn extent(vertices: &[ModelVertex], axis: usize) -> (f32, f32) {
        vertices
            .iter()
            .fold((f32::MAX, f32::MIN), |(low, high), v| {
                (low.min(v.position[axis]), high.max(v.position[axis]))
            })
    }

    #[test]
    fn a_horse_at_rest_is_23_boxes_and_stands_on_the_ground() {
        let horse = mesh(HorseLook::default());
        // Body, 3 tail, 12 leg, head, muzzle, jaw, neck, mane, 2 ears.
        assert_eq!(horse.len(), 23 * 36);
        let (low, high) = extent(&horse, 1);
        assert!(low.abs() < 0.02, "the hooves reach the ground: {low}");
        // The mane rises over the head, about 2.15 blocks.
        assert!((high - 2.15).abs() < 0.1, "{high}");
    }

    #[test]
    fn the_gear_adds_one_box_per_vanilla_part() {
        let extra = |look: HorseLook| mesh(look).len() / 36 - 23;
        assert_eq!(extra(HorseLook::default()), 0);
        // Face ropes, saddle (3), ropes and metal (4), face metal (2).
        let saddled = HorseLook {
            saddled: true,
            ..HorseLook::default()
        };
        assert_eq!(extra(saddled), 10);
        // The reins show only while ridden.
        assert_eq!(
            extra(HorseLook {
                ridden: true,
                ..saddled
            }),
            12
        );
        let chested = HorseLook {
            chested: true,
            ..HorseLook::default()
        };
        assert_eq!(extra(chested), 2);
        // A foal wears nothing.
        let foal = HorseLook {
            adult: false,
            size: 0.5,
            saddled: true,
            chested: true,
            ..HorseLook::default()
        };
        assert_eq!(extra(foal), 0);
    }

    #[test]
    fn the_bridle_turns_with_the_head_and_the_saddle_does_not() {
        let saddled = |head_yaw: f32| {
            mesh(HorseLook {
                saddled: true,
                head_yaw,
                ..HorseLook::default()
            })
        };
        let (straight, turned) = (saddled(0.0), saddled(15.0));
        let differing = straight
            .chunks(36)
            .zip(turned.chunks(36))
            .filter(|(a, b)| a != b)
            .count();
        // Face ropes and face metal (2), then head, muzzle, jaw, neck, mane
        // and the two ears.
        assert_eq!(differing, 3 + 7);
    }

    #[test]
    fn a_mule_swaps_the_horse_ears_for_two_long_ones() {
        let horse = HorseModel::new();
        let mule = HorseModel::mule();
        assert_eq!(mule.emit_mesh().len(), horse.emit_mesh().len());
        let top = |model: &HorseModel| extent(&model.emit_mesh(), 1).1;
        assert!(top(&mule) > top(&horse), "the long ears reach higher");
    }

    #[test]
    fn walking_swings_the_legs_and_bends_the_shins() {
        let walking = |swing: f32| {
            mesh(HorseLook {
                limb_swing: swing,
                limb_amount: 1.0,
                ..HorseLook::default()
            })
        };
        assert_ne!(walking(0.0), walking(2.0));
        let (low, _) = extent(&walking(2.0), 1);
        assert!(low > -0.05, "a bent leg does not sink in the ground: {low}");
    }

    #[test]
    fn rearing_lifts_the_front_and_tilts_the_body() {
        let rest = mesh(HorseLook::default());
        let rearing = mesh(HorseLook {
            rearing: 1.0,
            ..HorseLook::default()
        });
        let (_, rest_top) = extent(&rest, 1);
        let (_, top) = extent(&rearing, 1);
        assert!(top > rest_top + 0.3, "{top} {rest_top}");
        // The front is lifted: the lowest point of the front half is higher.
        let front_low = |mesh: &[ModelVertex]| {
            mesh.iter()
                .filter(|v| v.position[2] < -0.2)
                .map(|v| v.position[1])
                .fold(f32::MAX, f32::min)
        };
        assert!(front_low(&rearing) > front_low(&rest) + 0.2);
    }

    #[test]
    fn grazing_puts_the_head_down() {
        let rest = mesh(HorseLook::default());
        let grazing = mesh(HorseLook {
            grazing: 1.0,
            ..HorseLook::default()
        });
        let (_, rest_top) = extent(&rest, 1);
        let (_, top) = extent(&grazing, 1);
        assert!(top < rest_top - 0.2, "{top} {rest_top}");
    }

    #[test]
    fn an_open_mouth_drops_the_jaw() {
        let shut = mesh(HorseLook::default());
        let open = mesh(HorseLook {
            mouth: 1.0,
            ..HorseLook::default()
        });
        assert_ne!(shut, open);
    }

    #[test]
    fn a_foal_is_the_horse_shrunk_part_by_part() {
        let adult = mesh(HorseLook::default());
        let foal = mesh(HorseLook {
            adult: false,
            size: 0.5,
            ..HorseLook::default()
        });
        assert_eq!(adult.len(), foal.len());
        let (low, high) = extent(&foal, 1);
        let (_, adult_high) = extent(&adult, 1);
        assert!(low.abs() < 0.02, "a foal stands on the ground: {low}");
        assert!(high < adult_high * 0.8, "{high} {adult_high}");
    }

    #[test]
    fn the_tail_wags_sideways() {
        let wag = |age: f32| {
            mesh(HorseLook {
                tail_swinging: true,
                age,
                ..HorseLook::default()
            })
        };
        assert_ne!(wag(0.0), wag(2.0));
    }
}

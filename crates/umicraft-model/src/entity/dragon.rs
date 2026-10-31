// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ender Dragon box model matching MCP 1.8 `ModelDragon.java`.

use std::f32::consts::{PI, TAU};

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// Ender Dragon boss model (256×256 texture sheet) with head, jaw, neck, body, wings, articulated 3-part legs and tail.
#[derive(Debug, Clone)]
pub struct DragonModel {
    /// Head part with horns and nostrils.
    pub head: ModelPart,
    /// Articulated lower jaw (child of `head`).
    pub jaw: ModelPart,
    /// 5 neck segments connecting body to head.
    pub neck: Vec<ModelPart>,
    /// Massive 24×24×64 body torso with dorsal spines.
    pub body: ModelPart,
    /// Inner wing bone and membrane (rendered on right and mirrored on left).
    pub wing: ModelPart,
    /// Outer wing tip bone and membrane (child of `wing`).
    pub wing_tip: ModelPart,
    /// Upper front leg (thigh).
    pub front_leg: ModelPart,
    /// Lower front leg (shin, child of `front_leg`).
    pub front_leg_tip: ModelPart,
    /// Front foot (child of `front_leg_tip`).
    pub front_foot: ModelPart,
    /// Upper rear leg (thigh).
    pub rear_leg: ModelPart,
    /// Lower rear leg (shin, child of `rear_leg`).
    pub rear_leg_tip: ModelPart,
    /// Rear foot (child of `rear_leg_tip`).
    pub rear_foot: ModelPart,
    /// 12 tail segments trailing behind body.
    pub tail: Vec<ModelPart>,
}

/// The two transforms `ModelDragon.render` applies besides the angles of the
/// parts, in blocks and in the space of this crate's models.
#[derive(Debug, Clone, Copy)]
pub struct FlightFrame {
    /// Moves and tips the whole dragon (`translate(0, f1 - 2, -3)`,
    /// `rotate(f1 * 2, x)`).
    pub prelude: Mat4,
    /// Rolls the body, the wings and the legs about the back
    /// (`translate(0, 1, 0)`, `rotate(-f6 * 1.5, z)`, `translate(0, -1, 0)`).
    pub group: Mat4,
}

impl Default for FlightFrame {
    fn default() -> Self {
        Self {
            prelude: Mat4::IDENTITY,
            group: Mat4::IDENTITY,
        }
    }
}

/// `ModelDragon.updateRotations`: an angle brought into `-180..180`.
fn wrap_180(angle: f64) -> f32 {
    ((angle + 180.0).rem_euclid(360.0) - 180.0) as f32
}

impl DragonModel {
    /// Creates a new dragon model matching `ModelDragon.java`.
    #[must_use]
    pub fn new() -> Self {
        let tex = [256.0, 256.0];
        // MCP f = -16.0
        // Using exact isomorphism: MCP addBox(ox, oy, oz, w, h, d) -> Umicraft origin [ox, -(oy + h), oz]
        let head = ModelPart::new([0.0, 4.0, -62.0])
            // upperlip: addBox(-6, -1, -24, 12, 5, 16) at (176, 44)
            .with_box(ModelBox::new(
                [-6.0, -4.0, -24.0],
                [12.0, 5.0, 16.0],
                [176.0, 44.0],
                tex,
                0.0,
            ))
            // upperhead: addBox(-8, -8, -10, 16, 16, 16) at (112, 30)
            .with_box(ModelBox::new(
                [-8.0, -8.0, -10.0],
                [16.0, 16.0, 16.0],
                [112.0, 30.0],
                tex,
                0.0,
            ))
            // scale (mirrored): addBox(-5, -12, -4, 2, 4, 6) at (0, 0)
            .with_box(
                ModelBox::new([-5.0, 8.0, -4.0], [2.0, 4.0, 6.0], [0.0, 0.0], tex, 0.0).mirrored(),
            )
            // nostril (mirrored): addBox(-5, -3, -22, 2, 2, 4) at (112, 0)
            .with_box(
                ModelBox::new([-5.0, 1.0, -22.0], [2.0, 2.0, 4.0], [112.0, 0.0], tex, 0.0)
                    .mirrored(),
            )
            // scale: addBox(3, -12, -4, 2, 4, 6) at (0, 0)
            .with_box(ModelBox::new(
                [3.0, 8.0, -4.0],
                [2.0, 4.0, 6.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            // nostril: addBox(3, -3, -22, 2, 2, 4) at (112, 0)
            .with_box(ModelBox::new(
                [3.0, 1.0, -22.0],
                [2.0, 2.0, 4.0],
                [112.0, 0.0],
                tex,
                0.0,
            ));

        // Lower jaw: child of head at MCP (0, 4, -8), addBox(-6, 0, -16, 12, 4, 16) at (176, 65)
        let jaw = ModelPart::new([0.0, -4.0, -8.0]).with_box(ModelBox::new(
            [-6.0, -4.0, -16.0],
            [12.0, 4.0, 16.0],
            [176.0, 65.0],
            tex,
            0.0,
        ));

        let make_spine_segment = |pivot: [f32; 3]| {
            ModelPart::new(pivot)
                // neck.box: addBox(-5, -5, -5, 10, 10, 10) at (192, 104)
                .with_box(ModelBox::new(
                    [-5.0, -5.0, -5.0],
                    [10.0, 10.0, 10.0],
                    [192.0, 104.0],
                    tex,
                    0.0,
                ))
                // neck.scale: addBox(-1, -9, -3, 2, 4, 6) at (48, 0)
                .with_box(ModelBox::new(
                    [-1.0, 5.0, -3.0],
                    [2.0, 4.0, 6.0],
                    [48.0, 0.0],
                    tex,
                    0.0,
                ))
        };

        let neck = (0..5)
            .map(|i| make_spine_segment([0.0, 4.0, -12.0 - (i as f32) * 10.0]))
            .collect();

        // Body: MCP setRotationPoint(0, 4, 8) -> Umicraft [0, 20, 8]
        // addBox(-12, 0, -16, 24, 24, 64) -> origin [-12, -24, -16]
        let body = ModelPart::new([0.0, 20.0, 8.0])
            .with_box(ModelBox::new(
                [-12.0, -24.0, -16.0],
                [24.0, 24.0, 64.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            // 3 dorsal scales: addBox(-1, -6, -10 / 10 / 30, 2, 6, 12) at (220, 53)
            .with_box(ModelBox::new(
                [-1.0, 0.0, -10.0],
                [2.0, 6.0, 12.0],
                [220.0, 53.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.0, 0.0, 10.0],
                [2.0, 6.0, 12.0],
                [220.0, 53.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.0, 0.0, 30.0],
                [2.0, 6.0, 12.0],
                [220.0, 53.0],
                tex,
                0.0,
            ));

        // Wing: MCP setRotationPoint(-12, 5, 2) -> Umicraft [-12, 19, 2]
        let wing = ModelPart::new([-12.0, 19.0, 2.0])
            // wing.bone: addBox(-56, -4, -4, 56, 8, 8) at (112, 88)
            .with_box(ModelBox::new(
                [-56.0, -4.0, -4.0],
                [56.0, 8.0, 8.0],
                [112.0, 88.0],
                tex,
                0.0,
            ))
            // wing.skin: addBox(-56, 0, 2, 56, 0, 56) at (-56, 88)
            .with_box(ModelBox::new(
                [-56.0, 0.0, 2.0],
                [56.0, 0.0, 56.0],
                [-56.0, 88.0],
                tex,
                0.0,
            ));

        // WingTip: child of wing at (-56, 0, 0)
        let wing_tip = ModelPart::new([-56.0, 0.0, 0.0])
            // wingtip.bone: addBox(-56, -2, -2, 56, 4, 4) at (112, 136)
            .with_box(ModelBox::new(
                [-56.0, -2.0, -2.0],
                [56.0, 4.0, 4.0],
                [112.0, 136.0],
                tex,
                0.0,
            ))
            // wingtip.skin: addBox(-56, 0, 2, 56, 0, 56) at (-56, 144)
            .with_box(ModelBox::new(
                [-56.0, 0.0, 2.0],
                [56.0, 0.0, 56.0],
                [-56.0, 144.0],
                tex,
                0.0,
            ));

        // Front leg 3-segment hierarchy:
        // 1. frontLeg at MCP (-12, 20, 2) -> Umicraft [-12, 4, 2], box (-4, -4, -4, 8, 24, 8)
        let front_leg = ModelPart::new([-12.0, 4.0, 2.0]).with_box(ModelBox::new(
            [-4.0, -20.0, -4.0],
            [8.0, 24.0, 8.0],
            [112.0, 104.0],
            tex,
            0.0,
        ));
        // 2. frontLegTip child at MCP (0, 20, -1) -> Umicraft [0, -20, -1], box (-3, -1, -3, 6, 24, 6)
        let front_leg_tip = ModelPart::new([0.0, -20.0, -1.0]).with_box(ModelBox::new(
            [-3.0, -23.0, -3.0],
            [6.0, 24.0, 6.0],
            [226.0, 138.0],
            tex,
            0.0,
        ));
        // 3. frontFoot child at MCP (0, 23, 0) -> Umicraft [0, -23, 0], box (-4, 0, -12, 8, 4, 16)
        let front_foot = ModelPart::new([0.0, -23.0, 0.0]).with_box(ModelBox::new(
            [-4.0, -4.0, -12.0],
            [8.0, 4.0, 16.0],
            [144.0, 104.0],
            tex,
            0.0,
        ));

        // Rear leg 3-segment hierarchy:
        // 1. rearLeg at MCP (-16, 16, 42) -> Umicraft [-16, 8, 42], box (-8, -4, -8, 16, 32, 16)
        let rear_leg = ModelPart::new([-16.0, 8.0, 42.0]).with_box(ModelBox::new(
            [-8.0, -28.0, -8.0],
            [16.0, 32.0, 16.0],
            [0.0, 0.0],
            tex,
            0.0,
        ));
        // 2. rearLegTip child at MCP (0, 32, -4) -> Umicraft [0, -32, -4], box (-6, -2, 0, 12, 32, 12)
        let rear_leg_tip = ModelPart::new([0.0, -32.0, -4.0]).with_box(ModelBox::new(
            [-6.0, -30.0, 0.0],
            [12.0, 32.0, 12.0],
            [196.0, 0.0],
            tex,
            0.0,
        ));
        // 3. rearFoot child at MCP (0, 31, 4) -> Umicraft [0, -31, 4], box (-9, 0, -20, 18, 6, 24)
        let rear_foot = ModelPart::new([0.0, -31.0, 4.0]).with_box(ModelBox::new(
            [-9.0, -6.0, -20.0],
            [18.0, 6.0, 24.0],
            [112.0, 0.0],
            tex,
            0.0,
        ));

        let tail = (0..12)
            .map(|k| make_spine_segment([0.0, 14.0, 60.0 + (k as f32) * 10.0]))
            .collect();

        let mut model = Self {
            head,
            jaw,
            neck,
            body,
            wing,
            wing_tip,
            front_leg,
            front_leg_tip,
            front_foot,
            rear_leg,
            rear_leg_tip,
            rear_foot,
            tail,
        };
        model.set_angles(0.0);
        model
    }

    /// Sets wing flapping, leg folding, neck undulation and tail undulation matching `ModelDragon.render`.
    pub fn set_angles(&mut self, anim_time: f32) {
        // Phase offset 2.85 rad so at anim_time = 0.0 both wings show their full membrane and finger bones
        let f8 = 2.85 + anim_time * 0.1 * PI * 2.0;

        // Jaw opening (MCP ModelDragon.java:143)
        self.jaw.rotation[0] = -((f8.sin() + 1.0) * 0.15);

        let mut f1 = (f8 - 1.0).sin() + 1.0;
        f1 = (f1 * f1 + f1 * 2.0) * 0.05;

        // 5-segment neck chain leading to head (MCP ModelDragon.java:156-180)
        let mut f2 = 20.0_f32;
        let mut f3 = -12.0_f32;
        let mut f4 = 0.0_f32;
        for (i, seg) in self.neck.iter_mut().enumerate() {
            let rx_mcp = (i as f32 * 0.45 + f8).cos() * 0.15;
            let ry_mcp = 0.0_f32;
            seg.pivot = [f4, 24.0 - f2, f3];
            seg.rotation = [-rx_mcp, ry_mcp, 0.0];
            f2 += rx_mcp.sin() * 10.0;
            f3 -= ry_mcp.cos() * rx_mcp.cos() * 10.0;
            f4 -= ry_mcp.sin() * rx_mcp.cos() * 10.0;
        }
        self.head.pivot = [f4, 24.0 - f2, f3];

        // Wings and legs flight pose (MCP ModelDragon.java:192-202)
        self.wing.rotation[0] = -(0.125 - f8.cos() * 0.2);
        self.wing.rotation[1] = 0.25;
        self.wing.rotation[2] = -((f8.sin() + 0.125) * 0.8);
        self.wing_tip.rotation[2] = ((f8 + 2.0).sin() + 0.5) * 0.75;

        self.rear_leg.rotation[0] = -(1.0 + f1 * 0.1);
        self.rear_leg_tip.rotation[0] = -(0.5 + f1 * 0.1);
        self.rear_foot.rotation[0] = -(0.75 + f1 * 0.1);

        self.front_leg.rotation[0] = -(1.3 + f1 * 0.1);
        self.front_leg_tip.rotation[0] = 0.5 + f1 * 0.1;
        self.front_foot.rotation[0] = -(0.75 + f1 * 0.1);

        // 12-segment tail chain (MCP ModelDragon.java:219-238)
        let mut f10 = 0.0_f32;
        f2 = 10.0;
        f3 = 60.0;
        f4 = 0.0;
        for (k, seg) in self.tail.iter_mut().enumerate() {
            f10 += (k as f32 * 0.45 + f8).sin() * 0.05;
            let rx_mcp = f10;
            let ry_mcp = PI + (k as f32 * 0.25 + anim_time * 0.2).sin() * 0.15;
            seg.pivot = [f4, 24.0 - f2, f3];
            seg.rotation = [-rx_mcp, ry_mcp, 0.0];
            f2 += rx_mcp.sin() * 10.0;
            f3 -= ry_mcp.cos() * rx_mcp.cos() * 10.0;
            f4 -= ry_mcp.sin() * rx_mcp.cos() * 10.0;
        }
    }

    /// Poses the model for a dragon that flew along `offsets` (MCP
    /// `ModelDragon.render`): `offsets[i]` is the `[yaw, height]` it had `i`
    /// ticks ago, `anim_time` the phase of its wings in turns. The neck, the
    /// body and the tail bend along the path; the returned frame goes to
    /// [`Self::emit_mesh_in_flight`].
    ///
    /// # Panics
    /// When `offsets` has fewer than 24 entries.
    pub fn set_flight(&mut self, anim_time: f32, offsets: &[[f64; 2]]) -> FlightFrame {
        const SPREAD: f32 = 1.5;
        let f8 = anim_time * TAU;
        let mut f1 = (f8 - 1.0).sin() + 1.0;
        f1 = (f1 * f1 + f1 * 2.0) * 0.05;
        self.jaw.rotation[0] = -((f8.sin() + 1.0) * 0.2);

        let reference = offsets[6];
        let f6 = wrap_180(offsets[5][0] - offsets[10][0]);
        let f7 = wrap_180(offsets[5][0] + f64::from(f6) / 2.0);
        let (mut f2, mut f3, mut f4) = (20.0_f32, -12.0_f32, 0.0_f32);
        for (i, seg) in self.neck.iter_mut().enumerate() {
            let at = offsets[5 - i];
            let rx = (i as f32 * 0.45 + f8).cos() * 0.15
                + (at[1] - reference[1]).to_radians() as f32 * SPREAD * 5.0;
            let ry = wrap_180(at[0] - reference[0]).to_radians() * SPREAD;
            let rz = -wrap_180(at[0] - f64::from(f7)).to_radians() * SPREAD;
            seg.pivot = [f4, 24.0 - f2, f3];
            seg.rotation = [-rx, ry, -rz];
            f2 += rx.sin() * 10.0;
            f3 -= ry.cos() * rx.cos() * 10.0;
            f4 -= ry.sin() * rx.cos() * 10.0;
        }
        self.head.pivot = [f4, 24.0 - f2, f3];
        let ry = wrap_180(offsets[0][0] - reference[0]).to_radians();
        let rz = -wrap_180(offsets[0][0] - f64::from(f7)).to_radians();
        self.head.rotation = [0.0, ry, -rz];

        self.wing.rotation[0] = -(0.125 - f8.cos() * 0.2);
        self.wing.rotation[1] = 0.25;
        self.wing.rotation[2] = -((f8.sin() + 0.125) * 0.8);
        self.wing_tip.rotation[2] = ((f8 + 2.0).sin() + 0.5) * 0.75;
        self.rear_leg.rotation[0] = -(1.0 + f1 * 0.1);
        self.rear_leg_tip.rotation[0] = -(0.5 + f1 * 0.1);
        self.rear_foot.rotation[0] = -(0.75 + f1 * 0.1);
        self.front_leg.rotation[0] = -(1.3 + f1 * 0.1);
        self.front_leg_tip.rotation[0] = 0.5 + f1 * 0.1;
        self.front_foot.rotation[0] = -(0.75 + f1 * 0.1);

        let tail_reference = offsets[11];
        let (mut f10, mut f2, mut f3, mut f4) = (0.0_f32, 10.0_f32, 60.0_f32, 0.0_f32);
        for (k, seg) in self.tail.iter_mut().enumerate() {
            let at = offsets[12 + k];
            f10 += (k as f32 * 0.45 + f8).sin() * 0.05;
            let ry = (wrap_180(at[0] - tail_reference[0]) * SPREAD + 180.0).to_radians();
            let rx = f10 + (at[1] - tail_reference[1]).to_radians() as f32 * SPREAD * 5.0;
            let rz = wrap_180(at[0] - f64::from(f7)).to_radians() * SPREAD;
            seg.pivot = [f4, 24.0 - f2, f3];
            seg.rotation = [-rx, ry, -rz];
            f2 += rx.sin() * 10.0;
            f3 -= ry.cos() * rx.cos() * 10.0;
            f4 -= ry.sin() * rx.cos() * 10.0;
        }

        // The reference translates in blocks inside its `scale(-1, -1, 1)`, so
        // the y of a translation and the angle of a turn about x change sign in
        // this crate's frame; the roll about z only conjugates the mirror.
        FlightFrame {
            prelude: Mat4::from_translation(Vec3::new(0.0, 2.0 - f1, -3.0))
                * Mat4::from_rotation_x(-(f1 * 2.0).to_radians()),
            group: Mat4::from_translation(Vec3::new(0.0, -1.0, 0.0))
                * Mat4::from_rotation_z((f6 * SPREAD).to_radians())
                * Mat4::from_translation(Vec3::new(0.0, 1.0, 0.0)),
        }
    }

    /// Emits the dragon mesh vertices transformed into world units.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        self.emit_mesh_in_flight(&FlightFrame::default())
    }

    /// Emits the mesh with the extra transforms of [`Self::set_flight`].
    #[must_use]
    pub fn emit_mesh_in_flight(&self, frame: &FlightFrame) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let scale = 1.0 / 16.0;
        let pixels = Mat4::from_scale(Vec3::splat(scale));
        // The neck, the head and the tail are drawn outside the group that
        // rolls with the body.
        let root = frame.prelude * pixels;
        let grouped = frame.prelude * frame.group * pixels;

        for seg in &self.neck {
            seg.emit(root, &mut out);
        }

        let head_t = self.head.transform(root);
        self.head.emit(root, &mut out);
        self.jaw.emit(head_t, &mut out);

        self.body.emit(grouped, &mut out);

        // Emit right side (j = 0) and X-mirrored left side (j = 1), matching ModelDragon.java:189-212
        for side in [1.0_f32, -1.0_f32] {
            let side_root = grouped * Mat4::from_scale(Vec3::new(side, 1.0, 1.0));

            let wing_t = self.wing.transform(side_root);
            self.wing.emit(side_root, &mut out);
            self.wing_tip.emit(wing_t, &mut out);

            let fl_t = self.front_leg.transform(side_root);
            self.front_leg.emit(side_root, &mut out);
            let fl_tip_t = self.front_leg_tip.transform(fl_t);
            self.front_leg_tip.emit(fl_t, &mut out);
            self.front_foot.emit(fl_tip_t, &mut out);

            let rl_t = self.rear_leg.transform(side_root);
            self.rear_leg.emit(side_root, &mut out);
            let rl_tip_t = self.rear_leg_tip.transform(rl_t);
            self.rear_leg_tip.emit(rl_t, &mut out);
            self.rear_foot.emit(rl_tip_t, &mut out);
        }

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

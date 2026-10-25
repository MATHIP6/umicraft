// SPDX-License-Identifier: MIT OR Apache-2.0

//! 1.8 Entity box models, humanoid skeleton, mob models, and animation posing.

use std::f32::consts::PI;

use glam::{Mat4, Vec3};

/// A single vertex produced by an entity model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelVertex {
    /// Local vertex position in blocks.
    pub position: [f32; 3],
    /// Surface normal vector.
    pub normal: [f32; 3],
    /// Normalized texture coordinates (0.0 to 1.0).
    pub uv: [f32; 2],
}

/// A textured cuboid primitive with 6 faces (from MCP `ModelBox`).
#[derive(Debug, Clone)]
pub struct ModelBox {
    /// Minimum corner in model coordinates.
    pub origin: [f32; 3],
    /// Width, height, depth in model units.
    pub size: [f32; 3],
    /// UV offset on the texture sheet in pixels.
    pub uv: [f32; 2],
    /// Texture sheet dimensions in pixels (usually [64.0, 64.0] or [64.0, 32.0]).
    pub texture_size: [f32; 2],
    /// Dilation inflation applied outward along all axes (e.g. 0.25 or 0.5 for outer layers).
    pub dilation: f32,
}

impl ModelBox {
    /// Creates a new model box.
    #[must_use]
    pub const fn new(
        origin: [f32; 3],
        size: [f32; 3],
        uv: [f32; 2],
        texture_size: [f32; 2],
        dilation: f32,
    ) -> Self {
        Self {
            origin,
            size,
            uv,
            texture_size,
            dilation,
        }
    }

    /// Emits the 36 vertices (12 triangles) for this cuboid transformed by `transform`.
    pub fn emit_triangles(&self, transform: Mat4, out: &mut Vec<ModelVertex>) {
        let x0 = self.origin[0] - self.dilation;
        let y0 = self.origin[1] - self.dilation;
        let z0 = self.origin[2] - self.dilation;
        let x1 = self.origin[0] + self.size[0] + self.dilation;
        let y1 = self.origin[1] + self.size[1] + self.dilation;
        let z1 = self.origin[2] + self.size[2] + self.dilation;

        let w = self.size[0];
        let h = self.size[1];
        let d = self.size[2];

        let tw = self.texture_size[0];
        let th = self.texture_size[1];
        let u = self.uv[0];
        let v = self.uv[1];

        // 8 box corners
        let c000 = transform.transform_point3(Vec3::new(x0, y0, z0)).to_array();
        let c100 = transform.transform_point3(Vec3::new(x1, y0, z0)).to_array();
        let c110 = transform.transform_point3(Vec3::new(x1, y1, z0)).to_array();
        let c010 = transform.transform_point3(Vec3::new(x0, y1, z0)).to_array();
        let c001 = transform.transform_point3(Vec3::new(x0, y0, z1)).to_array();
        let c101 = transform.transform_point3(Vec3::new(x1, y0, z1)).to_array();
        let c111 = transform.transform_point3(Vec3::new(x1, y1, z1)).to_array();
        let c011 = transform.transform_point3(Vec3::new(x0, y1, z1)).to_array();

        // Transform normals
        let n_down = transform
            .transform_vector3(Vec3::new(0.0, -1.0, 0.0))
            .normalize_or_zero()
            .to_array();
        let n_up = transform
            .transform_vector3(Vec3::new(0.0, 1.0, 0.0))
            .normalize_or_zero()
            .to_array();
        let n_north = transform
            .transform_vector3(Vec3::new(0.0, 0.0, -1.0))
            .normalize_or_zero()
            .to_array();
        let n_south = transform
            .transform_vector3(Vec3::new(0.0, 0.0, 1.0))
            .normalize_or_zero()
            .to_array();
        let n_west = transform
            .transform_vector3(Vec3::new(-1.0, 0.0, 0.0))
            .normalize_or_zero()
            .to_array();
        let n_east = transform
            .transform_vector3(Vec3::new(1.0, 0.0, 0.0))
            .normalize_or_zero()
            .to_array();

        // Standard 1.8 UV mapping layout from MCP ModelBox.java:
        // Down (-Y)
        emit_quad(
            out,
            [c000, c100, c101, c001],
            n_down,
            [
                [(u + d + w) / tw, (v + d) / th],
                [(u + d + 2.0 * w) / tw, (v + d) / th],
                [(u + d + 2.0 * w) / tw, v / th],
                [(u + d + w) / tw, v / th],
            ],
        );
        // Up (+Y)
        emit_quad(
            out,
            [c011, c111, c110, c010],
            n_up,
            [
                [(u + d) / tw, v / th],
                [(u + d + w) / tw, v / th],
                [(u + d + w) / tw, (v + d) / th],
                [(u + d) / tw, (v + d) / th],
            ],
        );
        // North (-Z)
        emit_quad(
            out,
            [c100, c000, c010, c110],
            n_north,
            [
                [(u + d + w) / tw, (v + d + h) / th],
                [(u + d) / tw, (v + d + h) / th],
                [(u + d) / tw, (v + d) / th],
                [(u + d + w) / tw, (v + d) / th],
            ],
        );
        // South (+Z)
        emit_quad(
            out,
            [c001, c101, c111, c011],
            n_south,
            [
                [(u + 2.0 * d + 2.0 * w) / tw, (v + d + h) / th],
                [(u + 2.0 * d + w) / tw, (v + d + h) / th],
                [(u + 2.0 * d + w) / tw, (v + d) / th],
                [(u + 2.0 * d + 2.0 * w) / tw, (v + d) / th],
            ],
        );
        // West (-X)
        emit_quad(
            out,
            [c000, c001, c011, c010],
            n_west,
            [
                [(u + d) / tw, (v + d + h) / th],
                [u / tw, (v + d + h) / th],
                [u / tw, (v + d) / th],
                [(u + d) / tw, (v + d) / th],
            ],
        );
        // East (+X)
        emit_quad(
            out,
            [c101, c100, c110, c111],
            n_east,
            [
                [(u + 2.0 * d + w) / tw, (v + d + h) / th],
                [(u + d + w) / tw, (v + d + h) / th],
                [(u + d + w) / tw, (v + d) / th],
                [(u + 2.0 * d + w) / tw, (v + d) / th],
            ],
        );
    }
}

fn emit_quad(
    out: &mut Vec<ModelVertex>,
    corners: [[f32; 3]; 4],
    normal: [f32; 3],
    uvs: [[f32; 2]; 4],
) {
    // First triangle (0, 1, 2)
    out.push(ModelVertex {
        position: corners[0],
        normal,
        uv: uvs[0],
    });
    out.push(ModelVertex {
        position: corners[1],
        normal,
        uv: uvs[1],
    });
    out.push(ModelVertex {
        position: corners[2],
        normal,
        uv: uvs[2],
    });

    // Second triangle (0, 2, 3)
    out.push(ModelVertex {
        position: corners[0],
        normal,
        uv: uvs[0],
    });
    out.push(ModelVertex {
        position: corners[2],
        normal,
        uv: uvs[2],
    });
    out.push(ModelVertex {
        position: corners[3],
        normal,
        uv: uvs[3],
    });
}

/// Emits the 36 vertices (12 triangles) for a 3D miniature block item cuboid.
///
/// Unlike mob cuboids (which unwrap a skin texture sheet across 6 faces),
/// a dropped block entity in Minecraft 1.8 (`RenderEntityItem`) displays the
/// full block texture (0.0..1.0 UV span) across all six faces.
pub fn emit_block_item_cube(half_size: f32, transform: Mat4, out: &mut Vec<ModelVertex>) {
    let s = half_size;
    let x0 = -s;
    let y0 = -s;
    let z0 = -s;
    let x1 = s;
    let y1 = s;
    let z1 = s;

    // 8 box corners
    let c000 = transform.transform_point3(Vec3::new(x0, y0, z0)).to_array();
    let c100 = transform.transform_point3(Vec3::new(x1, y0, z0)).to_array();
    let c110 = transform.transform_point3(Vec3::new(x1, y1, z0)).to_array();
    let c010 = transform.transform_point3(Vec3::new(x0, y1, z0)).to_array();
    let c001 = transform.transform_point3(Vec3::new(x0, y0, z1)).to_array();
    let c101 = transform.transform_point3(Vec3::new(x1, y0, z1)).to_array();
    let c111 = transform.transform_point3(Vec3::new(x1, y1, z1)).to_array();
    let c011 = transform.transform_point3(Vec3::new(x0, y1, z1)).to_array();

    // Transform normals
    let n_down = transform
        .transform_vector3(Vec3::new(0.0, -1.0, 0.0))
        .normalize_or_zero()
        .to_array();
    let n_up = transform
        .transform_vector3(Vec3::new(0.0, 1.0, 0.0))
        .normalize_or_zero()
        .to_array();
    let n_north = transform
        .transform_vector3(Vec3::new(0.0, 0.0, -1.0))
        .normalize_or_zero()
        .to_array();
    let n_south = transform
        .transform_vector3(Vec3::new(0.0, 0.0, 1.0))
        .normalize_or_zero()
        .to_array();
    let n_west = transform
        .transform_vector3(Vec3::new(-1.0, 0.0, 0.0))
        .normalize_or_zero()
        .to_array();
    let n_east = transform
        .transform_vector3(Vec3::new(1.0, 0.0, 0.0))
        .normalize_or_zero()
        .to_array();

    let full_face_uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];

    // Down (-Y)
    emit_quad(
        out,
        [c000, c100, c101, c001],
        n_down,
        [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
    );
    // Up (+Y)
    emit_quad(out, [c011, c111, c110, c010], n_up, full_face_uvs);
    // North (-Z)
    emit_quad(out, [c100, c000, c010, c110], n_north, full_face_uvs);
    // South (+Z)
    emit_quad(out, [c001, c101, c111, c011], n_south, full_face_uvs);
    // West (-X)
    emit_quad(out, [c000, c001, c011, c010], n_west, full_face_uvs);
    // East (+X)
    emit_quad(out, [c101, c100, c110, c111], n_east, full_face_uvs);
}

/// An articulated body part with pivot and local Euler angles.
#[derive(Debug, Clone)]
pub struct ModelPart {
    /// Pivot offset relative to parent/entity root (in units, 1 unit = 1/16 block).
    pub pivot: [f32; 3],
    /// Pitch, yaw, roll in radians.
    pub rotation: [f32; 3],
    /// Cuboids belonging to this part.
    pub boxes: Vec<ModelBox>,
}

impl ModelPart {
    /// Creates an empty body part with the given pivot.
    #[must_use]
    pub const fn new(pivot: [f32; 3]) -> Self {
        Self {
            pivot,
            rotation: [0.0, 0.0, 0.0],
            boxes: Vec::new(),
        }
    }

    /// Adds a box to this part.
    pub fn with_box(mut self, b: ModelBox) -> Self {
        self.boxes.push(b);
        self
    }

    /// Computes the local transformation matrix for this part.
    #[must_use]
    pub fn transform(&self, parent: Mat4) -> Mat4 {
        let p = Vec3::new(self.pivot[0], self.pivot[1], self.pivot[2]);
        parent
            * Mat4::from_translation(p)
            * Mat4::from_rotation_z(self.rotation[2])
            * Mat4::from_rotation_y(self.rotation[1])
            * Mat4::from_rotation_x(self.rotation[0])
    }

    /// Emits all boxes in this part.
    pub fn emit(&self, parent: Mat4, out: &mut Vec<ModelVertex>) {
        let t = self.transform(parent);
        for b in &self.boxes {
            b.emit_triangles(t, out);
        }
    }
}

/// 1.8 Humanoid model (Player, Zombie, Skeleton) supporting Steve and Alex arm widths.
#[derive(Debug, Clone)]
pub struct HumanoidModel {
    /// Head part.
    pub head: ModelPart,
    /// Torso part.
    pub body: ModelPart,
    /// Right arm part.
    pub right_arm: ModelPart,
    /// Left arm part.
    pub left_arm: ModelPart,
    /// Right leg part.
    pub right_leg: ModelPart,
    /// Left leg part.
    pub left_leg: ModelPart,
}

impl HumanoidModel {
    /// Builds a 1.8 player humanoid model (Steve or Alex) with base and outer layers.
    #[must_use]
    pub fn new(is_slim: bool) -> Self {
        Self::create(is_slim, false)
    }

    /// Builds a 1.8 biped mob model (Zombie, Skeleton) with mirrored limb textures.
    #[must_use]
    pub fn biped(_is_zombie: bool) -> Self {
        Self::create(false, true)
    }

    fn create(is_slim: bool, is_mob: bool) -> Self {
        let tex = [64.0, 64.0];

        // Head (pivot at [0, 24, 0] units above feet)
        let head = ModelPart::new([0.0, 24.0, 0.0])
            .with_box(ModelBox::new(
                [-4.0, 0.0, -4.0],
                [8.0, 8.0, 8.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-4.0, 0.0, -4.0],
                [8.0, 8.0, 8.0],
                [32.0, 0.0],
                tex,
                0.5,
            ));

        // Torso (pivot at [0, 12, 0])
        let body = ModelPart::new([0.0, 12.0, 0.0])
            .with_box(ModelBox::new(
                [-4.0, 0.0, -2.0],
                [8.0, 12.0, 4.0],
                [16.0, 16.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-4.0, 0.0, -2.0],
                [8.0, 12.0, 4.0],
                [16.0, 32.0],
                tex,
                0.25,
            ));

        // Right arm
        let arm_w = if is_slim { 3.0 } else { 4.0 };
        let right_arm_pivot = [-5.0, 22.0, 0.0];
        let right_arm = ModelPart::new(right_arm_pivot)
            .with_box(ModelBox::new(
                [-arm_w + 1.0, -10.0, -2.0],
                [arm_w, 12.0, 4.0],
                [40.0, 16.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-arm_w + 1.0, -10.0, -2.0],
                [arm_w, 12.0, 4.0],
                [40.0, 32.0],
                tex,
                0.25,
            ));

        // Left arm (in 1.8 mobs like Zombie use 64x32 layout, mirroring the right arm texture)
        let left_arm_uv = if is_mob { [40.0, 16.0] } else { [32.0, 48.0] };
        let left_arm_wear_uv = if is_mob { [40.0, 16.0] } else { [48.0, 48.0] };
        let left_arm_pivot = [5.0, 22.0, 0.0];
        let left_arm = ModelPart::new(left_arm_pivot)
            .with_box(ModelBox::new(
                [-1.0, -10.0, -2.0],
                [arm_w, 12.0, 4.0],
                left_arm_uv,
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-1.0, -10.0, -2.0],
                [arm_w, 12.0, 4.0],
                left_arm_wear_uv,
                tex,
                0.25,
            ));

        // Right leg (pivot at [-1.9, 12, 0])
        let right_leg = ModelPart::new([-1.9, 12.0, 0.0])
            .with_box(ModelBox::new(
                [-2.0, -12.0, -2.0],
                [4.0, 12.0, 4.0],
                [0.0, 16.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-2.0, -12.0, -2.0],
                [4.0, 12.0, 4.0],
                [0.0, 32.0],
                tex,
                0.25,
            ));

        // Left leg (in 1.8 mobs mirror right leg texture at [0, 16])
        let left_leg_uv = if is_mob { [0.0, 16.0] } else { [16.0, 48.0] };
        let left_leg_wear_uv = if is_mob { [0.0, 16.0] } else { [0.0, 48.0] };
        let left_leg = ModelPart::new([1.9, 12.0, 0.0])
            .with_box(ModelBox::new(
                [-2.0, -12.0, -2.0],
                [4.0, 12.0, 4.0],
                left_leg_uv,
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [-2.0, -12.0, -2.0],
                [4.0, 12.0, 4.0],
                left_leg_wear_uv,
                tex,
                0.25,
            ));

        Self {
            head,
            body,
            right_arm,
            left_arm,
            right_leg,
            left_leg,
        }
    }

    /// Sets animation angles based on limb swing and head orientation.
    pub fn set_angles(
        &mut self,
        limb_swing: f32,
        limb_swing_amount: f32,
        head_yaw: f32,
        head_pitch: f32,
        is_zombie: bool,
    ) {
        self.head.rotation[0] = head_pitch.to_radians();
        self.head.rotation[1] = head_yaw.to_radians();

        // Walking animation
        let leg_angle = (limb_swing * 0.6662).cos() * 1.4 * limb_swing_amount;
        self.right_leg.rotation[0] = leg_angle;
        self.left_leg.rotation[0] = -leg_angle;

        if is_zombie {
            // Zombie arms extended forward towards -Z (North)
            self.right_arm.rotation[0] = PI / 2.0;
            self.left_arm.rotation[0] = PI / 2.0;
            self.right_arm.rotation[1] = -0.1;
            self.left_arm.rotation[1] = 0.1;
            self.right_arm.rotation[2] = 0.0;
            self.left_arm.rotation[2] = 0.0;
        } else {
            // Normal human swinging arms
            let arm_angle = (limb_swing * 0.6662).cos() * 2.0 * limb_swing_amount * 0.5;
            self.right_arm.rotation[0] = -arm_angle;
            self.left_arm.rotation[0] = arm_angle;
        }
    }

    /// Generates the complete vertex mesh in blocks (1 unit = 1/16 block).
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        // Scale by 1/16 to convert pixel units to block coordinates
        let root = Mat4::from_scale(Vec3::splat(1.0 / 16.0));

        self.head.emit(root, &mut out);
        self.body.emit(root, &mut out);
        self.right_arm.emit(root, &mut out);
        self.left_arm.emit(root, &mut out);
        self.right_leg.emit(root, &mut out);
        self.left_leg.emit(root, &mut out);

        out
    }
}

/// Quadruped model (Cow, Pig).
#[derive(Debug, Clone)]
pub struct QuadrupedModel {
    /// Head part.
    pub head: ModelPart,
    /// Torso part.
    pub body: ModelPart,
    /// Leg 1 (rear right).
    pub leg1: ModelPart,
    /// Leg 2 (rear left).
    pub leg2: ModelPart,
    /// Leg 3 (front right).
    pub leg3: ModelPart,
    /// Leg 4 (front left).
    pub leg4: ModelPart,
}

impl QuadrupedModel {
    /// Creates a cow model (64×32 texture) exactly following MCP `ModelCow.java` and `ModelQuadruped.java`.
    #[must_use]
    pub fn cow() -> Self {
        let tex = [64.0, 32.0];
        // Head: pivot at [0, 4, -8] in MCP (Y=20 in Umicraft)
        let head = ModelPart::new([0.0, 20.0, -8.0])
            .with_box(ModelBox::new(
                [-4.0, -4.0, -6.0],
                [8.0, 8.0, 6.0],
                [0.0, 0.0],
                tex,
                0.0,
            ))
            // Horns: MCP [-5, -5, -4] and [4, -5, -4] size [1, 3, 1]
            .with_box(ModelBox::new(
                [-5.0, 2.0, -4.0],
                [1.0, 3.0, 1.0],
                [22.0, 0.0],
                tex,
                0.0,
            ))
            .with_box(ModelBox::new(
                [4.0, 2.0, -4.0],
                [1.0, 3.0, 1.0],
                [22.0, 0.0],
                tex,
                0.0,
            ));

        // Body: pivot at [0, 5, 2] in MCP (Y=15 in Umicraft) with rotation +PI/2
        // Main box [-6, -10, -7] size [12, 18, 10]:
        //   y' = -z in [-3, 7] -> Y_world in [12, 22] (belly at 12, back at 22)
        //   z' = y in [-10, 8] -> Z_world in [-8, 10] (front at -8, rear at 10)
        let mut body = ModelPart::new([0.0, 15.0, 2.0])
            .with_box(ModelBox::new(
                [-6.0, -10.0, -7.0],
                [12.0, 18.0, 10.0],
                [18.0, 4.0],
                tex,
                0.0,
            ))
            // Udder (MCP: addBox(-2.0F, 2.0F, -8.0F, 4, 6, 1) with textureOffset(52, 0))
            // local: x in [-2, 2], y in [2, 8] (Z_world in [4, 10]), z in [3, 4] (Y_world in [11, 12])
            .with_box(ModelBox::new(
                [-2.0, 2.0, 3.0],
                [4.0, 6.0, 1.0],
                [52.0, 0.0],
                tex,
                0.0,
            ));
        body.rotation[0] = PI / 2.0;

        // Legs: pivots from MCP ModelCow (--leg3.Z, --leg4.Z -> Z = -6.0)
        let leg1 = ModelPart::new([-4.0, 12.0, 7.0]).with_box(ModelBox::new(
            [-2.0, -12.0, -2.0],
            [4.0, 12.0, 4.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));
        let leg2 = ModelPart::new([4.0, 12.0, 7.0]).with_box(ModelBox::new(
            [-2.0, -12.0, -2.0],
            [4.0, 12.0, 4.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));
        let leg3 = ModelPart::new([-4.0, 12.0, -6.0]).with_box(ModelBox::new(
            [-2.0, -12.0, -2.0],
            [4.0, 12.0, 4.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));
        let leg4 = ModelPart::new([4.0, 12.0, -6.0]).with_box(ModelBox::new(
            [-2.0, -12.0, -2.0],
            [4.0, 12.0, 4.0],
            [0.0, 16.0],
            tex,
            0.0,
        ));

        Self {
            head,
            body,
            leg1,
            leg2,
            leg3,
            leg4,
        }
    }

    /// Sets walking animation angles matching MCP `ModelQuadruped.java`.
    pub fn set_angles(
        &mut self,
        limb_swing: f32,
        limb_swing_amount: f32,
        head_yaw: f32,
        head_pitch: f32,
    ) {
        self.head.rotation[0] = head_pitch.to_radians();
        self.head.rotation[1] = head_yaw.to_radians();

        let leg_angle = (limb_swing * 0.6662).cos() * 1.4 * limb_swing_amount;
        self.leg1.rotation[0] = -leg_angle;
        self.leg2.rotation[0] = leg_angle;
        self.leg3.rotation[0] = leg_angle;
        self.leg4.rotation[0] = -leg_angle;
    }

    /// Emits all vertices.
    #[must_use]
    pub fn emit_mesh(&self) -> Vec<ModelVertex> {
        let mut out = Vec::new();
        let root = Mat4::from_scale(Vec3::splat(1.0 / 16.0));
        self.head.emit(root, &mut out);
        self.body.emit(root, &mut out);
        self.leg1.emit(root, &mut out);
        self.leg2.emit(root, &mut out);
        self.leg3.emit(root, &mut out);
        self.leg4.emit(root, &mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanoid_model_vertex_count() {
        let model = HumanoidModel::new(false);
        let mesh = model.emit_mesh();
        // 6 parts * (base box 36 + outer box 36) = 12 boxes * 36 = 432 vertices
        assert_eq!(mesh.len(), 432);
    }

    #[test]
    fn quadruped_model_vertex_count() {
        let model = QuadrupedModel::cow();
        let mesh = model.emit_mesh();
        // Head (3 boxes: head + 2 horns) + Body (2 boxes: body + udder) + 4 legs (4 boxes) = 9 boxes * 36 = 324 vertices
        assert_eq!(mesh.len(), 324);
    }
}

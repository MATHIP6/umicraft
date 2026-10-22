// SPDX-License-Identifier: MIT OR Apache-2.0

//! Baked block-local geometry shared by the model pipeline and the mesher.
//!
//! The face baker reproduces the 1.8 `FaceBakery` pipeline:
//!
//! 1. corner order from `EnumFaceDirection`;
//! 2. automatic UVs from the element box (`BlockPart`);
//! 3. element rotation around `origin / 16`, including the `rescale` factor;
//! 4. blockstate `x`/`y` rotation around the block centre (`ModelRotation`);
//! 5. blockstate face UV rotation (`BlockFaceUV`);
//! 6. optional `uvlock` UVs derived from the rotated corner positions.
//!
//! References (ADR-0001): `FaceBakery.java`, `EnumFaceDirection.java`,
//! `BlockPart.java`, `BlockFaceUV.java` and `ModelRotation.java` in the local
//! 1.8 MCP tree. Corner order is counter-clockwise seen from outside, so the
//! mesher can emit `(0,1,2)` and `(0,2,3)` with a front-face winding of CCW.
//! The source of the formulas is recorded in `docs/sources.md`.

/// The six axis-aligned block faces.
///
/// The declaration order is the canonical face order used by the baker (the
/// same order as [`Face::ALL`]); `EnumFacing`'s wire order is only needed when
/// snapping a computed normal back to a face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Face {
    /// −X.
    West,
    /// +X.
    East,
    /// −Y.
    Down,
    /// +Y.
    Up,
    /// −Z.
    North,
    /// +Z.
    South,
}

impl Face {
    /// Neighbour offset from the current block.
    #[must_use]
    pub const fn offset(self) -> [i32; 3] {
        match self {
            Self::West => [-1, 0, 0],
            Self::East => [1, 0, 0],
            Self::Down => [0, -1, 0],
            Self::Up => [0, 1, 0],
            Self::North => [0, 0, -1],
            Self::South => [0, 0, 1],
        }
    }

    /// Outward unit normal.
    #[must_use]
    pub const fn normal(self) -> [f32; 3] {
        match self {
            Self::West => [-1.0, 0.0, 0.0],
            Self::East => [1.0, 0.0, 0.0],
            Self::Down => [0.0, -1.0, 0.0],
            Self::Up => [0.0, 1.0, 0.0],
            Self::North => [0.0, 0.0, -1.0],
            Self::South => [0.0, 0.0, 1.0],
        }
    }

    /// All faces in the canonical order used by the model baker.
    pub const ALL: [Self; 6] = [
        Self::West,
        Self::East,
        Self::Down,
        Self::Up,
        Self::North,
        Self::South,
    ];

    /// `EnumFacing` order used by `FaceBakery.getFacingFromVertexData`.
    const VANILLA_ORDER: [Self; 6] = [
        Self::Down,
        Self::Up,
        Self::North,
        Self::South,
        Self::West,
        Self::East,
    ];

    /// Parses a 1.8 face name.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "west" => Some(Self::West),
            "east" => Some(Self::East),
            "down" => Some(Self::Down),
            "up" => Some(Self::Up),
            "north" => Some(Self::North),
            "south" => Some(Self::South),
            _ => None,
        }
    }

    /// Axis the face normal points along.
    fn axis(self) -> FaceAxis {
        match self {
            Self::West | Self::East => FaceAxis::X,
            Self::Down | Self::Up => FaceAxis::Y,
            Self::North | Self::South => FaceAxis::Z,
        }
    }

    /// `EnumFacing.rotateAround(Axis.X)`: clockwise for non-X faces.
    fn rotate_around_x(self) -> Self {
        match self {
            Self::North => Self::Down,
            Self::South => Self::Up,
            Self::Up => Self::North,
            Self::Down => Self::South,
            Self::West | Self::East => self,
        }
    }

    /// `EnumFacing.rotateAround(Axis.Y)`: clockwise for non-Y faces.
    fn rotate_around_y(self) -> Self {
        match self {
            Self::North => Self::East,
            Self::East => Self::South,
            Self::South => Self::West,
            Self::West => Self::North,
            Self::Up | Self::Down => self,
        }
    }
}

/// One baked, textured quad in block-local coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelQuad {
    /// Four corners, counter-clockwise seen from outside, in block-local space.
    pub positions: [[f32; 3]; 4],
    /// Outward normal after element and variant rotation.
    pub normal: [f32; 3],
    /// Four normalized texture coordinates with `v = 0` at the texture top.
    pub uvs: [[f32; 2]; 4],
    /// Concrete texture key, e.g. `blocks/stone`.
    pub texture: String,
    /// Face that may cull this quad against a full opaque neighbour.
    pub cullface: Option<Face>,
    /// Whether vanilla face shading applies (false for cross/plants).
    pub shade: bool,
    /// Optional tint index from the model.
    pub tint: Option<u32>,
}

/// Rotation axis of an element, from the `rotation.axis` model property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RotationAxis {
    /// X axis.
    X,
    /// Y axis.
    Y,
    /// Z axis.
    Z,
}

/// Element rotation in model units (`origin` is divided by 16 when applied).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ElementRotation {
    /// Rotation origin in model units (`0..16` for vanilla assets).
    pub origin: [f64; 3],
    /// Rotation axis.
    pub axis: RotationAxis,
    /// Angle in degrees, one of `-45/-22.5/0/22.5/45`.
    pub angle: f64,
    /// Whether the 1/cos correction is applied to the perpendicular axes.
    pub rescale: bool,
}

/// Normalized blockstate rotation (`ModelRotation`), in degrees `0/90/180/270`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) struct ModelRotation {
    /// X rotation after normalization.
    pub x: u32,
    /// Y rotation after normalization.
    pub y: u32,
}

impl ModelRotation {
    /// Normalizes raw JSON angles into `0/90/180/270`.
    ///
    /// The caller rejects angles that are not multiples of 90 before this
    /// point, so the result is always an exact quarter turn.
    #[must_use]
    pub(crate) fn new(x: i32, y: i32) -> Self {
        Self {
            x: x.rem_euclid(360) as u32,
            y: y.rem_euclid(360) as u32,
        }
    }

    /// Number of quarter turns around X.
    fn quarters_x(self) -> u32 {
        self.x / 90
    }

    /// Number of quarter turns around Y.
    fn quarters_y(self) -> u32 {
        self.y / 90
    }
}

/// Everything needed to bake one quad from one model face.
pub(crate) struct QuadSpec<'a> {
    /// Face key in the element (`north`, `up`, …).
    pub face: Face,
    /// Element `from` in model units.
    pub from: [f64; 3],
    /// Element `to` in model units.
    pub to: [f64; 3],
    /// Explicit `uv` rectangle in `0..16` units, or `None` for automatic UVs.
    pub uv: Option<[f64; 4]>,
    /// Face UV rotation in quarter turns (`0..=3`).
    pub uv_rotation: u8,
    /// Cull face as written on the face, before blockstate rotation.
    pub cullface: Option<Face>,
    /// Concrete texture key.
    pub texture: &'a str,
    /// Tint index; `None` for `-1`.
    pub tint: Option<u32>,
    /// Element `shade` flag.
    pub shade: bool,
    /// Element rotation, if any.
    pub element_rotation: Option<ElementRotation>,
    /// Blockstate rotation.
    pub model_rotation: ModelRotation,
    /// Whether UVs are derived from the final positions.
    pub uvlock: bool,
}

/// Bakes one face of one element into a [`ModelQuad`].
pub(crate) fn bake_quad(spec: &QuadSpec<'_>) -> ModelQuad {
    let quarters_x = spec.model_rotation.quarters_x();
    let quarters_y = spec.model_rotation.quarters_y();

    // Start in model units (the element box), then apply the 1/16 scale.
    let mut corners = face_corners(spec.face, spec.from, spec.to);
    for corner in &mut corners {
        *corner = [corner[0] / 16.0, corner[1] / 16.0, corner[2] / 16.0];
    }
    if let Some(rotation) = spec.element_rotation {
        apply_element_rotation(&mut corners, rotation);
    }

    // `FaceBakery` stores each rotated vertex at the index returned by
    // `ModelRotation.rotateVertex`; keeping that shuffle makes the output
    // exactly the vanilla vertex order.
    let mut positions = [[0.0_f64; 3]; 4];
    for (vertex, corner) in corners.iter().enumerate() {
        let rotated = if quarters_x == 0 && quarters_y == 0 {
            *corner
        } else {
            rotate_around_center(*corner, spec.model_rotation)
        };
        positions[rotate_vertex(spec.face, vertex, quarters_x, quarters_y)] = rotated;
    }

    // Default UVs come from the element box (`BlockPart.setDefaultUvs`) or the
    // explicit `uv`, then `BlockFaceUV` rotates them by quarter turns.
    let uv_rect = spec
        .uv
        .unwrap_or_else(|| automatic_uv(spec.face, spec.from, spec.to));
    let mut uvs = [[0.0_f32; 2]; 4];
    for vertex in 0..4 {
        let slot = rotate_vertex(spec.face, vertex, quarters_x, quarters_y);
        uvs[slot] = rotated_uv(uv_rect, spec.uv_rotation, vertex);
    }

    let normal = normal_from_positions(&positions);
    let actual_face = normal.and_then(facing_from_normal).unwrap_or(Face::Up);

    if spec.uvlock {
        // `FaceBakery.func_178409_a`: derive the UV of every final slot from
        // its rotated position, then store it in the slot rotated back by the
        // face rotation (`BlockFaceUV.func_178345_c`).
        for (slot, position) in positions.iter().enumerate() {
            let (u, v) = uv_from_position(actual_face, *position);
            let destination = (slot + (4 - usize::from(spec.uv_rotation))) % 4;
            uvs[destination] = [u as f32, v as f32];
        }
    }

    let normal = normal
        .map(|normal| [normal[0] as f32, normal[1] as f32, normal[2] as f32])
        .unwrap_or_else(|| actual_face.normal());

    ModelQuad {
        positions: positions.map(|corner| [corner[0] as f32, corner[1] as f32, corner[2] as f32]),
        normal,
        uvs,
        texture: spec.texture.to_string(),
        cullface: spec
            .cullface
            .map(|face| rotate_face(face, quarters_x, quarters_y)),
        shade: spec.shade,
        tint: spec.tint,
    }
}

/// `EnumFaceDirection`: four `(x, y, z)` selectors from the element box.
///
/// The order is counter-clockwise seen from outside. This table is the core of
/// the vanilla geometry contract; deriving it from a generic winding rule tends
/// to flip the `down`/`north` faces.
fn face_corners(face: Face, from: [f64; 3], to: [f64; 3]) -> [[f64; 3]; 4] {
    let [x0, y0, z0] = from;
    let [x1, y1, z1] = to;
    match face {
        Face::Down => [[x0, y0, z1], [x0, y0, z0], [x1, y0, z0], [x1, y0, z1]],
        Face::Up => [[x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]],
        Face::North => [[x1, y1, z0], [x1, y0, z0], [x0, y0, z0], [x0, y1, z0]],
        Face::South => [[x0, y1, z1], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1]],
        Face::West => [[x0, y1, z0], [x0, y0, z0], [x0, y0, z1], [x0, y1, z1]],
        Face::East => [[x1, y1, z1], [x1, y0, z1], [x1, y0, z0], [x1, y1, z0]],
    }
}

/// `BlockPart.getFaceUvs`: the automatic `uv` rectangle in model units.
fn automatic_uv(face: Face, from: [f64; 3], to: [f64; 3]) -> [f64; 4] {
    match face {
        Face::Down | Face::Up => [from[0], from[2], to[0], to[2]],
        Face::North | Face::South => [from[0], 16.0 - to[1], to[0], 16.0 - from[1]],
        Face::West | Face::East => [from[2], 16.0 - to[1], to[2], 16.0 - from[1]],
    }
}

/// `BlockFaceUV.func_178348_a` / `func_178346_b`: pick the rotated corner UV.
fn rotated_uv(rect: [f64; 4], rotation: u8, vertex: usize) -> [f32; 2] {
    let index = (vertex + usize::from(rotation)) % 4;
    let u = if index < 2 { rect[0] } else { rect[2] };
    let v = if index == 0 || index == 3 {
        rect[1]
    } else {
        rect[3]
    };
    [(u / 16.0) as f32, (v / 16.0) as f32]
}

/// `FaceBakery.func_178401_a`: the `uvlock` UV of one final corner.
fn uv_from_position(face: Face, position: [f64; 3]) -> (f64, f64) {
    let x = wrap_coordinate(position[0]);
    let y = wrap_coordinate(position[1]);
    let z = wrap_coordinate(position[2]);
    // The vanilla code computes in 0..16 texture units and divides by the
    // sprite frame; normalized coordinates make that a plain 0..1 result.
    match face {
        Face::Down => (x, 1.0 - z),
        Face::Up => (x, z),
        Face::North => (1.0 - x, 1.0 - y),
        Face::South => (x, 1.0 - y),
        Face::West => (z, 1.0 - y),
        Face::East => (1.0 - z, 1.0 - y),
    }
}

/// Positions just outside the block wrap back into `0..1` for `uvlock`.
fn wrap_coordinate(value: f64) -> f64 {
    if !(-0.1..1.1).contains(&value) {
        value - value.floor()
    } else {
        value
    }
}

/// `ModelRotation`: rotate the corners around the block centre.
fn rotate_around_center(position: [f64; 3], rotation: ModelRotation) -> [f64; 3] {
    let matrix =
        Mat3::rotation_y(-f64::from(rotation.y)).multiply(Mat3::rotation_x(-f64::from(rotation.x)));
    transform_around(position, [0.5, 0.5, 0.5], matrix, [1.0, 1.0, 1.0])
}

/// `FaceBakery.func_178407_a` / `BlockPartRotation`.
fn apply_element_rotation(corners: &mut [[f64; 3]; 4], rotation: ElementRotation) {
    let matrix = match rotation.axis {
        RotationAxis::X => Mat3::rotation_x(rotation.angle),
        RotationAxis::Y => Mat3::rotation_y(rotation.angle),
        RotationAxis::Z => Mat3::rotation_z(rotation.angle),
    };
    let scale = if rotation.rescale {
        // 1/cos(22.5°) - 1 for 22.5°, 1/cos(45°) - 1 otherwise.
        let factor = if rotation.angle.abs() == 22.5 {
            1.0 / (std::f64::consts::PI / 8.0).cos() - 1.0
        } else {
            1.0 / (std::f64::consts::PI / 4.0).cos() - 1.0
        };
        let stretched = 1.0 + factor;
        match rotation.axis {
            RotationAxis::X => [1.0, stretched, stretched],
            RotationAxis::Y => [stretched, 1.0, stretched],
            RotationAxis::Z => [stretched, stretched, 1.0],
        }
    } else {
        [1.0, 1.0, 1.0]
    };
    let origin = [
        rotation.origin[0] / 16.0,
        rotation.origin[1] / 16.0,
        rotation.origin[2] / 16.0,
    ];
    for corner in corners.iter_mut() {
        *corner = transform_around(*corner, origin, matrix, scale);
    }
}

/// `FaceBakery.rotateScale`: rotate around `origin`, then scale per axis.
fn transform_around(
    position: [f64; 3],
    origin: [f64; 3],
    matrix: Mat3,
    scale: [f64; 3],
) -> [f64; 3] {
    let local = [
        position[0] - origin[0],
        position[1] - origin[1],
        position[2] - origin[2],
    ];
    let rotated = matrix.transform(local);
    [
        rotated[0] * scale[0] + origin[0],
        rotated[1] * scale[1] + origin[1],
        rotated[2] * scale[2] + origin[2],
    ]
}

/// `FaceBakery.rotateVertex`: index of a rotated corner in the final quad.
fn rotate_vertex(face: Face, vertex: usize, quarters_x: u32, quarters_y: u32) -> usize {
    let mut index = vertex;
    if face.axis() == FaceAxis::X {
        index = (index + quarters_x as usize) % 4;
    }
    let mut rotated = face;
    for _ in 0..quarters_x {
        rotated = rotated.rotate_around_x();
    }
    if rotated.axis() == FaceAxis::Y {
        index = (index + quarters_y as usize) % 4;
    }
    index
}

/// `ModelRotation.rotateFace`: rotate a face key, including a `cullface`.
fn rotate_face(mut face: Face, quarters_x: u32, quarters_y: u32) -> Face {
    for _ in 0..quarters_x {
        face = face.rotate_around_x();
    }
    if face.axis() != FaceAxis::Y {
        for _ in 0..quarters_y {
            face = face.rotate_around_y();
        }
    }
    face
}

/// Normal of the final quad, normalized; `None` for a degenerate quad.
fn normal_from_positions(positions: &[[f64; 3]; 4]) -> Option<[f64; 3]> {
    let edge_a = subtract(positions[2], positions[1]);
    let edge_b = subtract(positions[0], positions[1]);
    let normal = cross(edge_a, edge_b);
    let length = dot(normal, normal).sqrt();
    if length <= 1.0e-12 || !length.is_finite() {
        return None;
    }
    Some([normal[0] / length, normal[1] / length, normal[2] / length])
}

/// `FaceBakery.getFacingFromVertexData`: nearest face for `uvlock`.
fn facing_from_normal(normal: [f64; 3]) -> Option<Face> {
    let mut best = 0.0;
    let mut facing = None;
    for face in Face::VANILLA_ORDER {
        let axis = match face {
            Face::West => [-1.0, 0.0, 0.0],
            Face::East => [1.0, 0.0, 0.0],
            Face::Down => [0.0, -1.0, 0.0],
            Face::Up => [0.0, 1.0, 0.0],
            Face::North => [0.0, 0.0, -1.0],
            Face::South => [0.0, 0.0, 1.0],
        };
        let projection = dot(normal, axis);
        if projection >= 0.0 && projection > best {
            best = projection;
            facing = Some(face);
        }
    }
    facing
}

fn subtract(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

/// Axis of a face normal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FaceAxis {
    X,
    Y,
    Z,
}

/// Column-vector 3×3 matrix.
#[derive(Debug, Clone, Copy)]
struct Mat3 {
    rows: [[f64; 3]; 3],
}

impl Mat3 {
    fn rotation_x(degrees: f64) -> Self {
        let (sin, cos) = degrees.to_radians().sin_cos();
        Self {
            rows: [[1.0, 0.0, 0.0], [0.0, cos, -sin], [0.0, sin, cos]],
        }
    }

    fn rotation_y(degrees: f64) -> Self {
        let (sin, cos) = degrees.to_radians().sin_cos();
        Self {
            rows: [[cos, 0.0, sin], [0.0, 1.0, 0.0], [-sin, 0.0, cos]],
        }
    }

    fn rotation_z(degrees: f64) -> Self {
        let (sin, cos) = degrees.to_radians().sin_cos();
        Self {
            rows: [[cos, -sin, 0.0], [sin, cos, 0.0], [0.0, 0.0, 1.0]],
        }
    }

    /// Matrix product `self * other`, i.e. apply `other` first.
    fn multiply(self, other: Self) -> Self {
        let mut rows = [[0.0; 3]; 3];
        for (row, values) in rows.iter_mut().enumerate() {
            for (column, value) in values.iter_mut().enumerate() {
                *value = (0..3)
                    .map(|k| self.rows[row][k] * other.rows[k][column])
                    .sum();
            }
        }
        Self { rows }
    }

    fn transform(self, vector: [f64; 3]) -> [f64; 3] {
        [
            dot(self.rows[0], vector),
            dot(self.rows[1], vector),
            dot(self.rows[2], vector),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn up_quad() -> QuadSpec<'static> {
        QuadSpec {
            face: Face::Up,
            from: [0.0, 0.0, 0.0],
            to: [16.0, 16.0, 16.0],
            uv: None,
            uv_rotation: 0,
            cullface: None,
            texture: "blocks/stone",
            tint: None,
            shade: true,
            element_rotation: None,
            model_rotation: ModelRotation::default(),
            uvlock: false,
        }
    }

    #[test]
    fn corner_order_is_counter_clockwise_from_outside() {
        for face in Face::ALL {
            let corners = face_corners(face, [0.0, 0.0, 0.0], [16.0, 16.0, 16.0]);
            let normal = normal_from_positions(&corners).expect("non-degenerate face");
            assert_eq!(facing_from_normal(normal), Some(face), "{face:?}");
        }
    }

    #[test]
    fn automatic_uv_uses_v_zero_at_the_texture_top() {
        let quad = bake_quad(&up_quad());
        assert_eq!(quad.uvs, [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]);
    }

    #[test]
    fn face_rotation_rotates_the_uv_corners() {
        let mut spec = up_quad();
        spec.uv = Some([0.0, 0.0, 16.0, 16.0]);
        spec.uv_rotation = 1;
        let quad = bake_quad(&spec);
        assert_eq!(quad.uvs, [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
    }

    #[test]
    fn y_rotation_rotates_the_cullface_and_positions() {
        let mut spec = up_quad();
        spec.face = Face::North;
        spec.from = [0.0, 0.0, 0.0];
        spec.to = [16.0, 16.0, 0.0];
        spec.cullface = Some(Face::North);
        spec.model_rotation = ModelRotation::new(0, 90);
        let quad = bake_quad(&spec);
        assert_eq!(quad.cullface, Some(Face::East));
        for corner in quad.positions {
            assert!((corner[0] - 1.0).abs() < 1.0e-6, "{corner:?}");
        }
        assert!((quad.normal[0] - 1.0).abs() < 1.0e-6);
    }
}

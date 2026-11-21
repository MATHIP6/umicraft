// SPDX-License-Identifier: MIT OR Apache-2.0

//! Geometry of custom entity models (CEM, ADR-0089).
//!
//! A [`CemRig`] holds a parsed model and its compiled animations. Each frame
//! it takes the posed vanilla model of a mob, replaces or extends the parts
//! the pack names, runs the animations and returns the vertices of the custom
//! parts. The vanilla parts it replaced have no boxes left, so the caller
//! draws the vanilla model as usual for the rest.
//!
//! Coordinates: a `.jem` is in the space of the MCP models (pixels, y down,
//! `rx`/`rz` positive the other way round), while the models of this crate
//! are y-up (`24 - y` for a top-level pivot, and `x`/`z` rotations negated;
//! see `docs/tasks/mob-animations`). The conversion happens here, once, so
//! the animations keep reading and writing the values the pack author sees.
//!
//! Replace versus attach follows the public documentation
//! (`cem_model.txt`): `attach: true` adds the part to the vanilla part, so it
//! moves with it; otherwise the vanilla part is dropped and the custom part
//! stands in its place. Deviations: sprites, per-part textures, `mirrorTexture`
//! on `v` and `sizesAdd` are not drawn or used.

use glam::{Mat4, Vec3};
use umicraft_packext::cem::{
    BoxUv, CemAnimator, CemBox, CemModel, CemPart, CemState, EntityInputs, PackPath, PartPose,
    RootDelta,
};

use super::cem_parts::CemMob;
use super::{emit_quad, ModelBox, ModelPart, ModelVertex};

/// Texture size when neither the file nor a part gives one (the default of
/// the vanilla `ModelBase`).
const DEFAULT_TEXTURE_SIZE: [f32; 2] = [64.0, 32.0];

/// Height of the model origin above the feet, which turns y-down pivots into
/// y-up ones for top-level parts (see the module comment).
const MODEL_HEIGHT: f32 = 24.0;

/// A custom model with its compiled animations.
#[derive(Debug, Clone)]
pub struct CemRig {
    model: CemModel,
    animator: CemAnimator,
}

impl CemRig {
    /// Compiles the animations of `model`.
    #[must_use]
    pub fn new(model: CemModel) -> Self {
        let animator = CemAnimator::new(&model);
        Self { model, animator }
    }

    /// The parsed model.
    #[must_use]
    pub fn model(&self) -> &CemModel {
        &self.model
    }

    /// The texture the model draws its parts with, if the file names one.
    #[must_use]
    pub fn texture(&self) -> Option<&PackPath> {
        self.model.texture.as_ref()
    }

    /// Fresh per-entity animation state.
    #[must_use]
    pub fn new_state(&self, seed: u32) -> CemState {
        self.animator.new_state(seed)
    }

    /// Applies the model to the posed `vanilla` mob and returns the vertices
    /// of the custom parts, in the space `emit_mesh` of the vanilla models
    /// uses (blocks, origin at the feet).
    ///
    /// `rest` is the same mob model before any pose; its difference from
    /// `vanilla` is how far vanilla animation moved each part, which a
    /// replacing part keeps (see [`RootDelta`]). Parts that `vanilla` does
    /// not have are skipped. Replaced parts lose their boxes.
    pub fn apply<M: CemMob>(
        &self,
        vanilla: &mut M,
        rest: &M,
        state: &mut CemState,
        inputs: &EntityInputs,
        dt: f32,
    ) -> Vec<ModelVertex> {
        let root = Mat4::from_scale(Vec3::splat(1.0 / 16.0));
        let mut deltas = Vec::with_capacity(self.model.entries.len());
        let mut parents = Vec::with_capacity(self.model.entries.len());
        for entry in &self.model.entries {
            let part = vanilla.cem_part_mut(&entry.part);
            let (delta, parent) = match part {
                None => (RootDelta::default(), None),
                Some(part) if entry.attach => {
                    // The custom part hangs on the vanilla one, which already
                    // carries its animation.
                    (RootDelta::default(), Some(part.transform(root)))
                }
                Some(part) => {
                    let delta = rest
                        .cem_part(&entry.part)
                        .map_or_else(RootDelta::default, |rest| delta_between(part, rest));
                    part.boxes.clear();
                    (delta, Some(root))
                }
            };
            deltas.push(delta);
            parents.push(parent);
        }
        self.animator.evaluate(state, inputs, &deltas, dt);
        let mut out = Vec::new();
        let mut index = 0;
        for (entry, parent) in self.model.entries.iter().zip(parents) {
            // Each entry's parts are numbered whether or not it is drawn.
            let first = index;
            index += count_parts(&entry.root);
            if let Some(parent) = parent {
                let mut next = first;
                self.emit_part(
                    &entry.root,
                    &mut next,
                    parent,
                    entry.attach,
                    state,
                    &mut out,
                );
            }
        }
        out
    }

    fn emit_part(
        &self,
        part: &CemPart,
        index: &mut usize,
        parent: Mat4,
        nested: bool,
        state: &CemState,
        out: &mut Vec<ModelVertex>,
    ) {
        let pose = self.animator.pose(state, *index);
        *index += 1;
        let matrix = parent * local_matrix(&pose, nested);
        if pose.visible && pose.visible_boxes {
            for shape in &part.boxes {
                emit_box(shape, part, matrix, out);
            }
        }
        for child in &part.children {
            if pose.visible {
                self.emit_part(child, index, matrix, true, state, out);
            } else {
                *index += count_parts(child);
            }
        }
    }
}

fn count_parts(part: &CemPart) -> usize {
    1 + part.children.iter().map(count_parts).sum::<usize>()
}

/// The move of a vanilla part from its rest pose, in the space of the file:
/// y-up values back to y-down (see the module comment).
fn delta_between(posed: &ModelPart, rest: &ModelPart) -> RootDelta {
    let moved = |axis: usize| posed.pivot[axis] - rest.pivot[axis];
    let turned = |axis: usize| posed.rotation[axis] - rest.rotation[axis];
    RootDelta {
        translate: [moved(0), -moved(1), moved(2)],
        rotate: [-turned(0), turned(1), -turned(2)],
    }
}

/// The matrix of a part inside its parent, from a pose in the space of the
/// file. `nested` parts hang on another part and have no model height.
fn local_matrix(pose: &PartPose, nested: bool) -> Mat4 {
    // Offsets are in blocks in the reference; the model works in pixels.
    let at = |axis: usize| pose.translate[axis] + pose.offset[axis] * 16.0;
    let base = if nested { 0.0 } else { MODEL_HEIGHT };
    Mat4::from_translation(Vec3::new(at(0), base - at(1), at(2)))
        * Mat4::from_rotation_z(-pose.rotate[2])
        * Mat4::from_rotation_y(pose.rotate[1])
        * Mat4::from_rotation_x(-pose.rotate[0])
        * Mat4::from_scale(Vec3::from(pose.scale))
}

fn emit_box(shape: &CemBox, part: &CemPart, matrix: Mat4, out: &mut Vec<ModelVertex>) {
    let texture_size = part.texture_size.unwrap_or(DEFAULT_TEXTURE_SIZE);
    // The y flip: the box spans `-(y + h) .. -y` upwards.
    let origin = [
        shape.origin[0],
        -(shape.origin[1] + shape.size[1]),
        shape.origin[2],
    ];
    match shape.uv {
        BoxUv::Offset(uv) => {
            let mut model_box = ModelBox::new(origin, shape.size, uv, texture_size, shape.size_add);
            model_box.mirror = part.mirror_u;
            model_box.emit_triangles(matrix, out);
        }
        BoxUv::Faces(faces) => emit_face_box(origin, shape, texture_size, &faces, matrix, out),
    }
}

/// A box whose faces each take their own texture rectangle.
///
/// A rectangle `[u1, v1, u2, v2]` is read in image order (`u1` left, `v1`
/// top) for the side and top faces and for the bottom face too; swapping the
/// two coordinates of an axis flips the face. The corner order matches
/// [`ModelBox::emit_triangles`], so a box with the faces of the default
/// layout looks the same.
fn emit_face_box(
    origin: [f32; 3],
    shape: &CemBox,
    texture_size: [f32; 2],
    faces: &[Option<[f32; 4]>; 6],
    matrix: Mat4,
    out: &mut Vec<ModelVertex>,
) {
    let grow = shape.size_add;
    let lo = Vec3::from(origin) - Vec3::splat(grow);
    let hi = Vec3::from(origin) + Vec3::from(shape.size) + Vec3::splat(grow);
    let corner = |x: bool, y: bool, z: bool| {
        let pick = |high: bool, axis: usize| if high { hi[axis] } else { lo[axis] };
        matrix
            .transform_point3(Vec3::new(pick(x, 0), pick(y, 1), pick(z, 2)))
            .to_array()
    };
    let flip = matrix.determinant() < 0.0;
    let normal = |v: Vec3| matrix.transform_vector3(v).normalize_or_zero().to_array();
    // Corners in the order of the default layout, with the face normal.
    let layout = [
        (
            [
                corner(false, false, false),
                corner(true, false, false),
                corner(true, false, true),
                corner(false, false, true),
            ],
            normal(Vec3::NEG_Y),
            FaceCorners::Down,
        ),
        (
            [
                corner(false, true, true),
                corner(true, true, true),
                corner(true, true, false),
                corner(false, true, false),
            ],
            normal(Vec3::Y),
            FaceCorners::Up,
        ),
        (
            [
                corner(true, false, false),
                corner(false, false, false),
                corner(false, true, false),
                corner(true, true, false),
            ],
            normal(Vec3::NEG_Z),
            FaceCorners::Side,
        ),
        (
            [
                corner(false, false, true),
                corner(true, false, true),
                corner(true, true, true),
                corner(false, true, true),
            ],
            normal(Vec3::Z),
            FaceCorners::Side,
        ),
        (
            [
                corner(false, false, false),
                corner(false, false, true),
                corner(false, true, true),
                corner(false, true, false),
            ],
            normal(Vec3::NEG_X),
            FaceCorners::Side,
        ),
        (
            [
                corner(true, false, true),
                corner(true, false, false),
                corner(true, true, false),
                corner(true, true, true),
            ],
            normal(Vec3::X),
            FaceCorners::Side,
        ),
    ];
    for ((corners, n, kind), face) in layout.into_iter().zip(faces) {
        let Some([u1, v1, u2, v2]) = *face else {
            continue;
        };
        let (u1, u2) = (u1 / texture_size[0], u2 / texture_size[0]);
        let (v1, v2) = (v1 / texture_size[1], v2 / texture_size[1]);
        let uvs = kind.uvs([u1, v1, u2, v2]);
        if flip {
            emit_quad(
                out,
                [corners[3], corners[2], corners[1], corners[0]],
                n,
                [uvs[3], uvs[2], uvs[1], uvs[0]],
            );
        } else {
            emit_quad(out, corners, n, uvs);
        }
    }
}

/// How a face's rectangle lands on its four corners.
#[derive(Clone, Copy)]
enum FaceCorners {
    /// The bottom face is laid out upside down in the default layout.
    Down,
    Up,
    Side,
}

impl FaceCorners {
    fn uvs(self, [u1, v1, u2, v2]: [f32; 4]) -> [[f32; 2]; 4] {
        match self {
            Self::Down => [[u1, v2], [u2, v2], [u2, v1], [u1, v1]],
            Self::Up => [[u1, v1], [u2, v1], [u2, v2], [u1, v2]],
            Self::Side => [[u2, v2], [u1, v2], [u1, v1], [u2, v1]],
        }
    }
}

#[cfg(test)]
mod tests;

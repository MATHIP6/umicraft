// SPDX-License-Identifier: MIT OR Apache-2.0

//! Meshing of baked item models (P3).
//!
//! An item model is a list of [`ModelQuad`]s in block-local space. This
//! module places them with a pose matrix (see
//! [`umicraft_model::item::pose`]) and appends terrain [`Vertex`]es, so item
//! meshes draw through the terrain pipeline and its texture array: the item
//! sprites share the block atlas.
//!
//! The vertex normal stays in model space on purpose: the terrain shader
//! derives its 1.8 face shade (top 1.0, sides 0.8 / 0.6, bottom 0.5) from it,
//! which keeps a block icon lit the same whichever way the pose turns it
//! (the reference uses fixed GUI lights for the same effect).

use glam::{Mat4, Vec3};
use umicraft_model::{AtlasLayout, ModelQuad};

use crate::{tint, Vertex, VertexLight};

/// How the quads of one item are coloured and lit.
#[derive(Debug, Clone, Copy)]
pub struct ItemStyle<'a> {
    /// Texture key to atlas layer.
    pub atlas: &'a AtlasLayout,
    /// Light shared by every vertex (full light in the GUI, the world light
    /// at the position of a dropped item).
    pub light: VertexLight,
    /// `0x00RRGGBB` per tint index (the layer of a generated model); an index
    /// past the end, or an untinted quad, is white.
    pub tints: &'a [u32],
}

/// Vertices and triangle indices of one or more placed items.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemMesh {
    /// Four vertices per quad.
    pub vertices: Vec<Vertex>,
    /// Six indices per quad.
    pub indices: Vec<u32>,
}

impl ItemMesh {
    /// Removes the geometry and keeps the allocations for the next frame.
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
    }

    /// Whether the mesh has no quads.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Appends `quads` placed by `pose`.
    ///
    /// A pose that mirrors space (negative determinant, as the GUI chains
    /// do) reverses each quad's winding so back-face culling keeps the
    /// outside faces. A texture missing from the atlas uses layer 0, the
    /// missing-texture checker.
    pub fn push_quads(&mut self, quads: &[ModelQuad], pose: &Mat4, style: &ItemStyle<'_>) {
        let mirrored = pose.determinant() < 0.0;
        let light = style.light.pack();
        for quad in quads {
            let layer = style.atlas.layer(&quad.texture).unwrap_or(0);
            let tint = quad_tint(quad, style.tints);
            let base = self.vertices.len() as u32;
            self.vertices.extend((0..4).map(|corner| {
                Vertex {
                    position: pose
                        .transform_point3(Vec3::from(quad.positions[corner]))
                        .to_array(),
                    normal: quad.normal,
                    uv: quad.uvs[corner],
                    layer,
                    light,
                    tint,
                }
            }));
            let order = if mirrored {
                [0, 3, 2, 0, 2, 1]
            } else {
                [0, 1, 2, 0, 2, 3]
            };
            self.indices.extend(order.map(|offset| base + offset));
        }
    }
}

fn quad_tint(quad: &ModelQuad, tints: &[u32]) -> u32 {
    quad.tint
        .and_then(|index| tints.get(index as usize).copied())
        .unwrap_or(tint::WHITE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(tint: Option<u32>) -> ModelQuad {
        ModelQuad {
            positions: [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            normal: [0.0, 0.0, -1.0],
            uvs: [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            texture: "items/apple".to_string(),
            cullface: None,
            shade: true,
            tint,
        }
    }

    fn style<'a>(atlas: &'a AtlasLayout, tints: &'a [u32]) -> ItemStyle<'a> {
        ItemStyle {
            atlas,
            light: VertexLight::DAY,
            tints,
        }
    }

    #[test]
    fn places_vertices_with_the_pose_and_keeps_the_model_normal() {
        let atlas = AtlasLayout::from_keys(["items/apple"]);
        let mut mesh = ItemMesh::default();
        let pose = Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0));
        mesh.push_quads(&[quad(None)], &pose, &style(&atlas, &[]));
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.indices, [0, 1, 2, 0, 2, 3]);
        assert_eq!(mesh.vertices[0].position, [10.0, 0.0, 0.0]);
        assert_eq!(mesh.vertices[2].position, [11.0, 1.0, 0.0]);
        assert_eq!(mesh.vertices[0].normal, [0.0, 0.0, -1.0]);
        assert_eq!(mesh.vertices[0].layer, 1);
    }

    #[test]
    fn a_mirroring_pose_reverses_the_winding() {
        let atlas = AtlasLayout::from_keys(["items/apple"]);
        let mut mesh = ItemMesh::default();
        let pose = Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0));
        mesh.push_quads(&[quad(None)], &pose, &style(&atlas, &[]));
        assert_eq!(mesh.indices, [0, 3, 2, 0, 2, 1]);
    }

    #[test]
    fn tint_follows_the_quad_tint_index() {
        let atlas = AtlasLayout::from_keys(["items/apple"]);
        let mut mesh = ItemMesh::default();
        let tints = [0x00FF_0000, 0x0000_FF00];
        mesh.push_quads(
            &[quad(Some(1)), quad(Some(7)), quad(None)],
            &Mat4::IDENTITY,
            &style(&atlas, &tints),
        );
        assert_eq!(mesh.vertices[0].tint, 0x0000_FF00);
        assert_eq!(mesh.vertices[4].tint, tint::WHITE);
        assert_eq!(mesh.vertices[8].tint, tint::WHITE);
    }

    #[test]
    fn an_unknown_texture_uses_the_missing_layer_and_clear_keeps_capacity() {
        let atlas = AtlasLayout::from_keys(["blocks/stone"]);
        let mut mesh = ItemMesh::default();
        mesh.push_quads(&[quad(None)], &Mat4::IDENTITY, &style(&atlas, &[]));
        assert_eq!(mesh.vertices[0].layer, 0);
        let capacity = mesh.vertices.capacity();
        mesh.clear();
        assert!(mesh.is_empty());
        assert_eq!(mesh.vertices.capacity(), capacity);
    }
}

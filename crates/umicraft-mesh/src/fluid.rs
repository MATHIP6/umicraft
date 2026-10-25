// SPDX-License-Identifier: MIT OR Apache-2.0

//! Water and lava geometry, after MCP `BlockFluidRenderer.renderFluid`.
//!
//! A liquid block is not a cube: each corner of its surface sits at the
//! average fill of the four columns that share it (`getFluidHeight`), so
//! sources are 8/9 high, flowing liquid slopes down, and falling liquid fills
//! its block. The surface samples the still texture, or the flowing texture
//! turned along the current (`BlockLiquid.getFlowDirection`); the sides
//! sample the left half of the flowing texture up to the corner heights.
//!
//! Faces are drawn only towards blocks that are neither the same liquid nor
//! opaque cubes (the surface: whenever the block above is not the same
//! liquid), and from both sides, like the reference. The reference lights a
//! liquid face flat with the brighter of the block and the block above it and
//! multiplies its colour by a fixed factor per face (top 1, bottom 0.5,
//! north/south 0.8, west/east 0.6); both are reproduced here instead of the
//! smooth lighting and the shader's face shade of the other blocks.

use umicraft_world::fluid::{self, Liquid};
use umicraft_world::{BlockPos, BlockState, LightLevel};

use crate::{BlockSource, Mesh, Vertex, VertexLight};

/// The texture layers of one liquid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FluidLayers {
    /// The still texture (surface without current, bottom).
    pub still: u32,
    /// The flowing texture (surface with a current, sides).
    pub flow: u32,
}

/// How far the faces are pulled into the block (`f11 = 0.001F`), so the
/// surface does not z-fight a block on top and the sides a neighbour.
const INSET: f32 = 0.001;

/// The colour factor of the bottom face (`f3 = 0.5F`).
const BOTTOM_SHADE: f32 = 0.5;

/// The colour factor of the north and south faces (`f5 = 0.8F`).
const NORTH_SOUTH_SHADE: f32 = 0.8;

/// The colour factor of the west and east faces (`f6 = 0.6F`).
const WEST_EAST_SHADE: f32 = 0.6;

/// Emits the faces of one liquid block at `position` into `mesh`.
///
/// `tint` is the biome colour of the block (`0x00RRGGBB`, white for lava).
/// Water goes to the transparent pass, lava to the opaque one
/// (`BlockLiquid.getBlockLayer`).
pub(crate) fn emit_fluid<S: BlockSource + ?Sized>(
    mesh: &mut Mesh,
    source: &S,
    position: [i32; 3],
    liquid: Liquid,
    layers: FluidLayers,
    tint: u32,
) {
    let [x, y, z] = position;
    let at = |dx: i32, dy: i32, dz: i32| source.block(x + dx, y + dy, z + dz);
    let transparent = liquid == Liquid::Water;

    // `shouldSideBeRendered`: never towards the same liquid; the surface
    // otherwise always, the other faces when the neighbour is not opaque.
    let visible = |neighbour: BlockState, top: bool| {
        !liquid.contains(neighbour) && (top || !neighbour.is_opaque())
    };

    // The surface corners: north-west, south-west, south-east, north-east.
    let height = |dx: i32, dz: i32| fluid_height(source, liquid, [x + dx, y, z + dz]);
    let mut heights = [height(0, 0), height(0, 1), height(1, 1), height(1, 0)];

    if visible(at(0, 1, 0), true) {
        for corner in &mut heights {
            *corner -= INSET;
        }
        emit_surface(
            mesh,
            source,
            position,
            liquid,
            layers,
            tint,
            heights,
            transparent,
        );
    }
    if visible(at(0, -1, 0), false) {
        emit_bottom(mesh, source, position, layers.still, tint, transparent);
    }
    // North, south, west, east as in the reference: `(dx, dz)`, the two
    // corner heights left to right seen from outside, and the two ends.
    let [north_west, south_west, south_east, north_east] = heights;
    let sides = [
        ((0, -1), north_west, north_east, [0.0, INSET], [1.0, INSET]),
        (
            (0, 1),
            south_east,
            south_west,
            [1.0, 1.0 - INSET],
            [0.0, 1.0 - INSET],
        ),
        ((-1, 0), south_west, north_west, [INSET, 1.0], [INSET, 0.0]),
        (
            (1, 0),
            north_east,
            south_east,
            [1.0 - INSET, 0.0],
            [1.0 - INSET, 1.0],
        ),
    ];
    for ((dx, dz), left_height, right_height, left, right) in sides {
        if !visible(at(dx, 0, dz), false) {
            continue;
        }
        let shade = if dz != 0 {
            NORTH_SOUTH_SHADE
        } else {
            WEST_EAST_SHADE
        };
        let light = flat_light(source, [x + dx, y, z + dz]);
        let corner = |[cx, cz]: [f32; 2], cy: f32| [x as f32 + cx, y as f32 + cy, z as f32 + cz];
        let v = |h: f32| (1.0 - h) * 0.5;
        let quad = [
            (corner(left, left_height), [0.0, v(left_height)]),
            (corner(right, right_height), [0.5, v(right_height)]),
            (corner(right, 0.0), [0.5, 0.5]),
            (corner(left, 0.0), [0.0, 0.5]),
        ];
        let normal = [dx as f32, 0.0, dz as f32];
        push_both_sides(
            mesh,
            transparent,
            quad,
            normal,
            layers.flow,
            light,
            shaded(tint, shade),
        );
    }
}

/// The surface, with the still texture or the flowing one turned along the
/// current; its underside is added when a non-full block borders the space
/// above (`BlockLiquid.func_176364_g`), so the surface stays visible from
/// below.
#[allow(clippy::too_many_arguments)]
fn emit_surface<S: BlockSource + ?Sized>(
    mesh: &mut Mesh,
    source: &S,
    position: [i32; 3],
    liquid: Liquid,
    layers: FluidLayers,
    tint: u32,
    heights: [f32; 4],
    transparent: bool,
) {
    let [x, y, z] = position;
    let at = |pos: BlockPos| source.block(pos.x, pos.y, pos.z);
    let [flow_x, _, flow_z] = fluid::flow_vector(liquid, BlockPos::new(x, y, z), at);
    let (layer, uvs) = if flow_x == 0.0 && flow_z == 0.0 {
        (
            layers.still,
            [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
        )
    } else {
        // `getFlowDirection`: the angle of the current minus a quarter turn,
        // then the UVs of the reference, in sixteenths around the centre.
        let angle = (flow_z.atan2(flow_x) - std::f64::consts::FRAC_PI_2) as f32;
        let (sin, cos) = (angle.sin() * 0.25, angle.cos() * 0.25);
        let uv = |u: f32, v: f32| [(8.0 + u * 16.0) / 16.0, (8.0 + v * 16.0) / 16.0];
        (
            layers.flow,
            [
                uv(-cos - sin, -cos + sin),
                uv(-cos + sin, cos + sin),
                uv(cos + sin, cos - sin),
                uv(cos - sin, -cos - sin),
            ],
        )
    };
    let corner = |cx: f32, h: f32, cz: f32| [x as f32 + cx, y as f32 + h, z as f32 + cz];
    let quad = [
        (corner(0.0, heights[0], 0.0), uvs[0]),
        (corner(0.0, heights[1], 1.0), uvs[1]),
        (corner(1.0, heights[2], 1.0), uvs[2]),
        (corner(1.0, heights[3], 0.0), uvs[3]),
    ];
    let light = flat_light(source, position);
    let underside = (-1..=1).any(|dx| {
        (-1..=1).any(|dz| {
            let block = source.block(x + dx, y + 1, z + dz);
            !liquid.contains(block) && !block.is_opaque()
        })
    });
    let normal = [0.0, 1.0, 0.0];
    if underside {
        push_both_sides(mesh, transparent, quad, normal, layer, light, tint);
    } else {
        push(mesh, transparent, quad, normal, layer, light, tint);
    }
}

/// The bottom, with the still texture, lit by the block below.
fn emit_bottom<S: BlockSource + ?Sized>(
    mesh: &mut Mesh,
    source: &S,
    position: [i32; 3],
    layer: u32,
    tint: u32,
    transparent: bool,
) {
    let [x, y, z] = position;
    let corner = |cx: f32, cz: f32| [x as f32 + cx, y as f32, z as f32 + cz];
    let quad = [
        (corner(0.0, 1.0), [0.0, 1.0]),
        (corner(0.0, 0.0), [0.0, 0.0]),
        (corner(1.0, 0.0), [1.0, 0.0]),
        (corner(1.0, 1.0), [1.0, 1.0]),
    ];
    let light = flat_light(source, [x, y - 1, z]);
    let tint = shaded(tint, BOTTOM_SHADE);
    push(
        mesh,
        transparent,
        quad,
        [0.0, -1.0, 0.0],
        layer,
        light,
        tint,
    );
}

/// `BlockFluidRenderer.getFluidHeight`: the fill of the corner shared by the
/// columns at `(x - 1..=x, z - 1..=z)`. A column with the same liquid above
/// fills the corner; a source or falling block weighs eleven times; a
/// non-solid neighbour counts as empty; a solid one is ignored.
fn fluid_height<S: BlockSource + ?Sized>(source: &S, liquid: Liquid, [x, y, z]: [i32; 3]) -> f32 {
    let mut emptiness = 0.0_f32;
    let mut weight = 0_u32;
    for (dx, dz) in [(0, 0), (-1, 0), (0, -1), (-1, -1)] {
        if liquid.contains(source.block(x + dx, y + 1, z + dz)) {
            return 1.0;
        }
        let block = source.block(x + dx, y, z + dz);
        if liquid.contains(block) {
            let level = block.metadata();
            let empty = fluid::height_percent(level);
            if level >= 8 || level == 0 {
                emptiness += empty * 10.0;
                weight += 10;
            }
            emptiness += empty;
            weight += 1;
        } else if !fluid::is_solid_material(block) {
            emptiness += 1.0;
            weight += 1;
        }
    }
    1.0 - emptiness / weight as f32
}

/// `BlockLiquid.getMixedBrightnessForBlock`: the brighter of the block and
/// the block above it, per channel, without smoothing or occlusion.
fn flat_light<S: BlockSource + ?Sized>(source: &S, [x, y, z]: [i32; 3]) -> VertexLight {
    let here: LightLevel = source.light(x, y, z);
    let above: LightLevel = source.light(x, y + 1, z);
    VertexLight::new(
        here.block.max(above.block),
        here.sky.max(above.sky),
        VertexLight::AO_OPEN,
        false,
    )
}

/// A `0x00RRGGBB` colour scaled by a face factor.
fn shaded(tint: u32, factor: f32) -> u32 {
    let channel = |shift: u32| {
        let value = f32::from(((tint >> shift) & 0xFF) as u8) * factor;
        (value.round() as u32).min(255) << shift
    };
    channel(16) | channel(8) | channel(0)
}

/// One quad with its corners, in counter-clockwise order from the front.
type Quad = [([f32; 3], [f32; 2]); 4];

/// Pushes a quad facing `normal`.
fn push(
    mesh: &mut Mesh,
    transparent: bool,
    quad: Quad,
    normal: [f32; 3],
    layer: u32,
    light: VertexLight,
    tint: u32,
) {
    let light = light.pack();
    let vertices = quad.map(|(position, uv)| Vertex {
        position,
        normal,
        uv,
        layer,
        light,
        tint,
    });
    mesh.push_quad(transparent, vertices);
}

/// Pushes a quad and its back side, as the reference draws liquid faces.
fn push_both_sides(
    mesh: &mut Mesh,
    transparent: bool,
    quad: Quad,
    normal: [f32; 3],
    layer: u32,
    light: VertexLight,
    tint: u32,
) {
    push(mesh, transparent, quad, normal, layer, light, tint);
    let [a, b, c, d] = quad;
    let back = normal.map(|component| -component);
    push(mesh, transparent, [d, c, b, a], back, layer, light, tint);
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// A block source backed by a map; everything else is air in daylight.
    #[derive(Default)]
    struct Blocks(HashMap<[i32; 3], BlockState>);

    impl Blocks {
        fn with(blocks: &[([i32; 3], BlockState)]) -> Self {
            Self(blocks.iter().copied().collect())
        }
    }

    impl BlockSource for Blocks {
        fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
            self.0.get(&[x, y, z]).copied().unwrap_or(BlockState::AIR)
        }
    }

    const WATER: BlockState = BlockState::new(9, 0);
    const LAVA: BlockState = BlockState::new(11, 0);
    const STONE: BlockState = BlockState::new(1, 0);
    const LAYERS: FluidLayers = FluidLayers { still: 1, flow: 2 };

    fn mesh(blocks: &Blocks, position: [i32; 3], liquid: Liquid) -> Mesh {
        let mut mesh = Mesh::default();
        emit_fluid(&mut mesh, blocks, position, liquid, LAYERS, 0x00FF_FFFF);
        mesh
    }

    /// The highest vertex of the upward faces, relative to the block.
    fn surface_corners(mesh: &Mesh) -> Vec<f32> {
        let vertices = mesh.transparent_vertices.iter().chain(&mesh.vertices);
        vertices
            .filter(|vertex| vertex.normal == [0.0, 1.0, 0.0])
            .map(|vertex| vertex.position[1])
            .collect()
    }

    /// A lake of sources over stone, 3×3 around the origin.
    fn lake(extra: &[([i32; 3], BlockState)]) -> Blocks {
        let mut blocks = Vec::new();
        for x in -1..=1 {
            for z in -1..=1 {
                blocks.push(([x, -1, z], STONE));
                blocks.push(([x, 0, z], WATER));
            }
        }
        blocks.extend_from_slice(extra);
        Blocks::with(&blocks)
    }

    /// Regression: liquids were full cubes. A source surrounded by sources
    /// sits at 8/9 (`getFluidHeight`: four sources, each weighing eleven).
    #[test]
    fn a_source_inside_a_lake_is_eight_ninths_high() {
        let corners = surface_corners(&mesh(&lake(&[]), [0, 0, 0], Liquid::Water));
        assert_eq!(corners.len(), 4, "only the surface, no underside");
        for corner in corners {
            assert!((corner - (8.0 / 9.0 - INSET)).abs() < 1e-5, "{corner}");
        }
    }

    #[test]
    fn liquid_under_the_same_liquid_has_no_surface_and_fills_its_block() {
        let mesh = mesh(&lake(&[([0, 1, 0], WATER)]), [0, 0, 0], Liquid::Water);
        assert!(surface_corners(&mesh).is_empty());
        assert!(mesh.is_empty(), "every side touches water or stone");
    }

    /// A flowing block slopes towards its lower neighbours and samples the
    /// flowing texture turned along the current.
    #[test]
    fn flowing_water_slopes_and_turns_the_flow_texture() {
        let blocks = Blocks::with(&[
            ([0, -1, 0], STONE),
            ([1, -1, 0], STONE),
            ([-1, -1, 0], STONE),
            ([-1, 0, 0], BlockState::new(8, 1)),
            ([0, 0, 0], BlockState::new(8, 2)),
            ([1, 0, 0], BlockState::new(8, 3)),
        ]);
        let mesh = mesh(&blocks, [0, 0, 0], Liquid::Water);
        let surface: Vec<&Vertex> = mesh
            .transparent_vertices
            .iter()
            .filter(|vertex| vertex.normal == [0.0, 1.0, 0.0])
            .collect();
        assert!(surface.iter().all(|vertex| vertex.layer == LAYERS.flow));
        // The west corners are higher than the east ones (it flows east).
        let west = surface
            .iter()
            .find(|v| v.position[0] == 0.0)
            .map(|v| v.position[1]);
        let east = surface
            .iter()
            .find(|v| v.position[0] == 1.0)
            .map(|v| v.position[1]);
        assert!(west > east, "west {west:?}, east {east:?}");
        // Flowing east: angle 0 - pi/2, so the first corner samples
        // (8 + (-cos - sin) * 16) / 16 with sin = -0.25, cos = 0.
        let first = surface[0].uv;
        assert!(
            (first[0] - 0.75).abs() < 1e-5 && (first[1] - 0.25).abs() < 1e-5,
            "{first:?}"
        );
    }

    #[test]
    fn faces_towards_opaque_blocks_and_the_same_liquid_are_hidden() {
        let blocks = Blocks::with(&[
            ([0, 0, 0], WATER),
            ([0, -1, 0], STONE),
            ([1, 0, 0], STONE),
            ([-1, 0, 0], WATER),
            ([0, 0, 1], BlockState::new(20, 0)),
        ]);
        let mesh = mesh(&blocks, [0, 0, 0], Liquid::Water);
        let vertices = &mesh.transparent_vertices;
        // No bottom face on the stone floor (the surface's underside faces
        // down too, but it sits at the surface, not at y = 0).
        assert!(!vertices
            .iter()
            .any(|v| v.normal == [0.0, -1.0, 0.0] && v.position[1] == 0.0));
        // Neither the face towards the stone (east) nor towards the water
        // (west) exists, from either side.
        assert!(!vertices.iter().any(|v| v.normal[0] != 0.0));
        // The glass is not opaque: the south face exists, from both sides.
        assert!(vertices.iter().any(|v| v.normal == [0.0, 0.0, 1.0]));
        assert!(vertices.iter().any(|v| v.normal == [0.0, 0.0, -1.0]));
    }

    #[test]
    fn lava_is_opaque_and_water_is_tinted_and_shaded() {
        let lava = mesh(
            &Blocks::with(&[([0, 0, 0], LAVA), ([0, -1, 0], STONE)]),
            [0, 0, 0],
            Liquid::Lava,
        );
        assert!(lava.transparent_vertices.is_empty());
        assert!(!lava.vertices.is_empty());

        let water = Blocks::with(&[([0, 0, 0], WATER), ([0, -1, 0], STONE)]);
        let mut mesh = Mesh::default();
        emit_fluid(
            &mut mesh,
            &water,
            [0, 0, 0],
            Liquid::Water,
            LAYERS,
            0x0040_80C0,
        );
        let tint_of = |normal: [f32; 3]| {
            mesh.transparent_vertices
                .iter()
                .find(|vertex| vertex.normal == normal)
                .map(|vertex| vertex.tint)
        };
        assert_eq!(tint_of([0.0, 1.0, 0.0]), Some(0x0040_80C0));
        assert_eq!(tint_of([0.0, 0.0, -1.0]), Some(shaded(0x0040_80C0, 0.8)));
        assert_eq!(tint_of([1.0, 0.0, 0.0]), Some(shaded(0x0040_80C0, 0.6)));
        assert!(mesh
            .transparent_vertices
            .iter()
            .all(|vertex| !VertexLight::unpack(vertex.light).shade));
    }
}

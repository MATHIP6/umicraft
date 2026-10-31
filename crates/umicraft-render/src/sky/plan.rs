// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the sky pass draws this frame, as plain data.
//!
//! [`build`] turns a [`SkyState`], the dimension and the eye height into the
//! ordered layers of `RenderGlobal.renderSky` / `renderSkyEnd`, each with its
//! blend, texture, mesh and uniform. Keeping it free of wgpu makes the draw
//! order and the dimension rules testable without a GPU.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec4};

use super::celestial;
use super::dome;
use super::state::{SkyKind, SkyState};

/// Maximum layers one frame can hold: dome, fan, sun, moon, stars, black
/// plane, black box and void plane (the End needs one).
pub(super) const MAX_LAYERS: usize = 8;
/// Bytes between two uniform slots (the dynamic-offset alignment).
pub(super) const SLOT_STRIDE: usize = 256;

/// How a layer is blended into the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Blend {
    /// Replaces the destination (the reference draws these with blending off).
    Opaque,
    /// `SRC_ALPHA, ONE_MINUS_SRC_ALPHA` (`tryBlendFuncSeparate(770, 771, 1, 0)`).
    Alpha,
    /// `SRC_ALPHA, ONE` (`tryBlendFuncSeparate(770, 1, 1, 0)`).
    Additive,
}

/// The image a layer samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Image {
    /// A white texel: untextured geometry.
    White,
    /// `environment/sun.png`.
    Sun,
    /// `environment/moon_phases.png`.
    Moon,
    /// `environment/end_sky.png`.
    EndSky,
}

/// The mesh a layer draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mesh {
    /// The 13 x 13 dome grid.
    Grid,
    /// The sunrise / sunset fan (rewritten each frame).
    Fan,
    /// The sun quad.
    Sun,
    /// The moon quad of a phase.
    Moon(u8),
    /// The star field.
    Stars,
    /// The black prism around the eye.
    VoidBox,
    /// The End cube.
    EndCube,
}

/// One uniform slot, padded to [`SLOT_STRIDE`]; mirrors `Slot` in `sky.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(super) struct SlotUniform {
    pub view_proj: [[f32; 4]; 4],
    pub model: [[f32; 4]; 4],
    pub tint: [f32; 4],
    pub fog_color: [f32; 4],
    /// `x` fog end, `y` fog on, `z` textured.
    pub params: [f32; 4],
    pub _pad: [f32; 20],
}

/// One draw of the pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Layer {
    pub blend: Blend,
    pub image: Image,
    pub mesh: Mesh,
    pub uniform: SlotUniform,
}

/// The inputs of one frame's plan.
pub(super) struct PlanInput<'a> {
    /// The dimension's sky.
    pub kind: SkyKind,
    /// Colours and factors of the frame.
    pub state: &'a SkyState,
    /// Projection times the camera rotation (no translation).
    pub view_proj: Mat4,
    /// Eye height in world blocks.
    pub eye_y: f32,
    /// Distance at which the sky fog is complete, in blocks.
    pub fog_end: f32,
}

/// Appends the layers of the frame to `layers` (cleared first), in draw order.
pub(super) fn build(layers: &mut Vec<Layer>, input: &PlanInput<'_>) {
    layers.clear();
    match input.kind {
        SkyKind::Nether => {}
        SkyKind::End => layers.push(end_layer(input)),
        SkyKind::Overworld => overworld_layers(layers, input),
    }
}

/// `renderSkyEnd`: the textured cube, alpha-blended, without fog.
fn end_layer(input: &PlanInput<'_>) -> Layer {
    layer(
        input,
        Blend::Alpha,
        Image::EndSky,
        Mesh::EndCube,
        Mat4::IDENTITY,
        [1.0; 4],
        false,
    )
}

/// The surface sky in the order of `renderSky`.
fn overworld_layers(layers: &mut Vec<Layer>, input: &PlanInput<'_>) {
    let state = input.state;
    let [r, g, b] = state.sky_color;
    layers.push(layer(
        input,
        Blend::Opaque,
        Image::White,
        Mesh::Grid,
        dome::sky_model(),
        [r, g, b, 1.0],
        true,
    ));
    if state.sunrise.is_some() {
        let model = celestial::sunrise_model(state.celestial_angle);
        layers.push(layer(
            input,
            Blend::Alpha,
            Image::White,
            Mesh::Fan,
            model,
            [1.0; 4],
            false,
        ));
    }
    let turn = celestial::sky_turn(state.celestial_angle);
    let dim = [1.0, 1.0, 1.0, state.rain_dim];
    layers.push(layer(
        input,
        Blend::Additive,
        Image::Sun,
        Mesh::Sun,
        turn,
        dim,
        false,
    ));
    layers.push(layer(
        input,
        Blend::Additive,
        Image::Moon,
        Mesh::Moon(state.moon_phase),
        turn,
        dim,
        false,
    ));
    let stars = state.star_brightness * state.rain_dim;
    if stars > 0.0 {
        layers.push(layer(
            input,
            Blend::Additive,
            Image::White,
            Mesh::Stars,
            turn,
            [stars; 4],
            false,
        ));
    }
    void_layers(layers, input);
}

/// The void below the horizon: the black plane and box while the eye is under
/// it, then the dark sky-coloured plane on the horizon.
fn void_layers(layers: &mut Vec<Layer>, input: &PlanInput<'_>) {
    let below = dome::below_horizon(input.eye_y);
    let black = [0.0, 0.0, 0.0, 1.0];
    if below < 0.0 {
        layers.push(layer(
            input,
            Blend::Opaque,
            Image::White,
            Mesh::Grid,
            dome::black_model(),
            black,
            true,
        ));
        layers.push(layer(
            input,
            Blend::Opaque,
            Image::White,
            Mesh::VoidBox,
            dome::box_model(below),
            black,
            true,
        ));
    }
    let [r, g, b] = input.state.sky_color;
    let tint = [r * 0.2 + 0.04, g * 0.2 + 0.04, b * 0.6 + 0.1, 1.0];
    layers.push(layer(
        input,
        Blend::Opaque,
        Image::White,
        Mesh::Grid,
        dome::void_model(below),
        tint,
        true,
    ));
}

/// Builds one layer; `fogged` switches the linear fog on for the draw.
fn layer(
    input: &PlanInput<'_>,
    blend: Blend,
    image: Image,
    mesh: Mesh,
    model: Mat4,
    tint: [f32; 4],
    fogged: bool,
) -> Layer {
    let [fog_r, fog_g, fog_b] = input.state.fog_color;
    let textured = image != Image::White;
    Layer {
        blend,
        image,
        mesh,
        uniform: SlotUniform {
            view_proj: input.view_proj.to_cols_array_2d(),
            model: model.to_cols_array_2d(),
            tint,
            fog_color: Vec4::new(fog_r, fog_g, fog_b, 1.0).to_array(),
            params: [
                input.fog_end,
                f32::from(u8::from(fogged)),
                f32::from(u8::from(textured)),
                0.0,
            ],
            _pad: [0.0; 20],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RenderEnvironment;

    fn plan(environment: RenderEnvironment, eye_y: f32) -> Vec<Layer> {
        let state = SkyState::at(environment);
        let input = PlanInput {
            kind: SkyKind::of(environment.dimension),
            state: &state,
            view_proj: Mat4::IDENTITY,
            eye_y,
            fog_end: 128.0,
        };
        let mut layers = Vec::new();
        build(&mut layers, &input);
        layers
    }

    fn meshes(layers: &[Layer]) -> Vec<Mesh> {
        layers.iter().map(|l| l.mesh).collect()
    }

    #[test]
    fn the_slot_matches_the_wgsl_layout() {
        assert_eq!(std::mem::offset_of!(SlotUniform, model), 64);
        assert_eq!(std::mem::offset_of!(SlotUniform, tint), 128);
        assert_eq!(std::mem::offset_of!(SlotUniform, params), 160);
        assert_eq!(std::mem::size_of::<SlotUniform>(), SLOT_STRIDE);
    }

    #[test]
    fn noon_draws_dome_bodies_and_the_horizon_plane_but_no_stars() {
        let layers = plan(RenderEnvironment::DEFAULT, 80.0);
        assert_eq!(
            meshes(&layers),
            [Mesh::Grid, Mesh::Sun, Mesh::Moon(0), Mesh::Grid]
        );
        assert!(layers.len() <= MAX_LAYERS);
        assert_eq!(layers[1].blend, Blend::Additive);
        assert_eq!(layers[0].blend, Blend::Opaque);
    }

    #[test]
    fn night_adds_the_stars_and_sunrise_adds_the_fan_before_the_bodies() {
        let night = plan(RenderEnvironment::new(0, 18_000, 0.0, 8), 80.0);
        assert!(meshes(&night).contains(&Mesh::Stars));
        let sunrise = plan(RenderEnvironment::new(0, 23_000, 0.0, 8), 80.0);
        let order = meshes(&sunrise);
        let fan = order.iter().position(|m| *m == Mesh::Fan).expect("fan");
        let sun = order.iter().position(|m| *m == Mesh::Sun).expect("sun");
        assert!(fan < sun);
        assert_eq!(sunrise[fan].blend, Blend::Alpha);
    }

    #[test]
    fn rain_dims_the_bodies_and_hides_the_stars_in_full_rain() {
        let rain = RenderEnvironment {
            rain_strength: 1.0,
            world_time: 18_000,
            ..RenderEnvironment::DEFAULT
        };
        let layers = plan(rain, 80.0);
        assert!(!meshes(&layers).contains(&Mesh::Stars));
        assert_eq!(layers[1].uniform.tint[3], 0.0);
    }

    #[test]
    fn an_eye_below_the_horizon_adds_the_black_plane_and_box() {
        let layers = plan(RenderEnvironment::DEFAULT, 40.0);
        let order = meshes(&layers);
        assert_eq!(order.iter().filter(|m| **m == Mesh::VoidBox).count(), 1);
        assert_eq!(order.iter().filter(|m| **m == Mesh::Grid).count(), 3);
        // The coloured horizon plane is last.
        assert_ne!(
            layers.last().map(|l| l.uniform.tint),
            Some([0.0, 0.0, 0.0, 1.0])
        );
        assert!(layers.len() <= MAX_LAYERS);
    }

    #[test]
    fn the_nether_draws_nothing_and_the_end_draws_its_cube() {
        assert!(plan(RenderEnvironment::new(-1, 0, 0.0, 8), 80.0).is_empty());
        let end = plan(RenderEnvironment::new(1, 0, 0.0, 8), 80.0);
        assert_eq!(meshes(&end), [Mesh::EndCube]);
        assert_eq!(end[0].blend, Blend::Alpha);
        assert_eq!(end[0].image, Image::EndSky);
        assert_eq!(end[0].uniform.params[1], 0.0, "no fog on the cube");
    }

    #[test]
    fn only_dome_layers_are_fogged_and_only_textured_images_sample() {
        let layers = plan(RenderEnvironment::DEFAULT, 80.0);
        assert_eq!(layers[0].uniform.params[..3], [128.0, 1.0, 0.0]);
        assert_eq!(layers[1].uniform.params[..3], [128.0, 0.0, 1.0]);
    }
}

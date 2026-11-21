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
use super::custom;
use super::dome;
use super::state::{SkyKind, SkyState};
use crate::SkyDetails;
use umicraft_packext::sky::LayerFrame;

/// Maximum layers one frame can hold: dome, fan, sun, moon, stars, black
/// plane, black box and void plane (the End needs one).
pub(super) const MAX_LAYERS: usize = 8;
/// Custom sky layers drawn in one frame; further ones are dropped.
pub(super) const MAX_CUSTOM_LAYERS: usize = 32;
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
    /// `ONE_MINUS_DST_COLOR, ZERO` (custom sky `subtract`).
    Subtract,
    /// `DST_COLOR, ONE_MINUS_SRC_ALPHA` (custom sky `multiply`).
    Multiply,
    /// `ONE, ONE` (custom sky `dodge`).
    Dodge,
    /// `ZERO, ONE_MINUS_SRC_COLOR` (custom sky `burn`).
    Burn,
    /// `ONE, ONE_MINUS_SRC_COLOR` (custom sky `screen`).
    Screen,
    /// `DST_COLOR, SRC_COLOR` (custom sky `overlay`).
    Overlay,
}

impl Blend {
    /// Every mode, in declaration order: the index of its pipeline.
    pub(super) const ALL: [Self; 9] = [
        Self::Opaque,
        Self::Alpha,
        Self::Additive,
        Self::Subtract,
        Self::Multiply,
        Self::Dodge,
        Self::Burn,
        Self::Screen,
        Self::Overlay,
    ];
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
    /// The picture of custom sky layer `n` (a flat index over the plan).
    Custom(u16),
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
    /// The cube of a custom sky layer.
    Cube,
}

/// One uniform slot, padded to [`SLOT_STRIDE`]; mirrors `Slot` in `sky.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(super) struct SlotUniform {
    pub view_proj: [[f32; 4]; 4],
    pub model: [[f32; 4]; 4],
    pub tint: [f32; 4],
    pub fog_color: [f32; 4],
    /// `x` textured, `y` fog on.
    pub params: [f32; 4],
    /// `x` fog start, `y` fog end, `z` exponential density (see `fog.wgsl`).
    pub fog: [f32; 4],
    pub _pad: [f32; 16],
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
    /// Whether the world is a superflat one, whose horizon is at `y = 0`.
    pub flat_world: bool,
    /// Which parts of the sky the user wants drawn.
    pub details: SkyDetails,
    /// The custom sky layers to draw this frame, in order.
    pub custom: &'a [LayerFrame],
    /// Whether the dimension has custom layers, which replace the stars
    /// (`!CustomSky.hasSkyLayers`) even while all of them are faded out.
    pub custom_replaces_stars: bool,
}

/// Appends the layers of the frame to `layers` (cleared first), in draw order.
pub(super) fn build(layers: &mut Vec<Layer>, input: &PlanInput<'_>) {
    layers.clear();
    match input.kind {
        SkyKind::Nether => {}
        SkyKind::End => {
            if input.details.sky {
                layers.push(end_layer(input));
            }
        }
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
    let details = input.details;
    if details.sky {
        layers.push(layer(
            input,
            Blend::Opaque,
            Image::White,
            Mesh::Grid,
            dome::sky_model(),
            [r, g, b, 1.0],
            true,
        ));
    }
    // OptiFine gates the sunrise fan with Sun & Moon, not with Sky
    // (`RenderGlobal.renderSky`, `isSunMoonEnabled` around the fan).
    if details.sun_moon && state.sunrise.is_some() {
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
    custom_layers(layers, input);
    let turn = celestial::sky_turn(state.celestial_angle);
    let dim = [1.0, 1.0, 1.0, state.rain_dim];
    if details.sun_moon {
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
    }
    let stars = state.star_brightness * state.rain_dim;
    if details.stars && stars > 0.0 && !input.custom_replaces_stars {
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

/// The pack's sky layers, between the sunrise fan and the sun
/// (`CustomSky.renderSky` runs there). The tint carries the brightness the
/// way the reference's `GlStateManager.color` does for each blend mode.
fn custom_layers(layers: &mut Vec<Layer>, input: &PlanInput<'_>) {
    for frame in input.custom.iter().take(MAX_CUSTOM_LAYERS) {
        let (blend, alpha_test) = custom::blend(frame.blend);
        let Ok(index) = u16::try_from(frame.index) else {
            continue;
        };
        let mut drawn = layer(
            input,
            blend,
            Image::Custom(index),
            Mesh::Cube,
            custom::model(frame.rotation, frame.axis),
            frame.blend.color(frame.brightness),
            false,
        );
        if alpha_test {
            drawn.uniform.params[2] = custom::ALPHA_TEST;
        }
        layers.push(drawn);
    }
}

/// The void below the horizon: the black plane and box while the eye is under
/// it, then the dark sky-coloured plane on the horizon.
fn void_layers(layers: &mut Vec<Layer>, input: &PlanInput<'_>) {
    let below = dome::below_horizon(input.eye_y, input.flat_world);
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
    // Sky off also drops this horizon plane (`glSkyList2` under
    // `isSkyEnabled`); the black plane and box above stay, as in OptiFine.
    if !input.details.sky {
        return;
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
    let [fog_r, fog_g, fog_b] = input.state.fog.color;
    let [fog_start, fog_end, fog_density] = input.state.fog.sky.uniform();
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
                f32::from(u8::from(textured)),
                f32::from(u8::from(fogged)),
                0.0,
                0.0,
            ],
            fog: [
                fog_start,
                fog_end,
                fog_density,
                f32::from(u8::from(input.state.fog.planar)),
            ],
            _pad: [0.0; 16],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RenderEnvironment, SkyDetails};

    fn plan(environment: RenderEnvironment, eye_y: f32) -> Vec<Layer> {
        let state = SkyState::at(environment);
        let input = PlanInput {
            kind: SkyKind::of(environment.dimension),
            state: &state,
            view_proj: Mat4::IDENTITY,
            eye_y,
            flat_world: environment.flat_world,
            details: environment.sky_details,
            custom: &[],
            custom_replaces_stars: false,
        };
        let mut layers = Vec::new();
        build(&mut layers, &input);
        layers
    }

    fn meshes(layers: &[Layer]) -> Vec<Mesh> {
        layers.iter().map(|l| l.mesh).collect()
    }

    fn plan_with(details: SkyDetails, environment: RenderEnvironment) -> Vec<Mesh> {
        meshes(&plan(
            RenderEnvironment {
                sky_details: details,
                ..environment
            },
            80.0,
        ))
    }

    const NIGHT: RenderEnvironment = RenderEnvironment {
        world_time: 18_000,
        ..RenderEnvironment::DEFAULT
    };

    #[test]
    fn every_part_is_drawn_by_default() {
        let drawn = plan_with(SkyDetails::ALL, NIGHT);
        assert!(drawn.contains(&Mesh::Stars));
        assert!(drawn.contains(&Mesh::Sun));
        assert!(drawn.iter().any(|mesh| matches!(mesh, Mesh::Moon(_))));
        assert_eq!(drawn[0], Mesh::Grid);
    }

    #[test]
    fn sky_off_skips_the_dome_and_horizon_but_keeps_the_sun_moon_and_stars() {
        let drawn = plan_with(
            SkyDetails {
                sky: false,
                ..SkyDetails::ALL
            },
            NIGHT,
        );
        // Above the horizon no grid remains: no dome, no horizon plane.
        assert_eq!(drawn.iter().filter(|m| **m == Mesh::Grid).count(), 0);
        assert!(drawn.contains(&Mesh::Stars) && drawn.contains(&Mesh::Sun));
    }

    #[test]
    fn stars_off_skips_the_stars() {
        let drawn = plan_with(
            SkyDetails {
                stars: false,
                ..SkyDetails::ALL
            },
            NIGHT,
        );
        assert!(!drawn.contains(&Mesh::Stars) && drawn.contains(&Mesh::Sun));
    }

    #[test]
    fn sun_and_moon_off_skips_both_quads() {
        let drawn = plan_with(
            SkyDetails {
                sun_moon: false,
                ..SkyDetails::ALL
            },
            NIGHT,
        );
        assert!(!drawn.contains(&Mesh::Sun));
        assert!(!drawn.iter().any(|mesh| matches!(mesh, Mesh::Moon(_))));
        assert!(drawn.contains(&Mesh::Stars));
        let sunrise = RenderEnvironment::new(0, 23_000, 0.0, 8);
        let no_bodies = SkyDetails {
            sun_moon: false,
            ..SkyDetails::ALL
        };
        assert!(!plan_with(no_bodies, sunrise).contains(&Mesh::Fan));
    }

    #[test]
    fn the_end_sky_follows_the_sky_option() {
        let end = RenderEnvironment {
            dimension: 1,
            ..RenderEnvironment::DEFAULT
        };
        assert_eq!(plan_with(SkyDetails::ALL, end), [Mesh::EndCube]);
        let off = SkyDetails {
            sky: false,
            ..SkyDetails::ALL
        };
        assert!(plan_with(off, end).is_empty());
    }

    #[test]
    fn the_fog_mode_reaches_the_slot() {
        let environment = RenderEnvironment {
            fog_settings: crate::FogSettings {
                kind: crate::FogKind::Fast,
                start: 0.8,
            },
            ..RenderEnvironment::DEFAULT
        };
        assert_eq!(plan(environment, 80.0)[0].uniform.fog[3], 1.0);
        assert_eq!(
            plan(RenderEnvironment::DEFAULT, 80.0)[0].uniform.fog[3],
            0.0
        );
    }

    #[test]
    fn the_slot_matches_the_wgsl_layout() {
        assert_eq!(std::mem::offset_of!(SlotUniform, model), 64);
        assert_eq!(std::mem::offset_of!(SlotUniform, tint), 128);
        assert_eq!(std::mem::offset_of!(SlotUniform, params), 160);
        assert_eq!(std::mem::offset_of!(SlotUniform, fog), 176);
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
        assert_eq!(layers[0].uniform.params[..2], [0.0, 1.0]);
        assert_eq!(layers[1].uniform.params[..2], [1.0, 0.0]);
        assert_eq!(layers[0].uniform.fog[..3], [0.0, 128.0, 0.0]);
    }

    fn plan_custom(
        environment: RenderEnvironment,
        custom: &[LayerFrame],
        replaces_stars: bool,
    ) -> Vec<Layer> {
        let state = SkyState::at(environment);
        let input = PlanInput {
            kind: SkyKind::of(environment.dimension),
            state: &state,
            view_proj: Mat4::IDENTITY,
            eye_y: 80.0,
            flat_world: false,
            details: environment.sky_details,
            custom,
            custom_replaces_stars: replaces_stars,
        };
        let mut layers = Vec::new();
        build(&mut layers, &input);
        layers
    }

    fn frame(index: usize, blend: umicraft_packext::sky::Blend) -> LayerFrame {
        LayerFrame {
            index,
            blend,
            brightness: 0.5,
            rotation: None,
            axis: [0.0, 0.0, -1.0],
        }
    }

    #[test]
    fn custom_layers_sit_between_the_fan_and_the_sun_in_order() {
        use umicraft_packext::sky::Blend as B;
        let sunrise = RenderEnvironment {
            world_time: 23_000,
            ..RenderEnvironment::DEFAULT
        };
        let layers = plan_custom(sunrise, &[frame(3, B::Add), frame(4, B::Screen)], true);
        let order = meshes(&layers);
        let fan = order.iter().position(|m| *m == Mesh::Fan).expect("fan");
        assert_eq!(order[fan + 1..fan + 3], [Mesh::Cube, Mesh::Cube]);
        assert_eq!(order[fan + 3], Mesh::Sun);
        assert_eq!(layers[fan + 1].image, Image::Custom(3));
        assert_eq!(layers[fan + 2].blend, Blend::Screen);
    }

    #[test]
    fn custom_layers_replace_the_stars_even_when_faded_out() {
        let night = RenderEnvironment {
            world_time: 18_000,
            ..RenderEnvironment::DEFAULT
        };
        assert!(meshes(&plan_custom(night, &[], false)).contains(&Mesh::Stars));
        assert!(!meshes(&plan_custom(night, &[], true)).contains(&Mesh::Stars));
    }

    #[test]
    fn the_tint_follows_the_blend_and_replace_alpha_tests() {
        use umicraft_packext::sky::Blend as B;
        let layers = plan_custom(
            RenderEnvironment::DEFAULT,
            &[
                frame(0, B::Multiply),
                frame(1, B::Replace),
                frame(2, B::Alpha),
            ],
            true,
        );
        let cubes: Vec<_> = layers.iter().filter(|l| l.mesh == Mesh::Cube).collect();
        assert_eq!(cubes[0].uniform.tint, [0.5; 4]);
        assert_eq!(cubes[0].uniform.params[2], 0.0);
        assert_eq!(cubes[1].blend, Blend::Opaque);
        assert_eq!(cubes[1].uniform.params[2], 0.1);
        assert_eq!(cubes[2].uniform.tint, [1.0, 1.0, 1.0, 0.5]);
        assert_eq!(cubes[2].uniform.params[..2], [1.0, 0.0], "textured, no fog");
    }

    #[test]
    fn the_end_draws_no_custom_layers_it_has_none_of() {
        let end = plan_custom(RenderEnvironment::new(1, 0, 0.0, 8), &[], false);
        assert_eq!(meshes(&end), [Mesh::EndCube]);
    }
}

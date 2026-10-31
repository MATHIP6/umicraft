// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the fog model against the reference formulas.

use super::*;

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

fn env(fog: FogContext) -> RenderEnvironment {
    RenderEnvironment {
        fog,
        ..RenderEnvironment::DEFAULT
    }
}

const BASE: [f32; 3] = [0.6, 0.7, 0.9];

#[test]
fn plain_fog_is_linear_from_three_quarters_of_the_far_plane() {
    // Eight chunks: 128 blocks.
    let fog = FogParams::new(BASE, &RenderEnvironment::DEFAULT);
    assert_eq!(
        fog.world,
        FogMode::Linear {
            start: 96.0,
            end: 128.0
        }
    );
    assert_eq!(
        fog.sky,
        FogMode::Linear {
            start: 0.0,
            end: 128.0
        }
    );
}

#[test]
fn the_far_plane_is_capped_inside_the_camera() {
    assert_eq!(far_plane(32), 500.0);
    assert_eq!(far_plane(0), 1.0);
    assert_eq!(far_plane(255), 500.0);
}

#[test]
fn the_plain_colour_follows_the_view_distance_light_mix() {
    // Full light at eight chunks leaves the colour unchanged.
    let fog = FogParams::new(BASE, &RenderEnvironment::DEFAULT);
    assert!(near(fog.color[0], 0.6) && near(fog.color[2], 0.9));
    // A dark eye dims it to the `view / 32` floor (0.25 at eight chunks).
    let dark = FogParams::new(
        BASE,
        &env(FogContext {
            light_brightness: 0.0,
            ..FogContext::DEFAULT
        }),
    );
    assert!(near(dark.color[0], 0.6 * 0.25));
}

#[test]
fn water_uses_exponential_fog_and_the_deep_blue() {
    let water = env(FogContext {
        medium: FogMedium::Water,
        ..FogContext::DEFAULT
    });
    let fog = FogParams::new(BASE, &water);
    assert_eq!(fog.world, FogMode::Exp { density: 0.1 });
    assert_eq!(fog.sky, FogMode::Exp { density: 0.1 });
    assert!(near(fog.color[0], 0.02) && near(fog.color[2], 0.2));
}

#[test]
fn water_breathing_thins_and_lightens_the_water() {
    let water = env(FogContext {
        medium: FogMedium::Water,
        water_breathing: true,
        ..FogContext::DEFAULT
    });
    let fog = FogParams::new(BASE, &water);
    assert_eq!(fog.world, FogMode::Exp { density: 0.01 });
    assert!(near(fog.color[0], 0.62) && near(fog.color[2], 0.8));
}

#[test]
fn lava_is_dense_and_orange() {
    let lava = env(FogContext {
        medium: FogMedium::Lava,
        ..FogContext::DEFAULT
    });
    let fog = FogParams::new(BASE, &lava);
    assert_eq!(fog.world, FogMode::Exp { density: 2.0 });
    assert!(near(fog.color[0], 0.6) && near(fog.color[1], 0.1) && fog.color[2] == 0.0);
}

#[test]
fn blindness_closes_the_fog_to_five_blocks_and_opens_it_in_the_last_second() {
    let blind = |ticks| {
        FogParams::new(
            BASE,
            &env(FogContext {
                blindness_ticks: Some(ticks),
                ..FogContext::DEFAULT
            }),
        )
    };
    assert_eq!(
        blind(100).world,
        FogMode::Linear {
            start: 1.25,
            end: 5.0
        }
    );
    assert_eq!(
        blind(100).sky,
        FogMode::Linear {
            start: 0.0,
            end: 4.0
        }
    );
    // Halfway through the last second the reach is halfway to the far plane.
    let FogMode::Linear { end, .. } = blind(10).world else {
        panic!("linear");
    };
    assert!(near(end, 5.0 + (128.0 - 5.0) * 0.5));
    // The colour is black while blind, and returns with the fog.
    assert_eq!(blind(100).color, [0.0; 3]);
    assert!(blind(10).color[0] > 0.0);
}

#[test]
fn the_void_darkens_the_fog_below_y_32() {
    let at = |eye_y| {
        FogParams::new(
            BASE,
            &env(FogContext {
                eye_y,
                ..FogContext::DEFAULT
            }),
        )
        .color[0]
    };
    assert!(near(at(32.0), 0.6));
    assert!(near(at(16.0), 0.6 * 0.25));
    assert!(near(at(-5.0), 0.0));
}

#[test]
fn the_nether_and_the_end_close_the_fog_in() {
    for dimension in [-1, 1] {
        let fog = FogParams::new(
            BASE,
            &RenderEnvironment {
                dimension,
                ..RenderEnvironment::DEFAULT
            },
        );
        assert_eq!(
            fog.world,
            FogMode::Linear {
                start: 6.4,
                end: 64.0
            }
        );
        assert_eq!(fog.sky, fog.world, "the override covers the sky pass too");
    }
}

#[test]
fn the_end_fog_range_is_capped_at_192() {
    let fog = FogParams::new(
        BASE,
        &RenderEnvironment {
            dimension: 1,
            view_distance: 32,
            ..RenderEnvironment::DEFAULT
        },
    );
    assert_eq!(
        fog.world,
        FogMode::Linear {
            start: 25.0,
            end: 96.0
        }
    );
}

#[test]
fn night_vision_brightens_toward_the_strongest_channel() {
    let fog = FogParams::new(
        [0.2, 0.1, 0.4],
        &env(FogContext {
            night_vision_ticks: Some(1_000),
            ..FogContext::DEFAULT
        }),
    );
    assert!(near(fog.color[2], 1.0));
    assert!(near(fog.color[0], 0.5));
}

#[test]
fn night_vision_flashes_in_the_last_ten_seconds_and_keeps_black() {
    assert_eq!(night_vision_brightness(201, 0.0), 1.0);
    assert!(near(
        night_vision_brightness(5, 0.0),
        0.7 + 0.3 * (1.0f32 * std::f32::consts::PI).sin()
    ));
    assert_eq!(night_vision([0.0; 3], 1.0), [0.0; 3]);
}

#[test]
fn the_eye_light_follows_the_reference_brightness_table() {
    // Full light by day (noon subtracts nothing), a dark cave, and the
    // 1.8 table value for level 7.
    assert!(near(light_brightness(0, 15, 0, 6_000), 1.0));
    assert!(near(light_brightness(0, 0, 0, 6_000), 0.0));
    let dark = 1.0 - 7.0 / 15.0;
    assert!(near(
        light_brightness(7, 0, 0, 6_000),
        (1.0 - dark) / (dark * 3.0 + 1.0)
    ));
    // At night the open sky is no longer full light; the Nether never
    // drops below its ambient.
    assert!(light_brightness(0, 15, 0, 18_000) < 0.2);
    assert!(near(light_brightness(0, 0, -1, 6_000), 0.1));
}

#[test]
fn the_shader_triple_marks_the_exponential_mode_by_its_density() {
    assert_eq!(
        FogMode::Linear {
            start: 1.0,
            end: 2.0
        }
        .uniform(),
        [1.0, 2.0, 0.0]
    );
    assert_eq!(FogMode::Exp { density: 0.1 }.uniform()[2], 0.1);
}

#[test]
fn fog_wgsl_is_valid() {
    let module = naga::front::wgsl::parse_str(&format!(
        "{}\n@fragment fn fs() -> @location(0) vec4<f32> {{ return vec4<f32>(fog_amount(1.0, 0.0, 2.0, 0.0)); }}",
        include_str!("../fog.wgsl")
    ))
    .expect("the shader parses");
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .expect("the shader validates");
}

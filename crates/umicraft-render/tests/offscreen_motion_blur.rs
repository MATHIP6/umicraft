// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opt-in offscreen test of the motion blur (the "Motion Blur" mod): the three
//! versions blur a turning camera and leave a still one, a jump and the HUD
//! alone, and never darken or brighten the picture.
//!
//! The planner takes the frame time from the clock, so a frame that takes longer
//! than 0.2 s on a slow adapter counts as a hitch and is not blurred; run this on
//! a real GPU.
//!
//! Run with `cargo test -p umicraft-render --test offscreen_motion_blur -- --ignored
//! --test-threads=1`: seven tests opening a GPU device at once stalled once.

use std::sync::Arc;

use glam::{DVec3, Vec3};
use umicraft_mesh::{mesh_chunk, ChunkSnapshot};
use umicraft_render::{Camera, MotionBlurSettings, MotionBlurVersion, Overlay, Renderer};
use umicraft_world::{BlockState, ChunkColumn, ChunkPos};

const SIZE: u32 = 128;

/// A stone floor at y < 60 with a few pillars on it: edges at several depths.
fn column() -> ChunkColumn {
    let mut column = ChunkColumn::new(ChunkPos::new(0, 0));
    for y in 0..60 {
        for z in 0..16 {
            for x in 0..16 {
                column.set_block(x, y, z, BlockState::new(1, 0));
            }
        }
    }
    for (px, pz, top) in [(2, 3, 72), (7, 9, 66), (12, 4, 78), (10, 13, 70)] {
        for y in 60..top {
            for z in pz..pz + 2 {
                for x in px..px + 2 {
                    column.set_block(x, y, z, BlockState::new(1, 0));
                }
            }
        }
    }
    column
}

fn camera(yaw: f32) -> Camera {
    let mut camera = Camera::new(Vec3::new(8.0, 68.0, 36.0), yaw, -0.15);
    camera.set_aspect(SIZE, SIZE);
    camera
}

fn renderer(blur: Option<MotionBlurVersion>) -> Renderer {
    let mut renderer = Renderer::new_offscreen(SIZE, SIZE).expect("offscreen renderer");
    let pos = ChunkPos::new(0, 0);
    let snapshot = ChunkSnapshot::from_columns(pos, [Arc::new(column())]);
    renderer.upload_mesh(pos, &mesh_chunk(pos, &snapshot));
    renderer.set_motion_blur(blur.map(|version| MotionBlurSettings {
        version,
        strength: 0.8,
    }));
    renderer
}

fn draw(renderer: &mut Renderer, yaw: f32, overlay: &Overlay) -> Vec<u8> {
    renderer.render(&camera(yaw), overlay).expect("render");
    renderer.read_pixels().expect("pixels").rgba
}

/// The picture of a camera at `yaw` with no blur at all.
fn sharp(yaw: f32, overlay: &Overlay) -> Vec<u8> {
    draw(&mut renderer(None), yaw, overlay)
}

/// The picture after the camera turned through `yaws`, blur on.
fn blurred(version: MotionBlurVersion, yaws: &[f32], overlay: &Overlay) -> Vec<u8> {
    let mut renderer = renderer(Some(version));
    yaws.iter()
        .map(|&yaw| draw(&mut renderer, yaw, overlay))
        .last()
        .expect("a frame")
}

fn mean(pixels: &[u8]) -> f64 {
    pixels.iter().map(|&p| f64::from(p)).sum::<f64>() / pixels.len() as f64
}

fn differing(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).filter(|(x, y)| x != y).count()
}

/// The picture after the camera turned through `yaws` with the player's eye at
/// `body` (the third person), blur on.
fn blurred_around(
    version: MotionBlurVersion,
    yaws: &[f32],
    overlay: &Overlay,
    body: Option<DVec3>,
) -> Vec<u8> {
    let mut renderer = renderer(Some(version));
    renderer.set_motion_blur_body(body);
    yaws.iter()
        .map(|&yaw| draw(&mut renderer, yaw, overlay))
        .last()
        .expect("a frame")
}

/// The eye of a player whose body axis runs along the left edge of the pillar at
/// (7, 9), a third of a block behind its visible face: that edge, between stone and
/// what is behind it, stands for the outline of the skin.
const PLAYER_EYE: DVec3 = DVec3::new(7.0, 63.62, 10.7);

/// The pixels of the pillar just inside that edge, at the middle of the body, as
/// `(left, top, right, bottom)`, for a camera at `yaw`. They are the skin; the
/// pixels on the other side of the edge are the world behind it, which stays
/// blurred.
fn edge_box(yaw: f32) -> (u32, u32, u32, u32) {
    let clip = camera(yaw).view_projection() * Vec3::new(7.0, 63.0, 11.0).extend(1.0);
    let ndc = clip.truncate() / clip.w;
    let x = ((ndc.x * 0.5 + 0.5) * SIZE as f32) as u32;
    let y = ((1.0 - (ndc.y * 0.5 + 0.5)) * SIZE as f32) as u32;
    (x + 1, y - 3, x + 3, y + 3)
}

/// The bytes of the pixels inside `area`.
fn region(pixels: &[u8], (left, top, right, bottom): (u32, u32, u32, u32)) -> Vec<u8> {
    let mut inside = Vec::new();
    for y in top..bottom {
        for x in left..right {
            let at = ((y * SIZE + x) * 4) as usize;
            inside.extend_from_slice(&pixels[at..at + 4]);
        }
    }
    inside
}

const VERSIONS: [MotionBlurVersion; 3] = [
    MotionBlurVersion::V1,
    MotionBlurVersion::V2,
    MotionBlurVersion::V3,
];

/// Four frames of a camera that turns a little each time.
const TURN: [f32; 4] = [0.00, 0.03, 0.06, 0.09];

#[test]
#[ignore = "requires a GPU adapter"]
fn a_turning_camera_is_blurred_by_every_version() {
    let overlay = Overlay::new(SIZE, SIZE);
    let reference = sharp(0.09, &overlay);
    for version in VERSIONS {
        let picture = blurred(version, &TURN, &overlay);
        let changed = differing(&picture, &reference);
        assert!(
            changed > 100,
            "{version:?} left the picture sharp ({changed})"
        );
        let change = (mean(&picture) - mean(&reference)).abs() / mean(&reference);
        assert!(
            change < 0.03,
            "{version:?} changed the brightness by {change}"
        );
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_camera_that_stands_still_is_not_blurred() {
    let overlay = Overlay::new(SIZE, SIZE);
    let reference = sharp(0.05, &overlay);
    for version in VERSIONS {
        let picture = blurred(version, &[0.05, 0.05, 0.05], &overlay);
        assert_eq!(picture, reference, "{version:?} blurred a still camera");
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_jump_of_the_camera_is_not_blurred() {
    let overlay = Overlay::new(SIZE, SIZE);
    let reference = sharp(2.5, &overlay);
    for version in VERSIONS {
        let picture = blurred(version, &[0.0, 2.5], &overlay);
        assert_eq!(picture, reference, "{version:?} blurred a jump");
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn the_hud_stays_sharp_over_a_blurred_world() {
    let mut overlay = Overlay::new(SIZE, SIZE);
    overlay.rect(32.0, 32.0, 64.0, 24.0, [1.0, 0.0, 0.0, 1.0]);
    let reference = sharp(0.09, &overlay);
    let hud = |pixels: &[u8]| {
        let mut inside = Vec::new();
        for y in 36..52 {
            for x in 36..92 {
                let at = ((y * SIZE + x) * 4) as usize;
                inside.extend_from_slice(&pixels[at..at + 4]);
            }
        }
        inside
    };
    for version in VERSIONS {
        let picture = blurred(version, &TURN, &overlay);
        assert_ne!(picture, reference, "{version:?} did blur the world");
        assert_eq!(
            hud(&picture),
            hud(&reference),
            "{version:?} blurred the HUD"
        );
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn the_body_of_the_player_is_not_blurred_in_the_third_person() {
    let overlay = Overlay::new(SIZE, SIZE);
    let reference = sharp(0.09, &overlay);
    let area = edge_box(0.09);
    for version in VERSIONS {
        let without = blurred_around(version, &TURN, &overlay, None);
        assert_ne!(
            region(&without, area),
            region(&reference, area),
            "{version:?}: the skin is blurred when no body is spared, so the test sees it"
        );
        let with = blurred_around(version, &TURN, &overlay, Some(PLAYER_EYE));
        assert_eq!(
            region(&with, area),
            region(&reference, area),
            "{version:?} blurred the body of the player"
        );
        assert_ne!(
            with, reference,
            "{version:?} left the whole picture sharp, not only the body"
        );
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn the_frame_that_goes_to_the_third_person_is_not_blurred() {
    let overlay = Overlay::new(SIZE, SIZE);
    for version in VERSIONS {
        let mut renderer = renderer(Some(version));
        draw(&mut renderer, 0.00, &overlay);
        draw(&mut renderer, 0.03, &overlay);
        // F5: the camera goes to the orbit, and the body comes into view.
        renderer.set_motion_blur_body(Some(PLAYER_EYE));
        let picture = draw(&mut renderer, 0.06, &overlay);
        assert_eq!(
            picture,
            sharp(0.06, &overlay),
            "{version:?} blurred the frame of the switch"
        );
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn turning_the_blur_off_gives_the_sharp_picture_back() {
    let overlay = Overlay::new(SIZE, SIZE);
    let reference = sharp(0.09, &overlay);
    let mut renderer = renderer(Some(MotionBlurVersion::V3));
    for yaw in TURN {
        draw(&mut renderer, yaw, &overlay);
    }
    renderer.set_motion_blur(None);
    assert_eq!(draw(&mut renderer, 0.09, &overlay), reference);
    // And on again: it works after having been freed.
    renderer.set_motion_blur(Some(MotionBlurSettings {
        version: MotionBlurVersion::V2,
        strength: 0.8,
    }));
    for yaw in [0.12, 0.15, 0.18] {
        draw(&mut renderer, yaw, &overlay);
    }
    assert_ne!(draw(&mut renderer, 0.21, &overlay), sharp(0.21, &overlay));
}

/// A square of the hand in camera space, 0.4 wide, its centre at `x`.
fn hand_quad(x: f32) -> Vec<umicraft_render::EntityVertex> {
    let vertex = |dx: f32, y: f32| umicraft_render::EntityVertex {
        position: [x + dx, y, -0.6],
        normal: [0.0, 0.0, 1.0],
        uv: [0.5, 0.5],
        light: 0xFF,
    };
    vec![
        vertex(-0.2, -0.2),
        vertex(0.2, -0.2),
        vertex(0.2, 0.2),
        vertex(-0.2, -0.2),
        vertex(0.2, 0.2),
        vertex(-0.2, 0.2),
    ]
}

/// The last picture of a still camera and a hand that slides right through
/// `positions`, with the blur `version` on (`None`: off) and `moving` told to it.
fn slid_hand(version: Option<MotionBlurVersion>, positions: &[f32], moving: bool) -> Vec<u8> {
    let mut renderer = renderer(version);
    let skin = renderer.create_entity_texture(&[255, 0, 0, 255], 1, 1);
    renderer.set_motion_blur_hand_moving(moving);
    let overlay = Overlay::new(SIZE, SIZE);
    positions
        .iter()
        .map(|&x| {
            renderer.set_hand(&hand_quad(x), skin.bind_group());
            draw(&mut renderer, 0.0, &overlay)
        })
        .last()
        .expect("a frame")
}

const SLIDE: [f32; 3] = [-0.2, 0.0, 0.2];

#[test]
#[ignore = "requires a GPU adapter"]
fn a_moving_hand_leaves_a_trail_even_when_the_camera_stands_still() {
    let reference = slid_hand(None, &SLIDE, false);
    for version in VERSIONS {
        let picture = slid_hand(Some(version), &SLIDE, true);
        assert!(
            differing(&picture, &reference) > 0,
            "{version:?}: the hand is sharp"
        );
        // Away from the hand nothing moves: the world is not touched.
        let top = (SIZE * 4 * 8) as usize;
        assert_eq!(
            &picture[..top],
            &reference[..top],
            "{version:?}: the sky above the hand is blurred"
        );
        assert!(
            mean(&picture) > mean(&reference) * 0.95 && mean(&picture) < mean(&reference) * 1.05,
            "{version:?}: the brightness changed"
        );
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_hand_that_stands_still_is_not_blurred() {
    let reference = slid_hand(None, &SLIDE, false);
    for version in VERSIONS {
        let picture = slid_hand(Some(version), &SLIDE, false);
        assert_eq!(
            picture, reference,
            "{version:?}: a still camera and a hand not reported as moving"
        );
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The panorama of `--hud-demo --hud-view title`.
//!
//! The demo uses the user's own panorama faces when the verified jar is
//! installed. Without it, `--synthetic-panorama` draws six generated gradient
//! faces so the cube pass can still be looked at; nothing here is a Mojang
//! picture.

use umicraft_render::{Renderer, SkyImage};
use umicraft_ui::menus::PanoramaTextures;

use crate::args::Args;
use crate::panorama_faces::load_panorama_faces;

/// Edge of a generated face in pixels.
const SYNTHETIC_SIZE: u32 = 128;

/// The six faces the demo should show: generated ones when asked for, else
/// the user's download, else none.
fn faces(args: &Args) -> Option<[SkyImage; 6]> {
    if args.synthetic_panorama {
        return Some(synthetic_faces());
    }
    let jar = crate::official::find_verified_jar().ok();
    load_panorama_faces(jar.as_ref())
}

/// Uploads the demo faces to `renderer`; `None` keeps the dirt background.
pub(crate) fn upload(args: &Args, renderer: &mut Renderer) -> Option<PanoramaTextures> {
    renderer.upload_panorama(&faces(args)?)
}

/// Six gradients with a distinct hue per face, brighter towards the bottom
/// right, with a dark grid so the rotation and the seams are visible.
fn synthetic_faces() -> [SkyImage; 6] {
    const HUES: [[f32; 3]; 6] = [
        [1.0, 0.3, 0.3],
        [0.3, 1.0, 0.3],
        [0.3, 0.3, 1.0],
        [1.0, 1.0, 0.3],
        [0.3, 1.0, 1.0],
        [1.0, 0.3, 1.0],
    ];
    HUES.map(|hue| {
        let mut rgba = Vec::with_capacity((SYNTHETIC_SIZE * SYNTHETIC_SIZE * 4) as usize);
        for y in 0..SYNTHETIC_SIZE {
            for x in 0..SYNTHETIC_SIZE {
                let light = 0.4 + 0.6 * (x + y) as f32 / (2 * SYNTHETIC_SIZE) as f32;
                let on_grid = x % 32 == 0 || y % 32 == 0;
                let scale = if on_grid { 0.25 } else { light };
                rgba.extend(hue.map(|c| (c * scale * 255.0) as u8));
                rgba.push(255);
            }
        }
        SkyImage {
            width: SYNTHETIC_SIZE,
            height: SYNTHETIC_SIZE,
            rgba,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_synthetic_faces_are_valid_and_differ() {
        let faces = synthetic_faces();
        assert!(faces.iter().all(SkyImage::is_valid));
        assert_ne!(faces[0], faces[1]);
    }
}

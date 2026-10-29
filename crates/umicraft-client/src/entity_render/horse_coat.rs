// SPDX-License-Identifier: MIT OR Apache-2.0

//! Horse coats: a base colour with an optional marking drawn over it
//! (MCP `EntityHorse.getHorseTexturePaths`, `LayeredTexture`).

use umicraft_assets::client_jar::ClientJar;
use umicraft_assets::skin::{decode_png_bytes, procedural_horse_texture};

/// Base colour textures by the low byte of the variant (`0..=6`).
const BASES: [&str; 7] = [
    "horse_white",
    "horse_creamy",
    "horse_chestnut",
    "horse_brown",
    "horse_black",
    "horse_gray",
    "horse_darkbrown",
];

/// Marking textures by the second byte of the variant (`1..=4`; `0` is none).
const MARKINGS: [&str; 5] = [
    "",
    "horse_markings_white",
    "horse_markings_whitefield",
    "horse_markings_whitedots",
    "horse_markings_blackdots",
];

/// The jar path of a horse texture.
fn path(name: &str) -> String {
    format!("assets/minecraft/textures/entity/horse/{name}.png")
}

/// The base and marking names of a variant; unknown values fall back to the
/// first entry, as the modulo of `EntityHorse.getHorseTexturePaths` does.
fn names(variant: u16) -> (&'static str, &'static str) {
    let base = BASES[usize::from(variant & 255) % BASES.len()];
    let marking = MARKINGS[usize::from(variant >> 8) % MARKINGS.len()];
    (base, marking)
}

/// Draws `over` on `under`, both RGBA of the same size (source-over).
fn composite(under: &mut [u8], over: &[u8]) {
    for (below, above) in under.chunks_exact_mut(4).zip(over.chunks_exact(4)) {
        let alpha = u32::from(above[3]);
        if alpha == 0 {
            continue;
        }
        for channel in 0..3 {
            let mixed =
                u32::from(above[channel]) * alpha + u32::from(below[channel]) * (255 - alpha);
            below[channel] = (mixed / 255) as u8;
        }
        below[3] = below[3].max(above[3]);
    }
}

/// The RGBA texture of a coat, from the jar or the procedural horse.
#[must_use]
pub(super) fn horse_coat(jar: Option<&ClientJar>, variant: u16) -> (Vec<u8>, u32, u32) {
    let (base, marking) = names(variant);
    let read = |name: &str| {
        jar.and_then(|jar| jar.read_entry(&path(name)).ok())
            .and_then(|bytes| decode_png_bytes(&bytes).ok())
    };
    let Some((mut rgba, width, height)) = read(base) else {
        return (procedural_horse_texture(), 128, 128);
    };
    if !marking.is_empty() {
        if let Some((over, w, h)) = read(marking) {
            if (w, h) == (width, height) {
                composite(&mut rgba, &over);
            }
        }
    }
    (rgba, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_variant_picks_a_base_and_a_marking() {
        assert_eq!(names(0), ("horse_white", ""));
        assert_eq!(names(0x0102), ("horse_chestnut", "horse_markings_white"));
        assert_eq!(
            names(0x0406),
            ("horse_darkbrown", "horse_markings_blackdots")
        );
    }

    #[test]
    fn unknown_variants_wrap_instead_of_panicking() {
        assert_eq!(names(0xFFFF).0, BASES[255 % 7]);
        assert_eq!(names(0x0900).1, MARKINGS[9 % 5]);
    }

    #[test]
    fn an_opaque_pixel_replaces_and_a_clear_one_keeps_the_base() {
        let mut under = vec![10, 20, 30, 255, 40, 50, 60, 255];
        composite(&mut under, &[200, 100, 0, 255, 1, 1, 1, 0]);
        assert_eq!(under, vec![200, 100, 0, 255, 40, 50, 60, 255]);
    }

    #[test]
    fn a_half_transparent_pixel_blends() {
        let mut under = vec![0, 0, 0, 255];
        composite(&mut under, &[200, 100, 50, 128]);
        assert!((i32::from(under[0]) - 100).abs() <= 1);
        assert_eq!(under[3], 255);
    }

    #[test]
    fn without_a_jar_the_procedural_horse_is_used() {
        let (rgba, width, height) = horse_coat(None, 0x0102);
        assert_eq!((width, height), (128, 128));
        assert_eq!(rgba.len(), 128 * 128 * 4);
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Procedurally generated fallback block textures (original assets, no Mojang
//! art). Each layer is 16×16 RGBA8 and is uploaded as one `texture_2d_array`.

use umicraft_mesh::TEXTURE_LAYERS;

/// Edge length of one fallback texture in texels.
pub const TEXTURE_SIZE: u32 = 16;
/// Bytes in one 16×16 RGBA8 layer.
pub const LAYER_BYTES: usize = (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize;

/// Returns all fallback layers, layer-major, ready for one array upload.
#[must_use]
pub fn fallback_texture_data() -> Vec<u8> {
    let mut data = Vec::with_capacity(TEXTURE_LAYERS as usize * LAYER_BYTES);
    for layer in 0..TEXTURE_LAYERS {
        for y in 0..TEXTURE_SIZE as usize {
            for x in 0..TEXTURE_SIZE as usize {
                data.extend_from_slice(&color_for(layer, x, y));
            }
        }
    }
    data
}

fn color_for(layer: u32, x: usize, y: usize) -> [u8; 4] {
    match layer {
        0 => grass_top(x, y),
        1 => grass_side(x, y),
        2 => dirt(x, y, 2),
        3 => stone(x, y),
        4 => sand(x, y),
        5 => bedrock(x, y),
        6 => water(x, y),
        9 => log_top(x, y),
        10 => log_side(x, y),
        11 => leaves(x, y),
        12 => planks(x, y),
        13 => gravel(x, y),
        14 => glass(x, y),
        15 => sandstone(x, y),
        16 => bricks(x, y),
        17 => obsidian(x, y),
        _ => missing(x, y),
    }
}

fn grass_top(x: usize, y: usize) -> [u8; 4] {
    let g = varied(145, x, y, 0x11, 18);
    [
        varied(92, x, y, 0x12, 12),
        g,
        varied(50, x, y, 0x13, 10),
        255,
    ]
}

fn grass_side(x: usize, y: usize) -> [u8; 4] {
    let lip = 3 + (noise(x, 0, 0x21) & 1) as usize;
    if y < lip {
        grass_top(x, y)
    } else {
        dirt(x, y, 0x22)
    }
}

fn dirt(x: usize, y: usize, seed: u32) -> [u8; 4] {
    [
        varied(134, x, y, seed, 20),
        varied(96, x, y, seed ^ 0x31, 16),
        varied(67, x, y, seed ^ 0x32, 12),
        255,
    ]
}

fn stone(x: usize, y: usize) -> [u8; 4] {
    let base = varied(126, x, y, 0x41, 18);
    if noise(x, y, 0x42) > 246 {
        [
            base.saturating_sub(30),
            base.saturating_sub(30),
            base.saturating_sub(30),
            255,
        ]
    } else {
        [base, base, varied(base, x, y, 0x43, 3), 255]
    }
}

fn sand(x: usize, y: usize) -> [u8; 4] {
    [
        varied(219, x, y, 0x51, 12),
        varied(207, x, y, 0x52, 12),
        varied(163, x, y, 0x53, 10),
        255,
    ]
}

fn bedrock(x: usize, y: usize) -> [u8; 4] {
    let value = varied(64, x, y, 0x61, 38);
    [value, value, value, 255]
}

fn water(x: usize, y: usize) -> [u8; 4] {
    let wave = if (y + x / 4).is_multiple_of(4) { 20 } else { 0 };
    [
        40u8.saturating_add(wave),
        105u8.saturating_add(wave),
        215u8.saturating_add(wave / 2),
        255,
    ]
}

fn log_top(x: usize, y: usize) -> [u8; 4] {
    let dx = x as i32 - 8;
    let dy = y as i32 - 8;
    let ring = (dx * dx + dy * dy) / 8;
    if ring % 2 == 0 {
        [142, 105, 58, 255]
    } else {
        [110, 78, 42, 255]
    }
}

fn log_side(x: usize, y: usize) -> [u8; 4] {
    if x.is_multiple_of(4) {
        [82, 57, 30, 255]
    } else {
        [
            varied(118, x, y, 0x71, 10),
            varied(84, x, y, 0x72, 8),
            varied(45, x, y, 0x73, 6),
            255,
        ]
    }
}

fn leaves(x: usize, y: usize) -> [u8; 4] {
    if noise(x, y, 0x81) > 238 {
        [28, 58, 20, 255]
    } else {
        [
            varied(58, x, y, 0x82, 18),
            varied(108, x, y, 0x83, 22),
            varied(38, x, y, 0x84, 12),
            255,
        ]
    }
}

fn planks(x: usize, y: usize) -> [u8; 4] {
    if y.is_multiple_of(4) || (x == (y / 4) * 5 + 3) {
        [105, 76, 43, 255]
    } else {
        [
            varied(163, x, y, 0x91, 10),
            varied(120, x, y, 0x92, 8),
            varied(70, x, y, 0x93, 6),
            255,
        ]
    }
}

fn gravel(x: usize, y: usize) -> [u8; 4] {
    let n = noise(x, y, 0xA1);
    if n > 200 {
        [90, 82, 78, 255]
    } else if n < 55 {
        [150, 137, 126, 255]
    } else {
        [
            varied(122, x, y, 0xA2, 14),
            varied(112, x, y, 0xA3, 12),
            varied(104, x, y, 0xA4, 10),
            255,
        ]
    }
}

fn glass(x: usize, y: usize) -> [u8; 4] {
    if x == 0 || y == 0 || x == 15 || y == 15 {
        [190, 220, 235, 255]
    } else if x == y || x + y == 15 {
        [225, 245, 255, 255]
    } else {
        [170, 210, 230, 255]
    }
}

fn sandstone(x: usize, y: usize) -> [u8; 4] {
    if y.is_multiple_of(5) {
        [196, 184, 140, 255]
    } else {
        [
            varied(224, x, y, 0xB1, 8),
            varied(213, x, y, 0xB2, 8),
            varied(170, x, y, 0xB3, 6),
            255,
        ]
    }
}

fn bricks(x: usize, y: usize) -> [u8; 4] {
    let row = y / 4;
    let offset = (row % 2) * 4;
    if y.is_multiple_of(4) || (x + offset).is_multiple_of(8) {
        [156, 148, 140, 255]
    } else {
        [
            varied(154, x, y, 0xC1, 10),
            varied(74, x, y, 0xC2, 8),
            varied(60, x, y, 0xC3, 6),
            255,
        ]
    }
}

fn obsidian(x: usize, y: usize) -> [u8; 4] {
    [
        varied(30, x, y, 0xD1, 12),
        varied(20, x, y, 0xD2, 10),
        varied(48, x, y, 0xD3, 14),
        255,
    ]
}

fn missing(x: usize, y: usize) -> [u8; 4] {
    if ((x / 4) + (y / 4)).is_multiple_of(2) {
        [220, 40, 220, 255]
    } else {
        [25, 25, 25, 255]
    }
}

/// Deterministic value noise in `0..=255` for tiling textures.
fn noise(x: usize, y: usize, seed: u32) -> u32 {
    let mut hash = (x as u32).wrapping_mul(0x9E37_79B9)
        ^ (y as u32).wrapping_mul(0x85EB_CA6B)
        ^ seed.wrapping_mul(0xC2B2_AE35);
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x7FEB_352D);
    hash ^= hash >> 15;
    hash & 0xFF
}

fn varied(base: u8, x: usize, y: usize, seed: u32, spread: u8) -> u8 {
    let range = u32::from(spread) * 2 + 1;
    let offset = noise(x, y, seed) % range;
    (i32::from(base) + offset as i32 - i32::from(spread)).clamp(0, 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_data_is_complete_and_opaque() {
        let data = fallback_texture_data();
        assert_eq!(data.len(), TEXTURE_LAYERS as usize * LAYER_BYTES);
        assert!(data.chunks_exact(4).all(|pixel| pixel[3] == 255));
    }

    #[test]
    fn textures_are_deterministic_and_layers_differ() {
        let first = fallback_texture_data();
        let second = fallback_texture_data();
        assert_eq!(first, second);
        assert_ne!(&first[..LAYER_BYTES], &first[LAYER_BYTES..LAYER_BYTES * 2]);
    }
}

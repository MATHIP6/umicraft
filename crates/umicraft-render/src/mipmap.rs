// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mip chain of a block texture layer, as 1.8.8 builds it.
//!
//! `TextureUtil.generateMipmapData` halves a sprite by averaging each 2x2
//! block in a gamma-2.2 space (`pow(c / 255, 2.2)`, mean, `pow(m, 1 / 2.2)`),
//! so a mip is not darker than the average of its texels. When the sprite has
//! see-through texels (`flag` in the reference) the transparent texels are
//! left out of the sums and an alpha below 96 becomes fully transparent, which
//! keeps cut-out leaves and flowers crisp instead of fading.
//!
//! One quirk is kept on purpose: the reference looks for transparency in the
//! first `levels + 1` texels of the sprite only (it loops to
//! `data.length`, the number of mip arrays, not to the texel count), so a
//! sprite whose first transparent texel comes later is blended as if it were
//! opaque. Source: MCP `TextureUtil.generateMipmapData`, `blendColors`,
//! `TextureAtlasSprite.generateMipmaps` (`docs/sources.md`).

/// The exponent of the gamma curve (`Math.pow(x, 2.2D)`).
const GAMMA: f64 = 2.2;
/// Alpha under this value becomes 0 in a see-through sprite.
const ALPHA_CUTOFF: i32 = 96;

/// How many mip levels below the base a layer of `width` x `height` can have
/// when `requested` are wanted: every level must halve both sides exactly
/// (`TextureMap` drops levels below the smallest sprite's power of two).
#[must_use]
pub(crate) fn level_count(width: u32, height: u32, requested: u32) -> u32 {
    if width == 0 || height == 0 {
        return 0;
    }
    requested
        .min(width.trailing_zeros())
        .min(height.trailing_zeros())
}

/// The mip levels `1..=levels` of one RGBA8 layer, each tightly packed.
///
/// `levels` is clamped with [`level_count`]. Level 0 is `base` itself and is
/// not repeated.
#[must_use]
pub(crate) fn generate_chain(base: &[u8], width: u32, height: u32, levels: u32) -> Vec<Vec<u8>> {
    let levels = level_count(width, height, levels);
    let see_through = has_see_through_texel(base, levels as usize + 1);
    let table = gamma_table();
    let mut chain: Vec<Vec<u8>> = Vec::with_capacity(levels as usize);
    let (mut w, mut h) = (width as usize, height as usize);
    for _ in 0..levels {
        let previous = chain.last().map_or(base, Vec::as_slice);
        let (half_w, half_h) = (w / 2, h / 2);
        let mut next = Vec::with_capacity(half_w * half_h * 4);
        for y in 0..half_h {
            for x in 0..half_w {
                let at = |dx: usize, dy: usize| {
                    let index = ((2 * y + dy) * w + 2 * x + dx) * 4;
                    [
                        previous[index],
                        previous[index + 1],
                        previous[index + 2],
                        previous[index + 3],
                    ]
                };
                let block = [at(0, 0), at(1, 0), at(0, 1), at(1, 1)];
                next.extend_from_slice(&blend(&block, see_through, &table));
            }
        }
        chain.push(next);
        (w, h) = (half_w, half_h);
    }
    chain
}

/// The mip chains of every layer of `pixels` (layer-major RGBA8, `layer_bytes`
/// each), computed on all the cores: a layer does not depend on another, so
/// the result is the same as calling [`generate_chain`] one after the other.
///
/// A texture pack with 64 pixel layers made this the longest step of a
/// resource reload (about 3 s on one core).
#[must_use]
pub(crate) fn generate_chains(
    pixels: &[u8],
    layer_bytes: usize,
    (width, height): (u32, u32),
    levels: u32,
) -> Vec<Vec<Vec<u8>>> {
    let layers: Vec<&[u8]> = pixels.chunks_exact(layer_bytes).collect();
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let per_thread = layers.len().div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        let workers: Vec<_> = layers
            .chunks(per_thread)
            .map(|part| {
                scope.spawn(move || {
                    part.iter()
                        .map(|layer| generate_chain(layer, width, height, levels))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("a mip worker panicked"))
            .collect()
    })
}

/// The reference's `flag`: a texel with alpha 0 among the first `checked`.
fn has_see_through_texel(base: &[u8], checked: usize) -> bool {
    base.chunks_exact(4)
        .take(checked)
        .any(|texel| texel[3] == 0)
}

/// `(float) Math.pow(value / 255.0F, 2.2)` for every byte value.
fn gamma_table() -> [f32; 256] {
    std::array::from_fn(|value| (f64::from(value as f32 / 255.0)).powf(GAMMA) as f32)
}

/// `blendColors` for the four texels of one 2x2 block.
fn blend(block: &[[u8; 4]; 4], see_through: bool, table: &[f32; 256]) -> [u8; 4] {
    let mut out = [0u8; 4];
    for (channel, slot) in out.iter_mut().enumerate() {
        let mut sum = 0.0_f32;
        for texel in block {
            if !see_through || texel[3] != 0 {
                sum += table[usize::from(texel[channel])];
            }
        }
        *slot = if see_through {
            let mean = sum / 4.0;
            to_byte(f64::from(mean).powf(1.0 / GAMMA) * 255.0)
        } else {
            // The plain branch rounds the mean to a float before the root.
            let root = (f64::from(sum) * 0.25).powf(1.0 / GAMMA) as f32;
            to_byte(f64::from(root) * 255.0)
        };
    }
    if see_through && i32::from(out[3]) < ALPHA_CUTOFF {
        out[3] = 0;
    }
    out
}

/// Java's `(int)` cast of a value in `0..=255`.
fn to_byte(value: f64) -> u8 {
    value.clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(width: usize, texel: impl Fn(usize) -> [u8; 4]) -> Vec<u8> {
        (0..width * width).flat_map(texel).collect()
    }

    #[test]
    fn the_parallel_chains_equal_the_sequential_ones_in_order() {
        let layers: Vec<u8> = (0..9_usize)
            .flat_map(|n| layer(16, |i| [(i * 7 + n * 13) as u8, n as u8, i as u8, 255]))
            .collect();
        let layer_bytes = 16 * 16 * 4;
        let parallel = generate_chains(&layers, layer_bytes, (16, 16), 4);
        let sequential: Vec<_> = layers
            .chunks_exact(layer_bytes)
            .map(|one| generate_chain(one, 16, 16, 4))
            .collect();
        assert_eq!(parallel, sequential);
    }

    #[test]
    fn a_16_pixel_layer_has_four_levels_down_to_one_texel() {
        let base = layer(16, |_| [10, 20, 30, 255]);
        let chain = generate_chain(&base, 16, 16, 4);
        let sizes: Vec<usize> = chain.iter().map(|level| level.len() / 4).collect();
        assert_eq!(sizes, [64, 16, 4, 1]);
    }

    #[test]
    fn the_request_is_capped_by_the_layer_size() {
        assert_eq!(level_count(16, 16, 9), 4);
        assert_eq!(level_count(16, 8, 4), 3);
        assert_eq!(level_count(12, 12, 4), 2, "12 halves to 6 then 3");
        assert_eq!(level_count(16, 16, 0), 0);
        assert!(generate_chain(&layer(16, |_| [0; 4]), 16, 16, 0).is_empty());
    }

    #[test]
    fn black_and_white_average_in_gamma_space_not_linearly() {
        // Two white and two black texels: (0.5)^(1/2.2) * 255 = 186, where a
        // plain mean would give 127.
        let base = layer(2, |i| if i % 2 == 0 { [255; 4] } else { [0, 0, 0, 255] });
        let chain = generate_chain(&base, 2, 2, 1);
        assert_eq!(chain[0], [186, 186, 186, 255]);
    }

    #[test]
    fn an_opaque_sprite_keeps_a_flat_colour_and_full_alpha() {
        let base = layer(4, |_| [200, 100, 50, 255]);
        let chain = generate_chain(&base, 4, 4, 2);
        for level in &chain {
            for texel in level.chunks_exact(4) {
                assert_eq!(texel[3], 255);
                for (mip, source) in texel.iter().zip([200u8, 100, 50]) {
                    assert!(source.abs_diff(*mip) <= 1, "{texel:?}");
                }
            }
        }
    }

    #[test]
    fn transparent_texels_do_not_darken_a_see_through_sprite() {
        // The first texel is clear, so the sprite is see-through: the three
        // opaque white texels average as (3/4)^(1/2.2) = 0.877 -> 223 while
        // the clear one is skipped in the colour sums.
        let mut base = vec![0u8; 2 * 2 * 4];
        for texel in 1..4 {
            base[texel * 4..texel * 4 + 4].copy_from_slice(&[255; 4]);
        }
        let chain = generate_chain(&base, 2, 2, 1);
        assert_eq!(chain[0], [223, 223, 223, 223]);
    }

    #[test]
    fn a_faint_alpha_is_cut_to_transparent() {
        // One opaque texel of three clear: alpha (1/4)^(1/2.2) = 0.53 -> 135,
        // kept; a texel of alpha 100 gives 51 < 96 and is cut.
        let strong = [[0u8; 4], [0; 4], [0; 4], [255, 255, 255, 255]];
        let faint = [[0u8; 4], [0; 4], [0; 4], [255, 255, 255, 100]];
        let table = gamma_table();
        assert_eq!(blend(&strong, true, &table)[3], 135);
        assert_eq!(blend(&faint, true, &table)[3], 0);
    }

    #[test]
    fn only_the_first_levels_plus_one_texels_decide_the_flag() {
        // 1.8.8 loops over the number of mip arrays, not the texel count, so
        // with 4 levels only texels 0..=4 are checked (regression of the
        // quirk, see the module docs).
        let mut late = layer(16, |_| [255, 255, 255, 255]);
        late[10 * 4 + 3] = 0;
        assert!(!has_see_through_texel(&late, 5));
        let mut early = layer(16, |_| [255, 255, 255, 255]);
        early[4 * 4 + 3] = 0;
        assert!(has_see_through_texel(&early, 5));
        // So the late hole is averaged as opaque: no cut-out.
        let chain = generate_chain(&late, 16, 16, 4);
        assert!(chain[0].chunks_exact(4).all(|texel| texel[3] > 0));
    }
}

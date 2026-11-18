// SPDX-License-Identifier: MIT OR Apache-2.0

//! PNG builders for the tests of the cape module.

fn encode(
    width: u32,
    height: u32,
    color: png::ColorType,
    palette: Option<Vec<u8>>,
    data: &[u8],
) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(color);
    encoder.set_depth(png::BitDepth::Eight);
    if let Some(palette) = palette {
        encoder.set_palette(palette);
    }
    let mut writer = encoder.write_header().expect("header");
    writer.write_image_data(data).expect("pixels");
    writer.finish().expect("finish");
    out
}

/// An RGBA8 PNG of `width x height` from raw pixels.
pub(crate) fn rgba_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    encode(width, height, png::ColorType::Rgba, None, rgba)
}

/// A grayscale 8-bit PNG filled with `value`.
pub(crate) fn gray_png(width: u32, height: u32, value: u8) -> Vec<u8> {
    let data = vec![value; width as usize * height as usize];
    encode(width, height, png::ColorType::Grayscale, None, &data)
}

/// A 22x17 8-bit palette PNG, the shape of a LabyMod cape: index 0 is
/// `(10, 20, 30)`, index 1 is `(200, 100, 50)`; the first pixel uses 0, the
/// rest 1.
pub(crate) fn palette_png_22x17() -> Vec<u8> {
    let mut indices = vec![1; 22 * 17];
    indices[0] = 0;
    let palette = vec![10, 20, 30, 200, 100, 50];
    encode(22, 17, png::ColorType::Indexed, Some(palette), &indices)
}

/// Standard base64 with padding.
pub(crate) fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let word = chunk.iter().enumerate().fold(0u32, |acc, (i, byte)| {
            acc | u32::from(*byte) << (16 - 8 * i)
        });
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(TABLE[(word >> (18 - 6 * i) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// `width x height` pixels of one color.
pub(crate) fn solid(width: u32, height: u32, pixel: [u8; 4]) -> Vec<u8> {
    pixel.repeat(width as usize * height as usize)
}

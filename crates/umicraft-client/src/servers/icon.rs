// SPDX-License-Identifier: MIT OR Apache-2.0

//! Decoding of a server favicon into the pixels the GUI atlas takes.
//!
//! Vanilla accepts only a 64x64 image (`ServerListEntryNormal.prepareServerIcon`
//! validates both sides and logs "Invalid icon"); anything else leaves the
//! row on the default icon.

/// The side of a server icon in pixels.
pub const ICON_SIDE: u32 = 64;

/// Decodes `png` into tightly packed RGBA8 pixels when it is exactly 64x64.
///
/// Returns `None` for a corrupt PNG or another size; the caller then keeps
/// the default `unknown_server.png`.
#[must_use]
pub fn decode_favicon(png: &[u8]) -> Option<Vec<u8>> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(
        png::Transformations::EXPAND | png::Transformations::STRIP_16 | png::Transformations::ALPHA,
    );
    let mut reader = decoder.read_info().ok()?;
    let info = reader.info();
    if (info.width, info.height) != (ICON_SIDE, ICON_SIDE) {
        return None;
    }
    let mut buffer = vec![0_u8; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buffer).ok()?;
    let pixels = buffer.get(..frame.buffer_size())?;
    to_rgba(pixels, frame.color_type)
}

/// Expands the decoded samples to RGBA; `None` for an unexpected layout.
fn to_rgba(pixels: &[u8], color: png::ColorType) -> Option<Vec<u8>> {
    let rgba = match color {
        png::ColorType::Rgba => pixels.to_vec(),
        png::ColorType::Rgb => pixels
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => pixels
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    (rgba.len() == (ICON_SIDE * ICON_SIDE * 4) as usize).then_some(rgba)
}

/// A solid-colour 64x64 PNG, for tests and the offscreen demo.
#[cfg(test)]
pub(crate) fn test_png(side: u32, color: [u8; 4]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, side, side);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("header");
    let pixels: Vec<u8> = (0..side * side).flat_map(|_| color).collect();
    writer.write_image_data(&pixels).expect("data");
    writer.finish().expect("finish");
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_64_square_png_decodes_to_rgba() {
        let rgba = decode_favicon(&test_png(64, [10, 20, 30, 255])).expect("valid");
        assert_eq!(rgba.len(), 64 * 64 * 4);
        assert_eq!(&rgba[..4], &[10, 20, 30, 255]);
    }

    #[test]
    fn another_size_or_garbage_is_refused() {
        assert_eq!(decode_favicon(&test_png(32, [0; 4])), None);
        assert_eq!(decode_favicon(b"not a png"), None);
        assert_eq!(decode_favicon(&[]), None);
    }
}

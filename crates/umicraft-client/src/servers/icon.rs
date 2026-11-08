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
    let (width, height, rgba) = decode_png(png, ICON_SIDE)?;
    ((width, height) == (ICON_SIDE, ICON_SIDE)).then_some(rgba)
}

/// Decodes `png` into its size and tightly packed RGBA8 pixels, when both
/// sides are at most `max_side`. The size is read before the pixels, so an
/// oversized image is refused without being unpacked.
///
/// The `pack.png` of a resource pack takes this path: vanilla accepts any size
/// and scales it down to the 32x32 slot of the list.
#[must_use]
pub fn decode_png(png: &[u8], max_side: u32) -> Option<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(
        png::Transformations::EXPAND | png::Transformations::STRIP_16 | png::Transformations::ALPHA,
    );
    let mut reader = decoder.read_info().ok()?;
    let info = reader.info();
    let (width, height) = (info.width, info.height);
    if width == 0 || height == 0 || width > max_side || height > max_side {
        return None;
    }
    let mut buffer = vec![0_u8; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buffer).ok()?;
    let pixels = buffer.get(..frame.buffer_size())?;
    let rgba = to_rgba(pixels, frame.color_type)?;
    (rgba.len() == (width * height * 4) as usize).then_some((width, height, rgba))
}

/// Decodes the image of a resource pack icon: a PNG, or a JPEG when the bytes
/// start like one. Vanilla reads `pack.png` with `ImageIO`, which goes by the
/// content and not by the name, and packs exist whose `pack.png` is a JPEG.
///
/// Same limits as [`decode_png`]: both sides at most `max_side`, checked from
/// the header before the pixels are unpacked.
#[must_use]
pub fn decode_image(bytes: &[u8], max_side: u32) -> Option<(u32, u32, Vec<u8>)> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        decode_jpeg(bytes, max_side)
    } else {
        decode_png(bytes, max_side)
    }
}

fn decode_jpeg(bytes: &[u8], max_side: u32) -> Option<(u32, u32, Vec<u8>)> {
    let mut decoder = jpeg_decoder::Decoder::new(std::io::Cursor::new(bytes));
    decoder.read_info().ok()?;
    let info = decoder.info()?;
    let (width, height) = (u32::from(info.width), u32::from(info.height));
    if width == 0 || height == 0 || width > max_side || height > max_side {
        return None;
    }
    let pixels = decoder.decode().ok()?;
    let rgba: Vec<u8> = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => pixels
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        jpeg_decoder::PixelFormat::L8 => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        // 16-bit grey and CMYK are not worth a path for an icon.
        _ => return None,
    };
    (rgba.len() == (width * height * 4) as usize).then_some((width, height, rgba))
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
    Some(rgba)
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
    fn a_pack_icon_of_any_size_up_to_the_limit_decodes_with_its_size() {
        let (width, height, rgba) = decode_png(&test_png(128, [1, 2, 3, 255]), 256).expect("valid");
        assert_eq!((width, height, rgba.len()), (128, 128, 128 * 128 * 4));
        assert_eq!(decode_png(&test_png(128, [0; 4]), 64), None, "too large");
        assert_eq!(decode_png(b"not a png", 256), None);
    }

    #[test]
    fn a_jpeg_named_pack_png_decodes_like_vanilla_imageio() {
        // An 8x8 orange JPEG, as a pack could ship under the name `pack.png`.
        let jpeg: &[u8] = include_bytes!("test_icon.jpg");
        let (width, height, rgba) = decode_image(jpeg, 64).expect("a valid JPEG");
        assert_eq!((width, height, rgba.len()), (8, 8, 8 * 8 * 4));
        assert!(
            rgba[0] > 150 && rgba[2] < 100,
            "orange, not grey: {:?}",
            &rgba[..4]
        );
        assert_eq!(decode_image(jpeg, 0), None, "over the size limit");
        assert!(decode_image(&test_png(8, [1, 2, 3, 255]), 64).is_some());
        assert_eq!(decode_image(&[0xFF, 0xD8, 0xFF, 0x00], 64), None);
    }

    #[test]
    fn another_size_or_garbage_is_refused() {
        assert_eq!(decode_favicon(&test_png(32, [0; 4])), None);
        assert_eq!(decode_favicon(b"not a png"), None);
        assert_eq!(decode_favicon(&[]), None);
    }
}

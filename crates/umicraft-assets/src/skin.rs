// SPDX-License-Identifier: MIT OR Apache-2.0

//! Player skin models, format conversion (64×32 to 64×64), default skins and Mojang API fetcher.

use serde::Deserialize;

use crate::error::AssetsError;

/// Skin model geometry variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SkinModel {
    /// Classic 4-pixel wide arm model (Steve).
    #[default]
    Classic,
    /// Slim 3-pixel wide arm model (Alex).
    Slim,
}

impl SkinModel {
    /// Returns true if this is the slim (Alex) arm model.
    #[must_use]
    pub const fn is_slim(self) -> bool {
        matches!(self, Self::Slim)
    }

    /// Derives the default model from a player UUID according to 1.8 rules.
    ///
    /// In vanilla 1.8, the model is Alex if the UUID's least significant bits XOR
    /// produces an odd result (`(uuid[15] & 1) != 0`).
    #[must_use]
    pub const fn default_for_uuid(uuid: [u8; 16]) -> Self {
        if (uuid[15] & 1) != 0 {
            Self::Slim
        } else {
            Self::Classic
        }
    }
}

/// Upgrades a legacy 64×32 skin RGBA buffer to modern 64×64 format.
///
/// In 1.8, 64×32 skins mirror the right arm and leg onto the left arm and leg
/// in the lower half of the 64×64 layout.
///
/// # Errors
/// [`AssetsError::Image`] if the input buffer is not exactly 64×32×4 bytes.
pub fn convert_64x32_to_64x64(raw: &[u8]) -> Result<Vec<u8>, AssetsError> {
    if raw.len() != 64 * 32 * 4 {
        return Err(AssetsError::Image {
            path: "skin".to_string(),
            reason: format!(
                "expected 64x32 RGBA skin (8192 bytes), got {} bytes",
                raw.len()
            ),
        });
    }

    let mut out = vec![0u8; 64 * 64 * 4];
    // Copy the original 64x32 upper half
    out[0..raw.len()].copy_from_slice(raw);

    // Copy and mirror right leg (0..16, 16..32) to left leg (16..32, 48..64)
    copy_sub_image(raw, 64, 32, 0, 16, &mut out, 64, 64, 16, 48, 16, 16);
    // Copy and mirror right arm (40..56, 16..32) to left arm (32..48, 48..64)
    copy_sub_image(raw, 64, 32, 40, 16, &mut out, 64, 64, 32, 48, 16, 16);

    Ok(out)
}

/// Helper to copy a rectangular region between two RGBA buffers.
#[allow(clippy::too_many_arguments)]
fn copy_sub_image(
    src: &[u8],
    src_w: usize,
    _src_h: usize,
    src_x: usize,
    src_y: usize,
    dst: &mut [u8],
    dst_w: usize,
    _dst_h: usize,
    dst_x: usize,
    dst_y: usize,
    w: usize,
    h: usize,
) {
    for y in 0..h {
        let src_row_start = ((src_y + y) * src_w + src_x) * 4;
        let dst_row_start = ((dst_y + y) * dst_w + dst_x) * 4;
        let bytes_to_copy = w * 4;
        dst[dst_row_start..dst_row_start + bytes_to_copy]
            .copy_from_slice(&src[src_row_start..src_row_start + bytes_to_copy]);
    }
}

/// Generates an original procedural fallback skin (64×64 RGBA8).
///
/// Ensures the client has a working, clean-room compliant skin texture without
/// downloading any assets.
#[must_use]
pub fn procedural_fallback_skin(model: SkinModel) -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 64 * 4];

    let skin_tone = [210, 160, 130, 255]; // Head / Arms
    let hair_color = [100, 65, 45, 255]; // Hair
    let shirt_color = if model.is_slim() {
        [100, 160, 100, 255] // Alex green
    } else {
        [0, 150, 170, 255] // Steve cyan
    };
    let pants_color = [60, 60, 150, 255]; // Blue jeans

    // Fill head (x: 0..32, y: 0..16)
    fill_rect(&mut rgba, 64, 0, 0, 32, 8, hair_color);
    fill_rect(&mut rgba, 64, 0, 8, 32, 8, skin_tone);

    // Torso (x: 16..40, y: 16..32)
    fill_rect(&mut rgba, 64, 16, 16, 24, 16, shirt_color);

    // Right arm (x: 40..56, y: 16..32) and left arm (x: 32..48, y: 48..64)
    fill_rect(&mut rgba, 64, 40, 16, 16, 16, skin_tone);
    fill_rect(&mut rgba, 64, 32, 48, 16, 16, skin_tone);

    // Right leg (x: 0..16, y: 16..32) and left leg (x: 16..32, y: 48..64)
    fill_rect(&mut rgba, 64, 0, 16, 16, 16, pants_color);
    fill_rect(&mut rgba, 64, 16, 48, 16, 16, pants_color);

    rgba
}

/// Generates an original procedural fallback zombie skin (64×64 RGBA8).
#[must_use]
pub fn procedural_zombie_skin() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 64 * 4];

    let skin_tone = [80, 130, 60, 255]; // Zombie green skin
    let hair_color = [40, 70, 30, 255]; // Dark green hair
    let shirt_color = [0, 110, 120, 255]; // Dark cyan tattered shirt
    let pants_color = [70, 45, 110, 255]; // Indigo/purple pants

    // Fill head
    fill_rect(&mut rgba, 64, 0, 0, 32, 8, hair_color);
    fill_rect(&mut rgba, 64, 0, 8, 32, 8, skin_tone);

    // Torso
    fill_rect(&mut rgba, 64, 16, 16, 24, 16, shirt_color);

    // Arms
    fill_rect(&mut rgba, 64, 40, 16, 16, 16, skin_tone);
    fill_rect(&mut rgba, 64, 32, 48, 16, 16, skin_tone);

    // Legs
    fill_rect(&mut rgba, 64, 0, 16, 16, 16, pants_color);
    fill_rect(&mut rgba, 64, 16, 48, 16, 16, pants_color);

    rgba
}

/// Generates an original procedural fallback cow texture (64×32 RGBA8).
#[must_use]
pub fn procedural_cow_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];

    let brown = [110, 80, 60, 255];
    let white = [220, 220, 220, 255];
    let horn = [160, 160, 150, 255];
    let udder = [230, 170, 180, 255];
    let hoof = [40, 40, 40, 255];

    // Base coat: brown with white patches
    for y in 0..32 {
        for x in 0..64 {
            let color = if ((x / 4) + (y / 4)) % 2 == 0 {
                brown
            } else {
                white
            };
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&color);
        }
    }

    // Horns (UV: 22..24, 0..3)
    fill_rect(&mut rgba, 64, 22, 0, 2, 4, horn);

    // Udder (UV: 52..62, 0..7 in MCP ModelCow)
    fill_rect(&mut rgba, 64, 52, 0, 10, 7, udder);

    // Hooves (bottom of legs)
    fill_rect(&mut rgba, 64, 0, 28, 16, 4, hoof);
    fill_rect(&mut rgba, 64, 16, 28, 16, 4, hoof);

    rgba
}

fn fill_rect(dst: &mut [u8], dst_w: usize, x: usize, y: usize, w: usize, h: usize, color: [u8; 4]) {
    for py in y..(y + h) {
        for px in x..(x + w) {
            let idx = (py * dst_w + px) * 4;
            dst[idx..idx + 4].copy_from_slice(&color);
        }
    }
}

#[derive(Deserialize)]
struct ProfileResponse {
    properties: Option<Vec<ProfileProperty>>,
}

#[derive(Deserialize)]
struct ProfileProperty {
    name: String,
    value: String,
}

#[derive(Deserialize)]
struct TexturesPayload {
    textures: Option<TexturesRecord>,
}

#[derive(Deserialize)]
struct TexturesRecord {
    #[serde(rename = "SKIN")]
    skin: Option<SkinRecord>,
}

#[derive(Deserialize)]
struct SkinRecord {
    url: String,
    metadata: Option<SkinMetadata>,
}

#[derive(Deserialize)]
struct SkinMetadata {
    model: Option<String>,
}

/// Resolved online skin metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSkin {
    /// Skin model (Classic or Slim).
    pub model: SkinModel,
    /// Decoded 64×64 RGBA8 pixel data.
    pub rgba: Vec<u8>,
}

/// Asynchronously fetches a player's official skin from Mojang session servers.
///
/// # Errors
/// [`AssetsError`] on HTTP network errors or if the skin texture cannot be decoded.
pub async fn fetch_mojang_skin(
    uuid: [u8; 16],
    http_client: &reqwest::Client,
) -> Result<Option<ResolvedSkin>, AssetsError> {
    let hex_uuid = uuid.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let url = format!("https://sessionserver.mojang.com/session/minecraft/profile/{hex_uuid}");

    let response = http_client
        .get(&url)
        .send()
        .await
        .map_err(|err| AssetsError::Http {
            url: url.clone(),
            source: err,
        })?;

    if response.status() == reqwest::StatusCode::NO_CONTENT
        || response.status() == reqwest::StatusCode::NOT_FOUND
    {
        return Ok(None);
    }

    if !response.status().is_success() {
        return Err(AssetsError::HttpStatus {
            url,
            status: response.status().as_u16(),
        });
    }

    let profile_bytes = response.bytes().await.map_err(|err| AssetsError::Http {
        url: url.clone(),
        source: err,
    })?;

    let profile: ProfileResponse =
        serde_json::from_slice(&profile_bytes).map_err(|err| AssetsError::Json {
            what: "mojang profile json".to_string(),
            source: err,
        })?;

    let Some(properties) = profile.properties else {
        return Ok(None);
    };

    let Some(textures_prop) = properties.iter().find(|p| p.name == "textures") else {
        return Ok(None);
    };

    let decoded_bytes = base64_decode(&textures_prop.value)?;
    let payload: TexturesPayload =
        serde_json::from_slice(&decoded_bytes).map_err(|err| AssetsError::Json {
            what: "profile textures payload".to_string(),
            source: err,
        })?;

    let Some(skin_record) = payload.textures.and_then(|t| t.skin) else {
        return Ok(None);
    };

    let model = match skin_record.metadata.and_then(|m| m.model).as_deref() {
        Some("slim") => SkinModel::Slim,
        _ => SkinModel::Classic,
    };

    // Download skin image
    let img_response = http_client
        .get(&skin_record.url)
        .send()
        .await
        .map_err(|err| AssetsError::Http {
            url: skin_record.url.clone(),
            source: err,
        })?;

    let img_bytes = img_response
        .bytes()
        .await
        .map_err(|err| AssetsError::Http {
            url: skin_record.url,
            source: err,
        })?;

    let (rgba, width, height) = decode_png_bytes(&img_bytes)?;
    let final_rgba = if width == 64 && height == 32 {
        convert_64x32_to_64x64(&rgba)?
    } else if width == 64 && height == 64 {
        rgba
    } else {
        return Err(AssetsError::Image {
            path: "skin".to_string(),
            reason: format!("invalid skin dimensions: {width}x{height}"),
        });
    };

    Ok(Some(ResolvedSkin {
        model,
        rgba: final_rgba,
    }))
}

fn base64_decode(input: &str) -> Result<Vec<u8>, AssetsError> {
    // Simple standard base64 decoding
    const B64_TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;

    for &b in input.as_bytes() {
        if b == b'=' || b.is_ascii_whitespace() {
            continue;
        }
        let val = B64_TABLE
            .iter()
            .position(|&c| c == b)
            .ok_or_else(|| AssetsError::Image {
                path: "base64".to_string(),
                reason: "invalid base64 character in skin texture payload".to_string(),
            })? as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Ok(out)
}

/// Decodes PNG byte data to RGBA8 pixels and (width, height).
///
/// # Errors
/// [`AssetsError::Image`] on corrupt PNG or unsupported color format.
pub fn decode_png_bytes(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), AssetsError> {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder.read_info().map_err(|err| AssetsError::Image {
        path: "skin.png".to_string(),
        reason: err.to_string(),
    })?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|err| AssetsError::Image {
            path: "skin.png".to_string(),
            reason: err.to_string(),
        })?;

    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => {
            let mut expanded = Vec::with_capacity((buf.len() / 3) * 4);
            for chunk in buf.chunks_exact(3) {
                expanded.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
            expanded
        }
        other => {
            return Err(AssetsError::Image {
                path: "skin.png".to_string(),
                reason: format!("unsupported png color type {other:?}"),
            });
        }
    };
    Ok((rgba, info.width, info.height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procedural_fallback_skin_dimensions() {
        let classic = procedural_fallback_skin(SkinModel::Classic);
        assert_eq!(classic.len(), 64 * 64 * 4);

        let slim = procedural_fallback_skin(SkinModel::Slim);
        assert_eq!(slim.len(), 64 * 64 * 4);
    }

    #[test]
    fn model_default_for_uuid() {
        let mut uuid = [0u8; 16];
        uuid[15] = 0;
        assert_eq!(SkinModel::default_for_uuid(uuid), SkinModel::Classic);

        uuid[15] = 1;
        assert_eq!(SkinModel::default_for_uuid(uuid), SkinModel::Slim);
    }

    #[test]
    fn convert_64x32_to_64x64_preserves_upper_half() {
        let mut old = vec![0u8; 64 * 32 * 4];
        old[0..4].copy_from_slice(&[10, 20, 30, 255]);
        let new = convert_64x32_to_64x64(&old).unwrap();
        assert_eq!(new.len(), 64 * 64 * 4);
        assert_eq!(&new[0..4], &[10, 20, 30, 255]);
    }
}

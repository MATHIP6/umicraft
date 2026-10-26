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

    /// Derives the default model from a player UUID using the vanilla 1.8 algorithm.
    ///
    /// Java's `UUID.hashCode()` XORs the two 64-bit halves, then folds the result
    /// into 32 bits.  The model is Alex (slim) when the lowest bit is set.
    #[must_use]
    pub const fn default_for_uuid(uuid: [u8; 16]) -> Self {
        let msb = u64::from_be_bytes([
            uuid[0], uuid[1], uuid[2], uuid[3], uuid[4], uuid[5], uuid[6], uuid[7],
        ]);
        let lsb = u64::from_be_bytes([
            uuid[8], uuid[9], uuid[10], uuid[11], uuid[12], uuid[13], uuid[14], uuid[15],
        ]);
        let hilo = msb ^ lsb;
        let hash = ((hilo >> 32) as u32) ^ (hilo as u32);
        if hash & 1 != 0 {
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

    // Mirror right leg (0..16, 16..32) to left leg (16..32, 48..64)
    copy_sub_image_mirrored(raw, 64, 0, 16, &mut out, 64, 16, 48, 16, 16);
    // Mirror right arm (40..56, 16..32) to left arm (32..48, 48..64)
    copy_sub_image_mirrored(raw, 64, 40, 16, &mut out, 64, 32, 48, 16, 16);

    Ok(out)
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

/// Generates an original procedural fallback skeleton skin (64×32 RGBA8).
#[must_use]
pub fn procedural_skeleton_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];

    let bone = [200, 200, 190, 255];
    let dark_bone = [140, 140, 130, 255];
    let eye = [40, 40, 40, 255];

    // Head
    fill_rect(&mut rgba, 64, 0, 0, 32, 16, bone);
    fill_rect(&mut rgba, 64, 8, 10, 3, 3, eye);
    fill_rect(&mut rgba, 64, 13, 10, 3, 3, eye);

    // Torso
    fill_rect(&mut rgba, 64, 16, 16, 24, 16, dark_bone);

    // Thin arms and legs
    fill_rect(&mut rgba, 64, 40, 16, 16, 16, bone);
    fill_rect(&mut rgba, 64, 0, 16, 16, 16, bone);

    rgba
}

/// Generates an original procedural fallback creeper texture (64×32 RGBA8).
#[must_use]
pub fn procedural_creeper_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];

    let green = [80, 140, 55, 255];
    let dark_green = [50, 100, 35, 255];
    let eye_black = [20, 20, 20, 255];

    // Mottled green pattern
    for y in 0..32 {
        for x in 0..64 {
            let idx = (y * 64 + x) * 4;
            let color = if ((x / 3) + (y / 3)) % 2 == 0 {
                green
            } else {
                dark_green
            };
            rgba[idx..idx + 4].copy_from_slice(&color);
        }
    }

    // Eyes (in head UV region)
    fill_rect(&mut rgba, 64, 8, 10, 2, 4, eye_black);
    fill_rect(&mut rgba, 64, 14, 10, 2, 4, eye_black);

    rgba
}

/// Generates an original procedural fallback sheep body texture (64×32 RGBA8).
#[must_use]
pub fn procedural_sheep_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];

    let wool = [220, 220, 215, 255];
    let face = [180, 160, 150, 255];
    let hoof = [50, 45, 40, 255];

    // Wool base
    for y in 0..32 {
        for x in 0..64 {
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&wool);
        }
    }

    // Face (head region)
    fill_rect(&mut rgba, 64, 6, 6, 8, 8, face);
    // Hooves
    fill_rect(&mut rgba, 64, 0, 28, 16, 4, hoof);

    rgba
}

/// Generates an original procedural fallback sheep fur overlay texture (64×32 RGBA8).
///
/// Used with `ModelSheep1` (the wool layer) — separate from the base body texture.
#[must_use]
pub fn procedural_sheep_fur_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];

    for y in 0..32 {
        for x in 0..64 {
            let idx = (y * 64 + x) * 4;
            let noise = ((x * 13 + y * 7) ^ (x * 3 + y * 11)) % 20;
            let shade = 225_u8.saturating_add(noise as u8).min(245);
            rgba[idx] = shade;
            rgba[idx + 1] = shade;
            rgba[idx + 2] = shade.saturating_sub(5);
            rgba[idx + 3] = 255;
        }
    }

    rgba
}

/// Generates an original procedural fallback pig texture (64×32 RGBA8).
#[must_use]
pub fn procedural_pig_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];

    let skin = [230, 165, 145, 255];
    let snout = [200, 140, 120, 255];
    let hoof = [90, 70, 55, 255];

    // Base coat: solid pink
    for y in 0..32 {
        for x in 0..64 {
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&skin);
        }
    }

    // Snout (UV: 16..24, 16..21 in MCP ModelPig)
    fill_rect(&mut rgba, 64, 16, 17, 8, 4, snout);

    // Hooves (bottom of legs)
    fill_rect(&mut rgba, 64, 0, 28, 16, 4, hoof);

    rgba
}

/// Generates an original procedural fallback chicken texture (64×32 RGBA8).
#[must_use]
pub fn procedural_chicken_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];

    let feather = [220, 220, 220, 255];
    let beak = [200, 130, 40, 255];
    let wattle = [200, 50, 50, 255];
    let comb = [180, 40, 40, 255];
    let leg = [200, 170, 50, 255];

    // Base coat: white feathers
    for y in 0..32 {
        for x in 0..64 {
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&feather);
        }
    }

    // Beak (UV: 14..22, 0..4)
    fill_rect(&mut rgba, 64, 14, 0, 8, 4, beak);
    // Wattle (UV: 14..20, 4..8)
    fill_rect(&mut rgba, 64, 14, 4, 6, 4, wattle);
    // Head comb hint (top of head UV area)
    fill_rect(&mut rgba, 64, 2, 0, 4, 2, comb);
    // Legs (UV: 26..35, 0..8)
    fill_rect(&mut rgba, 64, 26, 0, 9, 8, leg);
    // Wing feathers (UV: 24..32, 13..23)
    fill_rect(&mut rgba, 64, 24, 13, 8, 10, feather);

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

/// Generates an original procedural fallback spider texture (64×32 RGBA8).
#[must_use]
pub fn procedural_spider_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];
    let dark = [20, 18, 16, 255];
    let red_eyes = [180, 20, 20, 255];
    for y in 0..32 {
        for x in 0..64 {
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&dark);
        }
    }
    // Red eyes on head (UV: 32..40, 4..12)
    fill_rect(&mut rgba, 64, 34, 8, 2, 2, red_eyes);
    fill_rect(&mut rgba, 64, 38, 8, 2, 2, red_eyes);
    rgba
}

/// Generates an original procedural fallback cave spider texture (64×32 RGBA8).
#[must_use]
pub fn procedural_cave_spider_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];
    let blue_dark = [16, 32, 45, 255];
    let red_eyes = [200, 20, 20, 255];
    for y in 0..32 {
        for x in 0..64 {
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&blue_dark);
        }
    }
    fill_rect(&mut rgba, 64, 34, 8, 2, 2, red_eyes);
    fill_rect(&mut rgba, 64, 38, 8, 2, 2, red_eyes);
    rgba
}

/// Generates an original procedural fallback enderman texture (64×32 RGBA8).
#[must_use]
pub fn procedural_enderman_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];
    let black = [18, 18, 18, 255];
    let white = [230, 230, 230, 255];
    let pupil = [180, 40, 220, 255];
    for y in 0..32 {
        for x in 0..64 {
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&black);
        }
    }
    // Eyes on head front face (X=8..16, Y=8..16)
    fill_rect(&mut rgba, 64, 8, 12, 3, 1, white);
    fill_rect(&mut rgba, 64, 13, 12, 3, 1, white);
    fill_rect(&mut rgba, 64, 9, 12, 1, 1, pupil);
    fill_rect(&mut rgba, 64, 14, 12, 1, 1, pupil);
    // Cutout for mouth on head
    fill_rect(&mut rgba, 64, 8, 14, 8, 2, [0, 0, 0, 0]);
    // Teeth on headwear front face (X=8..16, Y=24..32)
    fill_rect(&mut rgba, 64, 8, 24, 8, 5, [0, 0, 0, 0]);
    fill_rect(&mut rgba, 64, 9, 29, 2, 2, [220, 220, 220, 255]);
    fill_rect(&mut rgba, 64, 13, 29, 2, 2, [220, 220, 220, 255]);
    rgba
}

/// Generates an original procedural fallback slime texture (64×32 RGBA8).
#[must_use]
pub fn procedural_slime_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];
    let gel = [100, 180, 80, 180];
    let eye = [20, 50, 15, 255];
    for y in 0..32 {
        for x in 0..64 {
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&gel);
        }
    }
    fill_rect(&mut rgba, 64, 32, 0, 4, 4, eye);
    rgba
}

/// Generates an original procedural fallback magma cube texture (64×32 RGBA8).
#[must_use]
pub fn procedural_magma_cube_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];
    let magma = [180, 50, 20, 255];
    let orange = [220, 130, 20, 255];
    for y in 0..32 {
        for x in 0..64 {
            let color = if (x + y) % 2 == 0 { magma } else { orange };
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&color);
        }
    }
    rgba
}

/// Generates an original procedural fallback zombie pigman texture (64×32 RGBA8).
#[must_use]
pub fn procedural_zombie_pigman_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];
    let pig_skin = [230, 160, 150, 255];
    let rot = [80, 110, 60, 255];
    for y in 0..32 {
        for x in 0..64 {
            let color = if (x / 4 + y / 4) % 2 == 0 {
                pig_skin
            } else {
                rot
            };
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&color);
        }
    }
    rgba
}

/// Generates an original procedural fallback villager texture (64×64 RGBA8).
#[must_use]
pub fn procedural_villager_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 64 * 4];
    let brown_robe = [120, 80, 50, 255];
    let face = [200, 150, 130, 255];
    let nose = [180, 130, 110, 255];
    for y in 0..64 {
        for x in 0..64 {
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&brown_robe);
        }
    }
    fill_rect(&mut rgba, 64, 0, 0, 16, 16, face);
    fill_rect(&mut rgba, 64, 24, 0, 8, 8, nose);
    rgba
}

/// Generates an original procedural fallback zombie villager texture (64×64 RGBA8).
#[must_use]
pub fn procedural_zombie_villager_texture() -> Vec<u8> {
    let mut rgba = procedural_zombie_skin();
    let green_face = [80, 130, 60, 255];
    let dark_green = [50, 90, 40, 255];
    // Villager elongated head at (0, 32, 32, 32)
    fill_rect(&mut rgba, 64, 0, 32, 32, 10, dark_green);
    fill_rect(&mut rgba, 64, 0, 42, 32, 22, green_face);
    // Nose at (24, 32, 8, 8)
    fill_rect(&mut rgba, 64, 24, 32, 8, 8, dark_green);
    rgba
}

/// Generates an original procedural fallback witch texture (64×128 RGBA8).
#[must_use]
pub fn procedural_witch_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 128 * 4];
    let purple_robe = [70, 30, 80, 255];
    let black_hat = [30, 25, 35, 255];
    let face = [180, 160, 140, 255];
    for y in 0..128 {
        for x in 0..64 {
            let color = if y >= 64 { black_hat } else { purple_robe };
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&color);
        }
    }
    fill_rect(&mut rgba, 64, 0, 0, 16, 16, face);
    rgba
}

/// Generates an original procedural fallback iron golem texture (128×128 RGBA8).
#[must_use]
pub fn procedural_iron_golem_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 128 * 128 * 4];
    let iron = [210, 210, 205, 255];
    let vine = [90, 130, 60, 255];
    let eye = [180, 40, 40, 255];
    for y in 0..128 {
        for x in 0..128 {
            let color = if (x * 7 + y * 13) % 31 == 0 {
                vine
            } else {
                iron
            };
            let idx = (y * 128 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&color);
        }
    }
    fill_rect(&mut rgba, 128, 10, 10, 4, 2, eye);
    fill_rect(&mut rgba, 128, 18, 10, 4, 2, eye);
    rgba
}

/// Generates an original procedural fallback snowman texture (64×64 RGBA8).
#[must_use]
pub fn procedural_snowman_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 64 * 4];
    let snow = [240, 245, 250, 255];
    let branch = [90, 60, 35, 255];
    let coal = [30, 30, 30, 255];
    for y in 0..64 {
        for x in 0..64 {
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&snow);
        }
    }
    fill_rect(&mut rgba, 64, 32, 0, 16, 8, branch);
    fill_rect(&mut rgba, 64, 3, 4, 2, 2, coal);
    fill_rect(&mut rgba, 64, 7, 4, 2, 2, coal);
    rgba
}

/// Generates an original procedural fallback wolf texture (64×32 RGBA8).
#[must_use]
pub fn procedural_wolf_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];
    let grey = [180, 180, 180, 255];
    let dark_grey = [120, 120, 120, 255];
    let nose = [30, 30, 30, 255];
    for y in 0..32 {
        for x in 0..64 {
            let color = if (x + y) % 4 == 0 { dark_grey } else { grey };
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&color);
        }
    }
    fill_rect(&mut rgba, 64, 1, 11, 2, 2, nose);
    rgba
}

/// Generates an original procedural fallback mooshroom cow texture (64×32 RGBA8).
#[must_use]
pub fn procedural_mooshroom_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];
    let red = [160, 25, 25, 255];
    let white = [230, 230, 230, 255];
    for y in 0..32 {
        for x in 0..64 {
            let color = if (x / 4 + y / 4) % 2 == 0 { red } else { white };
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&color);
        }
    }
    rgba
}

/// Generates an original procedural fallback squid texture (64×32 RGBA8).
#[must_use]
pub fn procedural_squid_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];
    let ink = [35, 45, 65, 255];
    let eye = [220, 220, 220, 255];
    for y in 0..32 {
        for x in 0..64 {
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&ink);
        }
    }
    fill_rect(&mut rgba, 64, 4, 12, 2, 2, eye);
    fill_rect(&mut rgba, 64, 10, 12, 2, 2, eye);
    rgba
}

/// Generates an original procedural fallback bat texture (64×64 RGBA8).
#[must_use]
pub fn procedural_bat_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 64 * 4];
    let dark = [45, 35, 30, 255];
    let wing = [30, 25, 20, 255];
    for y in 0..64 {
        for x in 0..64 {
            let color = if x >= 24 { wing } else { dark };
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&color);
        }
    }
    rgba
}

/// Generates an original procedural fallback blaze texture (64×32 RGBA8).
#[must_use]
pub fn procedural_blaze_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];
    let yellow = [240, 180, 20, 255];
    let orange = [220, 100, 10, 255];
    let eye = [255, 255, 100, 255];
    for y in 0..32 {
        for x in 0..64 {
            let color = if (x + y) % 3 == 0 { orange } else { yellow };
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&color);
        }
    }
    fill_rect(&mut rgba, 64, 2, 3, 2, 2, eye);
    fill_rect(&mut rgba, 64, 5, 3, 2, 2, eye);
    rgba
}

/// Generates an original procedural fallback ender dragon texture (256×256 RGBA8).
#[must_use]
pub fn procedural_dragon_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 256 * 256 * 4];
    let black = [15, 15, 15, 255];
    let dark_grey = [30, 30, 30, 255];
    let eye = [200, 50, 230, 255];
    for y in 0..256 {
        for x in 0..256 {
            let color = if (x * 3 + y * 7) % 17 == 0 {
                dark_grey
            } else {
                black
            };
            let idx = (y * 256 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&color);
        }
    }
    // Eyes
    fill_rect(&mut rgba, 256, 120, 40, 8, 4, eye);
    fill_rect(&mut rgba, 256, 140, 40, 8, 4, eye);
    rgba
}

/// Generates an original procedural fallback endermite texture (64×32 RGBA8).
#[must_use]
pub fn procedural_endermite_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];
    let dark_purple = [35, 20, 45, 255];
    let eye = [180, 40, 200, 255];
    for y in 0..32 {
        for x in 0..64 {
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&dark_purple);
        }
    }
    fill_rect(&mut rgba, 64, 1, 1, 2, 1, eye);
    fill_rect(&mut rgba, 64, 3, 1, 2, 1, eye);
    rgba
}

/// Generates an original procedural fallback ghast texture (64×32 RGBA8).
#[must_use]
pub fn procedural_ghast_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];
    let white = [240, 240, 240, 255];
    let grey_face = [160, 160, 160, 255];
    for y in 0..32 {
        for x in 0..64 {
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&white);
        }
    }
    // Face (eyes and mouth)
    fill_rect(&mut rgba, 64, 3, 4, 2, 2, grey_face);
    fill_rect(&mut rgba, 64, 7, 4, 2, 2, grey_face);
    fill_rect(&mut rgba, 64, 4, 7, 4, 2, grey_face);
    rgba
}

/// Generates an original procedural fallback guardian texture (64×64 RGBA8).
#[must_use]
pub fn procedural_guardian_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 64 * 4];
    let teal = [80, 130, 130, 255];
    let orange_spike = [200, 100, 50, 255];
    let eye = [180, 40, 30, 255];
    for y in 0..64 {
        for x in 0..64 {
            let color = if y >= 40 { orange_spike } else { teal };
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&color);
        }
    }
    // Center eye
    fill_rect(&mut rgba, 64, 8, 0, 4, 4, eye);
    rgba
}

/// Generates an original procedural fallback horse texture (128×128 RGBA8).
#[must_use]
pub fn procedural_horse_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 128 * 128 * 4];
    let chestnut = [135, 75, 45, 255];
    let dark_mane = [40, 25, 20, 255];
    let white_marking = [220, 220, 220, 255];
    for y in 0..128 {
        for x in 0..128 {
            let idx = (y * 128 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&chestnut);
        }
    }
    fill_rect(&mut rgba, 128, 56, 0, 8, 32, dark_mane);
    fill_rect(&mut rgba, 128, 0, 0, 8, 8, white_marking);
    rgba
}

/// Generates an original procedural fallback ocelot texture (64×32 RGBA8).
#[must_use]
pub fn procedural_ocelot_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];
    let yellow = [215, 170, 70, 255];
    let dark_spot = [120, 80, 40, 255];
    let green_eye = [80, 180, 60, 255];
    for y in 0..32 {
        for x in 0..64 {
            let color = if (x * 5 + y * 11) % 13 == 0 {
                dark_spot
            } else {
                yellow
            };
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&color);
        }
    }
    fill_rect(&mut rgba, 64, 1, 1, 1, 1, green_eye);
    fill_rect(&mut rgba, 64, 3, 1, 1, 1, green_eye);
    rgba
}

/// Generates an original procedural fallback rabbit texture (64×32 RGBA8).
#[must_use]
pub fn procedural_rabbit_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];
    let tan = [170, 135, 105, 255];
    let white = [225, 220, 215, 255];
    let pink_ear = [220, 160, 160, 255];
    for y in 0..32 {
        for x in 0..64 {
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&tan);
        }
    }
    // Chest / belly
    fill_rect(&mut rgba, 64, 0, 0, 8, 6, white);
    // Ears
    fill_rect(&mut rgba, 64, 52, 0, 4, 5, pink_ear);
    rgba
}

/// Generates an original procedural fallback silverfish texture (64×32 RGBA8).
#[must_use]
pub fn procedural_silverfish_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 32 * 4];
    let silver = [140, 145, 150, 255];
    let dark_silver = [100, 105, 110, 255];
    for y in 0..32 {
        for x in 0..64 {
            let color = if (x + y) % 2 == 0 {
                silver
            } else {
                dark_silver
            };
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&color);
        }
    }
    rgba
}

/// Generates an original procedural fallback wither texture (64×64 RGBA8).
#[must_use]
pub fn procedural_wither_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 64 * 64 * 4];
    let wither_dark = [25, 25, 25, 255];
    let rib_grey = [50, 50, 55, 255];
    let eye = [210, 215, 220, 255];
    for y in 0..64 {
        for x in 0..64 {
            let color = if y % 4 == 0 { rib_grey } else { wither_dark };
            let idx = (y * 64 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&color);
        }
    }
    // Eyes on main head (UV: 0..8, 0..8)
    fill_rect(&mut rgba, 64, 2, 3, 2, 1, eye);
    fill_rect(&mut rgba, 64, 5, 3, 2, 1, eye);
    rgba
}

/// Copies a rectangular region between two RGBA buffers with horizontal flip.
#[allow(clippy::too_many_arguments)]
fn copy_sub_image_mirrored(
    src: &[u8],
    src_w: usize,
    src_x: usize,
    src_y: usize,
    dst: &mut [u8],
    dst_w: usize,
    dst_x: usize,
    dst_y: usize,
    w: usize,
    h: usize,
) {
    for y in 0..h {
        for x in 0..w {
            let src_idx = ((src_y + y) * src_w + src_x + x) * 4;
            let dst_idx = ((dst_y + y) * dst_w + dst_x + (w - 1 - x)) * 4;
            dst[dst_idx..dst_idx + 4].copy_from_slice(&src[src_idx..src_idx + 4]);
        }
    }
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
    let mut decoder = png::Decoder::new(bytes);
    decoder.set_transformations(png::Transformations::EXPAND);
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
    let (output_color, _bits) = reader.output_color_type();
    let rgba = match output_color {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => {
            let mut expanded = Vec::with_capacity((buf.len() / 3) * 4);
            for chunk in buf.chunks_exact(3) {
                expanded.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
            expanded
        }
        png::ColorType::Grayscale => {
            let mut expanded = Vec::with_capacity(buf.len() * 4);
            for &gray in &buf {
                expanded.extend_from_slice(&[gray, gray, gray, 255]);
            }
            expanded
        }
        png::ColorType::GrayscaleAlpha => {
            let mut expanded = Vec::with_capacity((buf.len() / 2) * 4);
            for chunk in buf.chunks_exact(2) {
                expanded.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]]);
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
        // All-zero UUID: msb=0, lsb=0, hash=0, even → Classic
        assert_eq!(SkinModel::default_for_uuid([0u8; 16]), SkinModel::Classic);

        // UUID where hash folds to an odd number → Slim.
        // msb = 0x00..01, lsb = 0x00..00 → hilo = 0x00..01 → hash = 0^1 = 1 (odd)
        let mut uuid = [0u8; 16];
        uuid[7] = 1;
        assert_eq!(SkinModel::default_for_uuid(uuid), SkinModel::Slim);

        // UUID where XOR cancels both halves → Classic.
        // msb = lsb → hilo = 0 → hash = 0 (even)
        let uuid = [0xAB; 16];
        assert_eq!(SkinModel::default_for_uuid(uuid), SkinModel::Classic);
    }

    #[test]
    fn convert_64x32_to_64x64_preserves_upper_half() {
        let mut old = vec![0u8; 64 * 32 * 4];
        old[0..4].copy_from_slice(&[10, 20, 30, 255]);
        let new = convert_64x32_to_64x64(&old).unwrap();
        assert_eq!(new.len(), 64 * 64 * 4);
        assert_eq!(&new[0..4], &[10, 20, 30, 255]);
    }

    #[test]
    fn convert_64x32_mirrors_limb_textures_horizontally() {
        let mut old = vec![0u8; 64 * 32 * 4];
        // Paint pixel (0, 16) in the right leg region with a unique color.
        let src_idx = (16 * 64) * 4;
        old[src_idx..src_idx + 4].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xFF]);

        let new = convert_64x32_to_64x64(&old).unwrap();

        // The right leg (x:0..16, y:16..32) is mirrored to left leg (x:16..32, y:48..64).
        // Source pixel at local (0, 0) of the 16-wide strip should appear at
        // destination local (15, 0) due to horizontal flip: dst (16+15, 48) = (31, 48).
        let dst_idx = (48 * 64 + 31) * 4;
        assert_eq!(
            &new[dst_idx..dst_idx + 4],
            &[0xAA, 0xBB, 0xCC, 0xFF],
            "right-leg pixel at src (0,16) should mirror to dst (31,48)"
        );

        // The un-flipped position (16, 48) should NOT have our color.
        let wrong_idx = (48 * 64 + 16) * 4;
        assert_ne!(
            &new[wrong_idx..wrong_idx + 4],
            &[0xAA, 0xBB, 0xCC, 0xFF],
            "mirrored copy must flip horizontally, not plain-copy"
        );
    }

    #[test]
    fn all_procedural_textures_have_expected_sizes() {
        assert_eq!(procedural_spider_texture().len(), 64 * 32 * 4);
        assert_eq!(procedural_cave_spider_texture().len(), 64 * 32 * 4);
        assert_eq!(procedural_enderman_texture().len(), 64 * 32 * 4);
        assert_eq!(procedural_slime_texture().len(), 64 * 32 * 4);
        assert_eq!(procedural_magma_cube_texture().len(), 64 * 32 * 4);
        assert_eq!(procedural_zombie_pigman_texture().len(), 64 * 32 * 4);
        assert_eq!(procedural_villager_texture().len(), 64 * 64 * 4);
        assert_eq!(procedural_zombie_villager_texture().len(), 64 * 64 * 4);
        assert_eq!(procedural_witch_texture().len(), 64 * 128 * 4);
        assert_eq!(procedural_iron_golem_texture().len(), 128 * 128 * 4);
        assert_eq!(procedural_snowman_texture().len(), 64 * 64 * 4);
        assert_eq!(procedural_wolf_texture().len(), 64 * 32 * 4);
        assert_eq!(procedural_mooshroom_texture().len(), 64 * 32 * 4);
        assert_eq!(procedural_squid_texture().len(), 64 * 32 * 4);
        assert_eq!(procedural_bat_texture().len(), 64 * 64 * 4);
        assert_eq!(procedural_blaze_texture().len(), 64 * 32 * 4);
        assert_eq!(procedural_dragon_texture().len(), 256 * 256 * 4);
        assert_eq!(procedural_endermite_texture().len(), 64 * 32 * 4);
        assert_eq!(procedural_ghast_texture().len(), 64 * 32 * 4);
        assert_eq!(procedural_guardian_texture().len(), 64 * 64 * 4);
        assert_eq!(procedural_horse_texture().len(), 128 * 128 * 4);
        assert_eq!(procedural_ocelot_texture().len(), 64 * 32 * 4);
        assert_eq!(procedural_rabbit_texture().len(), 64 * 32 * 4);
        assert_eq!(procedural_silverfish_texture().len(), 64 * 32 * 4);
        assert_eq!(procedural_wither_texture().len(), 64 * 64 * 4);
    }
}

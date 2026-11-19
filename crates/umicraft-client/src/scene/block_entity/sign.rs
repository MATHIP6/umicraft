// SPDX-License-Identifier: MIT OR Apache-2.0

//! Signs (MCP `TileEntitySignRenderer`).
//!
//! The plate and its post are the `SignModel`. The four lines are drawn on a
//! plane in front of the board with the font of the UI: one quad per glyph,
//! laid out in font pixels (x right, y down) as `drawString` does and mapped
//! to the block by [`sign_text_transform`].
//!
//! The reference draws each line as the formatted text of the first line of
//! the component wrapped to 90 pixels (`GuiUtilRenderComponents.func_178908_a`
//! with the colours forced on), centred on the plane's origin, in black unless
//! the component says otherwise; [`sign_text`](super::sign_text) lays the
//! styles out. The reference has no distance of its own for the text: it is
//! drawn with the tile entity, so the cut of `mod.rs` applies to both.

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use umicraft_game::server::block_entities::Pos;
use umicraft_mesh::{BlockEntityKind, BlockEntityRef};
use umicraft_model::entity::{
    sign_text_transform, standing_sign_degrees, wall_sign_degrees, SignModel, TextureSlot,
};
use umicraft_render::EntityVertex;
use umicraft_ui::FontAtlas;

use super::sign_text::{self, TextQuad};
use super::{light_word, push_at_block, BlockEntityScene};
use crate::hud::chat;

/// The widest a line is drawn, in font pixels (the `90` of the reference).
pub(crate) const MAX_LINE_WIDTH: f32 = 90.0;
/// Pixels between the tops of two lines (`j * 10`).
const LINE_SPACING: f32 = 10.0;
/// Lines of a sign.
const LINES: usize = 4;
/// The light-word bits that colour a vertex with the chat colour of bits
/// 14..=17: the dye and orb bits together (`entity.wgsl`, `CHAT_COLORS`).
const CHAT_COLOR_BITS: u32 = (1 << 13) | (1 << 18);
/// Shift of the chat colour index in the light word.
const CHAT_COLOR_SHIFT: u32 = 14;
/// Client ticks per second, the rate the obfuscated glyphs change at.
const TICKS_PER_SECOND: f32 = 20.0;

/// Draws the sign at `entry`, with its text when the lines are known.
pub(super) fn add_sign(
    batches: &mut HashMap<TextureSlot, Vec<EntityVertex>>,
    entry: &BlockEntityRef,
    scene: &BlockEntityScene<'_>,
) {
    let standing = BlockEntityKind::of(entry.state) == Some(BlockEntityKind::StandingSign);
    let degrees = rotation_degrees(standing, entry.state.metadata());
    push_at_block(
        batches.entry(TextureSlot::Sign).or_default(),
        SignModel::emit(standing, degrees),
        entry,
    );
    let [x, y, z] = scene.world_position(entry);
    let pos: Pos = (x, y, z);
    let (Some(font), Some(lines)) = (scene.font, scene.signs.get(&pos)) else {
        return;
    };
    let tick = (scene.seconds * TICKS_PER_SECOND) as u32;
    let quads = line_quads(font, lines, tick);
    if !quads.is_empty() {
        let out = batches.entry(TextureSlot::SignText).or_default();
        push_text(out, &quads, standing, degrees, entry);
    }
}

/// The turn of the sign from its block metadata.
fn rotation_degrees(standing: bool, metadata: u8) -> f32 {
    if standing {
        standing_sign_degrees(metadata)
    } else {
        wall_sign_degrees(metadata)
    }
}

/// The quads of the four lines, in font pixels around the plane's origin.
fn line_quads(font: &FontAtlas, lines: &[String; LINES], tick: u32) -> Vec<TextQuad> {
    let mut out = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let row = sign_text::first_row(font, &chat::parse_runs(line), MAX_LINE_WIDTH);
        // `-getStringWidth(s) / 2`, an integer division.
        let left = -(sign_text::width(font, &row) as i32 / 2) as f32;
        let top = index as f32 * LINE_SPACING - (LINES * 5) as f32;
        sign_text::layout(font, &row, (left, top), tick, &mut out);
    }
    out
}

/// Maps the quads onto the sign at `entry` and appends their two triangles
/// each, lit like the sign and coloured by their chat colour.
fn push_text(
    out: &mut Vec<EntityVertex>,
    quads: &[TextQuad],
    standing: bool,
    degrees: f32,
    entry: &BlockEntityRef,
) {
    let [x, y, z] = entry.position;
    let plane = Mat4::from_translation(Vec3::new(x as f32, y as f32, z as f32))
        * sign_text_transform(standing, degrees);
    let normal = plane
        .transform_vector3(Vec3::Z)
        .normalize_or_zero()
        .to_array();
    let light = light_word(entry) | CHAT_COLOR_BITS;
    for quad in quads {
        let light = light | (u32::from(quad.color & 15) << CHAT_COLOR_SHIFT);
        let [u0, v0, u1, v1] = quad.uv;
        let uvs = [[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
        let corner = |index: usize| EntityVertex {
            position: plane
                .transform_point3(Vec3::new(
                    quad.corners[index][0],
                    quad.corners[index][1],
                    0.0,
                ))
                .to_array(),
            normal,
            uv: uvs[index],
            light,
        };
        out.extend([0, 1, 2, 0, 2, 3].map(corner));
    }
}

#[cfg(test)]
mod tests {
    use umicraft_game::server::block_entities::SignTable;
    use umicraft_world::{BlockState, LightLevel};

    use super::*;

    const STANDING_SIGN: u16 = 63;
    const WALL_SIGN: u16 = 68;

    fn entry(id: u16, metadata: u8) -> BlockEntityRef {
        BlockEntityRef {
            position: [4, 70, 9],
            state: BlockState::new(id, metadata),
            same_neighbour: umicraft_mesh::SameNeighbours::default(),
            light: LightLevel { block: 0, sky: 15 },
        }
    }

    fn lines(texts: [&str; 4]) -> SignTable {
        SignTable::from([((4, 70, 9), texts.map(|text| format!("\"{text}\"")))])
    }

    fn draw(
        entry: &BlockEntityRef,
        signs: &SignTable,
        font: Option<&FontAtlas>,
    ) -> HashMap<TextureSlot, Vec<EntityVertex>> {
        let lids = umicraft_game::server::chest_lids::ChestLids::default();
        let scene = BlockEntityScene {
            data: &umicraft_game::server::block_entities::EntityTable::new(),
            seconds: 0.0,
            skins: &crate::scene::entity::NoSkins,
            lids: &lids,
            signs,
            font,
            partial_tick: 0.0,
            camera: Vec3::ZERO,
            origin: [0; 3],
        };
        let mut batches = HashMap::new();
        add_sign(&mut batches, entry, &scene);
        batches
    }

    #[test]
    fn a_standing_sign_has_a_plate_and_a_post_and_a_wall_sign_only_a_plate() {
        let none = SignTable::new();
        assert_eq!(
            draw(&entry(STANDING_SIGN, 0), &none, None)[&TextureSlot::Sign].len(),
            72
        );
        assert_eq!(
            draw(&entry(WALL_SIGN, 3), &none, None)[&TextureSlot::Sign].len(),
            36
        );
    }

    #[test]
    fn a_line_makes_one_quad_per_visible_glyph_and_an_empty_sign_none() {
        let font = FontAtlas::fallback();
        let hello = lines(["Hi a", "", "", ""]);
        let batches = draw(&entry(STANDING_SIGN, 0), &hello, Some(&font));
        // "Hi a": three glyphs, the space is not drawn.
        assert_eq!(batches[&TextureSlot::SignText].len(), 3 * 6);
        let empty = lines(["", "", "", ""]);
        let batches = draw(&entry(STANDING_SIGN, 0), &empty, Some(&font));
        assert!(!batches.contains_key(&TextureSlot::SignText));
    }

    #[test]
    fn no_text_is_drawn_without_a_font_or_without_lines() {
        let font = FontAtlas::fallback();
        let hello = lines(["Hi", "", "", ""]);
        assert!(!draw(&entry(WALL_SIGN, 2), &hello, None).contains_key(&TextureSlot::SignText));
        let unknown = SignTable::new();
        assert!(
            !draw(&entry(WALL_SIGN, 2), &unknown, Some(&font)).contains_key(&TextureSlot::SignText)
        );
    }

    #[test]
    fn a_long_line_is_cut_to_ninety_pixels() {
        let font = FontAtlas::fallback();
        let long = "a".repeat(100);
        let quads = line_quads(
            &font,
            &[format!("\"{long}\""), "".into(), "".into(), "".into()],
            0,
        );
        let right = quads
            .iter()
            .flat_map(|quad| quad.corners)
            .map(|[x, _]| x)
            .fold(f32::MIN, f32::max);
        assert!(right <= MAX_LINE_WIDTH / 2.0 + 1.0, "{right}");
    }

    #[test]
    fn coloured_text_carries_its_chat_colour_in_the_light_word() {
        let font = FontAtlas::fallback();
        let red = SignTable::from([(
            (4, 70, 9),
            [
                r#"{"text":"Hi","color":"red"}"#.to_owned(),
                r#""\u00a79a""#.to_owned(),
                String::new(),
                String::new(),
            ],
        )]);
        let batches = draw(&entry(STANDING_SIGN, 0), &red, Some(&font));
        let colours: Vec<u32> = batches[&TextureSlot::SignText]
            .iter()
            .map(|vertex| {
                assert_eq!(vertex.light & CHAT_COLOR_BITS, CHAT_COLOR_BITS);
                (vertex.light >> CHAT_COLOR_SHIFT) & 15
            })
            .collect();
        // "Hi" in red (12), then "a" in blue (9).
        assert_eq!(colours, [[12; 6], [12; 6], [9; 6]].concat());
    }

    #[test]
    fn the_text_lies_in_front_of_the_board_within_the_block() {
        let font = FontAtlas::fallback();
        let long = "W".repeat(40);
        let wide = lines([long.as_str(); 4]);
        let batches = draw(&entry(STANDING_SIGN, 0), &wide, Some(&font));
        for vertex in &batches[&TextureSlot::SignText] {
            let [x, _, z] = vertex.position;
            assert!((4.0..=5.0).contains(&x), "{x}");
            // In front of the board (z 9.5 + 0.047), not behind it.
            assert!(z > 9.5, "{z}");
        }
    }

    #[test]
    fn the_rotation_follows_the_kind() {
        assert_eq!(rotation_degrees(true, 4), 90.0);
        assert_eq!(rotation_degrees(false, 4), 90.0);
        assert_eq!(rotation_degrees(true, 2), 45.0);
        assert_eq!(rotation_degrees(false, 2), 180.0);
    }
}

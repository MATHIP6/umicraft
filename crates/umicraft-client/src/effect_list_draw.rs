// SPDX-License-Identifier: MIT OR Apache-2.0

//! Drawing of the potion effect list beside the survival and creative
//! inventories (MCP `InventoryEffectRenderer.drawActivePotionEffects`).
//!
//! The geometry is [`umicraft_ui::screens::effect_list`]; the lines come from
//! the game thread with the HUD ([`EffectLine`]). Runs on the render thread.

use umicraft_assets::GuiAtlas;
use umicraft_game::inventory::effect_list::EffectLine;
use umicraft_ui::screens::effect_list::{effect_boxes, DURATION_COLOR, NAME_COLOR};
use umicraft_ui::screens::SpriteRect;
use umicraft_ui::{TextStyle, Ui};

use umicraft_core::lang::Lang;

/// The texture of the boxes and the status icons.
const INVENTORY_TEXTURE: &str = "container/inventory";
const WHITE: [f32; 4] = [1.0; 4];

/// Draws `effects` beside a window whose top-left corner (`guiLeft`,
/// `guiTop`) is `window` in physical pixels, at GUI `scale`. Without the GUI
/// textures only the text is drawn.
pub(crate) fn draw_effect_list(
    ui: &mut Ui,
    (window, scale): ((f32, f32), u32),
    gui_atlas: Option<&GuiAtlas>,
    effects: &[EffectLine],
) {
    if effects.is_empty() {
        return;
    }
    let sheet = gui_atlas.and_then(|atlas| atlas.region(INVENTORY_TEXTURE));
    let icons: Vec<Option<u8>> = effects.iter().map(|line| line.icon).collect();
    let lang = crate::language::current();
    let s = scale as f32;
    for (line, entry) in effects.iter().zip(effect_boxes(window, scale, &icons)) {
        if let Some(sheet) = sheet {
            for sprite in std::iter::once(entry.background).chain(entry.icon) {
                draw_sprite(ui, sheet, &sprite);
            }
        }
        draw_shadowed(ui, entry.name_at, name_text(line, &lang), (NAME_COLOR, s));
        draw_shadowed(
            ui,
            entry.duration_at,
            line.duration.clone(),
            (DURATION_COLOR, s),
        );
    }
}

/// The name of the effect with its level, as `I18n.format` gives them.
fn name_text(line: &EffectLine, lang: &Lang) -> String {
    let name = lang.tr(line.name_key);
    match line.level_key {
        Some(level) => format!("{name} {}", lang.tr(level)),
        None => name.to_owned(),
    }
}

fn draw_sprite(ui: &mut Ui, sheet: &umicraft_assets::GuiRegion, sprite: &SpriteRect) {
    let (x, y, w, h) = sprite.src;
    ui.draw()
        .image(sheet.sub_region(x, y, w, h), sprite.dst, WHITE);
}

/// `FontRenderer.drawStringWithShadow`: the text over a copy one GUI pixel
/// down and right, at a quarter of the colour.
fn draw_shadowed(ui: &mut Ui, [x, y]: [f32; 2], text: String, (rgb, scale): (u32, f32)) {
    let channel = |shift: u32| ((rgb >> shift) & 0xFF) as f32 / 255.0;
    let color = [channel(16), channel(8), channel(0), 1.0];
    let shadow = [color[0] * 0.25, color[1] * 0.25, color[2] * 0.25, 1.0];
    ui.draw().text(
        [x + scale, y + scale],
        text.clone(),
        TextStyle::new(scale, shadow),
    );
    ui.draw().text([x, y], text, TextStyle::new(scale, color));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(level_key: Option<&'static str>) -> EffectLine {
        EffectLine {
            name_key: "potion.moveSpeed",
            level_key,
            icon: Some(0),
            duration: "1:30".to_owned(),
        }
    }

    #[test]
    fn the_name_carries_the_translated_level() {
        let lang = Lang::from_texts(
            "xx_XX",
            None,
            Some("potion.moveSpeed=Speed\nenchantment.level.2=II\n"),
        );
        assert_eq!(name_text(&line(None), &lang), "Speed");
        assert_eq!(
            name_text(&line(Some("enchantment.level.2")), &lang),
            "Speed II"
        );
    }
}

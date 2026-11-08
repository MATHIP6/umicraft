// SPDX-License-Identifier: MIT OR Apache-2.0

//! The bridge between the built-in mods registry and the mods menu (ADR-0065).
//!
//! `umicraft-ui` draws plain view types and answers with a [`ModsOutcome`]; this
//! module builds the views from the [`ModRegistry`] (translating every text with
//! the language keys the mods declare) and applies the outcomes back to it. The
//! registry sanitizes every value, so what the menu sends is never trusted.

use crate::hud_editor_view;
use umicraft_builtin_mods::{
    ModCategory, ModRegistry, ModView, RegistryError, SettingDef, SettingKind, SettingValue,
};
use umicraft_core::lang::Lang;
use umicraft_ui::menus::{
    ModCardView, ModValue, ModsOutcome, ModsView, SettingControl, SettingView,
};

/// Builds what the menu lists: the categories, then the mods of each category in
/// registration order.
#[must_use]
pub fn build(registry: &ModRegistry, lang: &Lang) -> ModsView {
    let categories = ModCategory::ALL
        .iter()
        .map(|category| lang.tr(category.lang_key()).to_owned())
        .collect();
    let cards = ModCategory::ALL
        .iter()
        .enumerate()
        .flat_map(|(index, category)| {
            registry
                .mods()
                .filter(move |view| view.meta.category == *category)
                .map(move |view| (index, view))
        })
        .map(|(category, view)| card(&view, category, lang))
        .collect();
    ModsView {
        categories,
        cards,
        ..ModsView::default()
    }
}

fn card(view: &ModView<'_>, category: usize, lang: &Lang) -> ModCardView {
    let settings = view
        .settings
        .schema()
        .iter()
        .map(|def| SettingView {
            key: def.key.to_owned(),
            label: lang.tr(def.label_key).to_owned(),
            control: control(def, view, lang),
        })
        .collect();
    ModCardView {
        id: view.meta.id.as_str().to_owned(),
        name: lang.tr(view.meta.name_key).to_owned(),
        description: lang.tr(view.meta.description_key).to_owned(),
        category,
        enabled: view.enabled,
        settings,
    }
}

/// The control of one setting with the value the mod currently has. The option
/// labels of a choice are the language keys `<label_key>.<option>`.
fn control(def: &SettingDef, view: &ModView<'_>, lang: &Lang) -> SettingControl {
    let settings = view.settings;
    match def.kind {
        SettingKind::Toggle { .. } => SettingControl::Toggle(settings.bool(def.key)),
        SettingKind::Slider { min, max, step, .. } => SettingControl::Slider {
            min,
            max,
            step,
            value: settings.number(def.key),
        },
        SettingKind::Choice { options, .. } => SettingControl::Choice {
            options: options
                .iter()
                .map(|option| {
                    let key = format!("{}.{option}", def.label_key);
                    ((*option).to_owned(), lang.tr(&key).to_owned())
                })
                .collect(),
            selected: options
                .iter()
                .position(|option| *option == settings.text(def.key))
                .unwrap_or(0),
        },
        SettingKind::Color { .. } => SettingControl::Color(settings.text(def.key).to_owned()),
    }
}

/// Applies what the player did on the menu to the registry. Returns whether the
/// saved state may have changed, so the caller writes it to disk.
///
/// # Errors
///
/// [`RegistryError`] if the outcome names a mod or a setting that does not exist.
pub fn apply(registry: &mut ModRegistry, outcome: &ModsOutcome) -> Result<bool, RegistryError> {
    match outcome {
        ModsOutcome::Toggle { id, enabled } => registry.set_enabled(id, *enabled)?,
        ModsOutcome::Set { id, key, value } => {
            registry.set_setting(id, key, &setting_value(value))?;
        }
        ModsOutcome::Hud(change) => return hud_editor_view::apply(registry, change),
        ModsOutcome::Close => return Ok(false),
    }
    Ok(true)
}

fn setting_value(value: &ModValue) -> SettingValue {
    match value {
        ModValue::Bool(on) => SettingValue::Bool(*on),
        ModValue::Number(number) => SettingValue::Number(*number),
        ModValue::Text(text) => SettingValue::Text(text.clone()),
    }
}

#[cfg(test)]
mod tests;

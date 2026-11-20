// SPDX-License-Identifier: MIT OR Apache-2.0

//! The external mods in the mods menu (F3.4, ADR-0086).
//!
//! The menu is generic over card ids, so an external mod is listed next to the
//! built-in ones under an id prefixed `ext:` (`ext:<mod>` for its card,
//! `ext:<mod>:<widget>` for a HUD widget). [`route`] is the one place that decides
//! whether an outcome of the menu belongs to the built-in registry or to the
//! [`ModHost`]; [`apply`] carries the external ones out. Texts of external mods are
//! literal (they ship no language file), apart from the few hints the client adds.

use thiserror::Error;
use umicraft_builtin_mods::{HudPlacement, SettingValue};
use umicraft_core::lang::Lang;
use umicraft_modhost::{
    ConfigureError, EnableError, IdError, ModHost, ModId, ModView, SettingSpecKind,
    SettingView as ExternalSetting,
};
use umicraft_ui::menus::{
    HudEditorOutcome, ModCardView, ModsOutcome, ModsView, SettingControl, SettingView,
};

use crate::mods::hud_editor::placement_of;
use crate::mods::menu::setting_value;

/// What an id of the menu starts with when it names an external mod.
const PREFIX: &str = "ext:";

/// The id of the card of the server-compatible mode. A mod id never contains a
/// hyphen, so no mod's card can take it.
const COMPAT_CARD: &str = "ext:server-compat";

/// The id of the card of mod `mod_id`.
pub(crate) fn card_id(mod_id: &ModId) -> String {
    format!("{PREFIX}{mod_id}")
}

/// The id of the HUD widget `widget` of mod `mod_id`.
pub(crate) fn widget_id(mod_id: &ModId, widget: &str) -> String {
    format!("{PREFIX}{mod_id}:{widget}")
}

/// Splits `ext:<mod>[:<widget>]`; `None` for an id of a built-in mod. A mod id never
/// contains a colon, so the split is unambiguous.
fn parse(id: &str) -> Option<(&str, Option<&str>)> {
    let rest = id.strip_prefix(PREFIX)?;
    Some(match rest.split_once(':') {
        Some((mod_id, widget)) => (mod_id, Some(widget)),
        None => (rest, None),
    })
}

/// What an outcome of the menu asks of an external mod.
#[derive(Debug, PartialEq)]
pub(crate) enum Request<'a> {
    /// The switch of the card.
    Enable { mod_id: &'a str, enabled: bool },
    /// A setting control.
    Setting {
        mod_id: &'a str,
        key: &'a str,
        value: SettingValue,
    },
    /// A widget dropped or resized in the HUD editor.
    Place {
        mod_id: &'a str,
        widget: &'a str,
        placement: HudPlacement,
    },
    /// One widget back to the place its mod suggests.
    ResetWidget { mod_id: &'a str, widget: &'a str },
    /// Every widget back to the place its mod suggests.
    ResetAll,
    /// The switch of the server-compatible mode (F3.5).
    ServerCompat { enabled: bool },
}

/// Who an outcome of the menu is for.
#[derive(Debug, PartialEq)]
pub(crate) struct Route<'a> {
    /// Whether the built-in registry handles it.
    pub(crate) builtin: bool,
    /// What the external mods are asked to do, if anything.
    pub(crate) external: Option<Request<'a>>,
}

impl<'a> Route<'a> {
    const BUILTIN: Self = Self {
        builtin: true,
        external: None,
    };

    fn external(request: Option<Request<'a>>) -> Self {
        Self {
            builtin: false,
            external: request,
        }
    }
}

/// Decides who handles `change`: the built-in registry for every id without the
/// `ext:` prefix, the external mods for the others. "Reset all" is for both. An
/// external widget cannot be locked and has no settings page behind its gear, so
/// those outcomes are dropped instead of reaching a registry that would refuse them.
pub(crate) fn route(change: &ModsOutcome) -> Route<'_> {
    use HudEditorOutcome::{Lock, OpenSettings, Placed, Reset, ResetAll, Scaled};
    match change {
        ModsOutcome::Toggle { id, enabled } if id == COMPAT_CARD => {
            Some(Route::external(Some(Request::ServerCompat {
                enabled: *enabled,
            })))
        }
        ModsOutcome::Toggle { id, enabled } => parse(id).map(|(mod_id, _)| {
            Route::external(Some(Request::Enable {
                mod_id,
                enabled: *enabled,
            }))
        }),
        ModsOutcome::Set { id, key, value } => parse(id).map(|(mod_id, _)| {
            Route::external(Some(Request::Setting {
                mod_id,
                key,
                value: setting_value(value),
            }))
        }),
        ModsOutcome::Hud(Placed {
            id,
            rect,
            scale,
            screen,
        })
        | ModsOutcome::Hud(Scaled {
            id,
            rect,
            scale,
            screen,
        }) => parse(id).map(|(mod_id, widget)| {
            Route::external(widget.map(|widget| Request::Place {
                mod_id,
                widget,
                placement: placement_of(*rect, *scale, *screen),
            }))
        }),
        ModsOutcome::Hud(Reset { id }) => parse(id).map(|(mod_id, widget)| {
            Route::external(widget.map(|widget| Request::ResetWidget { mod_id, widget }))
        }),
        ModsOutcome::Hud(Lock { id, .. } | OpenSettings { id }) => {
            parse(id).map(|_| Route::external(None))
        }
        ModsOutcome::Hud(ResetAll) => Some(Route {
            builtin: true,
            external: Some(Request::ResetAll),
        }),
        _ => None,
    }
    .unwrap_or(Route::BUILTIN)
}

/// Why a request of the menu could not be carried out.
#[derive(Debug, Error)]
pub(crate) enum ChangeError {
    /// The id after `ext:` is not a valid mod id.
    #[error(transparent)]
    Id(#[from] IdError),
    /// The mod is unknown, or failed and so refuses its switch.
    #[error(transparent)]
    Enable(#[from] EnableError),
    /// The mod is unknown, or declares no such setting or widget.
    #[error(transparent)]
    Configure(#[from] ConfigureError),
}

/// Carries out `request` and returns the mods whose saved choices changed, so the
/// caller writes their files.
///
/// # Errors
///
/// [`ChangeError`] for an unknown mod, setting or widget, or a mod that failed.
pub(crate) fn apply(host: &ModHost, request: Request<'_>) -> Result<Vec<ModId>, ChangeError> {
    match request {
        Request::Enable { mod_id, enabled } => {
            let id: ModId = mod_id.parse()?;
            host.set_enabled(&id, enabled)?;
            Ok(vec![id])
        }
        Request::Setting { mod_id, key, value } => {
            let id: ModId = mod_id.parse()?;
            host.set_setting(&id, key, &value)?;
            Ok(vec![id])
        }
        Request::Place {
            mod_id,
            widget,
            placement,
        } => {
            let id: ModId = mod_id.parse()?;
            host.set_placement(&id, widget, placement)?;
            Ok(vec![id])
        }
        Request::ResetWidget { mod_id, widget } => {
            let id: ModId = mod_id.parse()?;
            reset_widget(host, &id, widget)?;
            Ok(vec![id])
        }
        Request::ResetAll => reset_all(host),
        // No mod's file changes: the mode is a client option, saved by the caller.
        Request::ServerCompat { enabled } => {
            host.set_server_compat(enabled);
            Ok(Vec::new())
        }
    }
}

/// Puts widget `widget` of `id` back to its mod's default placement.
fn reset_widget(host: &ModHost, id: &ModId, widget: &str) -> Result<(), ConfigureError> {
    let default = host
        .mods()
        .filter(|view| view.id == id)
        .flat_map(|view| view.widgets())
        .find(|candidate| candidate.key == widget)
        .map(|found| found.default_placement);
    match default {
        Some(placement) => host.set_placement(id, widget, placement),
        None => Err(ConfigureError::UnknownKey {
            kind: "widget",
            key: widget.to_owned(),
        }),
    }
}

/// Puts every widget of every mod back to its default; returns the mods that have one.
fn reset_all(host: &ModHost) -> Result<Vec<ModId>, ChangeError> {
    let mut changed = Vec::new();
    for view in host.mods() {
        let mut has_widget = false;
        for widget in view.widgets() {
            host.set_placement(view.id, widget.key, widget.default_placement)?;
            has_widget = true;
        }
        if has_widget {
            changed.push(view.id.clone());
        }
    }
    Ok(changed)
}

/// Adds the extra category of the external mods and one card per mod to `view`.
/// Nothing is added when no external mod is loaded, so the menu shows no empty tab.
/// The switch of the server-compatible mode comes first when a mod alters packets
/// or movement, the only mods it affects.
pub(crate) fn extend(view: &mut ModsView, host: &ModHost, lang: &Lang) {
    let mut cards: Vec<ModCardView> = host.mods().map(|mod_view| card(&mod_view, lang)).collect();
    if cards.is_empty() {
        return;
    }
    if host.mods().any(|mod_view| mod_view.alters_game) {
        cards.insert(0, compat_card(host, lang));
    }
    let category = view.categories.len();
    view.categories
        .push(lang.tr("umicraft.mods.category.external").to_owned());
    view.cards.extend(
        cards
            .into_iter()
            .map(|card| ModCardView { category, ..card }),
    );
}

/// The card whose switch is the server-compatible mode.
fn compat_card(host: &ModHost, lang: &Lang) -> ModCardView {
    ModCardView {
        id: COMPAT_CARD.to_owned(),
        name: lang.tr("umicraft.mods.external.compat").to_owned(),
        description: lang
            .tr("umicraft.mods.external.compat.description")
            .to_owned(),
        category: 0,
        enabled: host.server_compat(),
        settings: Vec::new(),
    }
}

/// The card of one external mod. A failed mod shows as off with the failure as its
/// description, so the player sees why its switch does nothing; so does a mod held
/// off by the server-compatible mode. A mod that alters packets or movement says so
/// before its description, whatever its state.
fn card(view: &ModView<'_>, lang: &Lang) -> ModCardView {
    let description = if view.failed {
        lang.tr("umicraft.mods.external.failed")
    } else if view.held_by_compat {
        lang.tr("umicraft.mods.external.held")
    } else {
        view.description.unwrap_or_default()
    };
    let description = if view.alters_game {
        format!("{} {description}", lang.tr("umicraft.mods.external.alters"))
    } else {
        description.to_owned()
    };
    ModCardView {
        id: card_id(view.id),
        name: view.name.to_owned(),
        description,
        category: 0,
        enabled: view.enabled && !view.failed && !view.held_by_compat,
        settings: view
            .settings()
            .map(|setting| setting_view(&setting))
            .collect(),
    }
}

fn setting_view(setting: &ExternalSetting<'_>) -> SettingView {
    SettingView {
        key: setting.spec.key.clone(),
        label: setting.spec.label.clone(),
        control: control(&setting.spec.kind, &setting.value),
    }
}

/// The control of a setting with its current value. The options of a choice are
/// their own labels, which are literal text.
fn control(kind: &SettingSpecKind, value: &SettingValue) -> SettingControl {
    let text = || match value {
        SettingValue::Text(text) => text.as_str(),
        _ => "",
    };
    match kind {
        SettingSpecKind::Toggle { .. } => {
            SettingControl::Toggle(matches!(value, SettingValue::Bool(true)))
        }
        SettingSpecKind::Slider { min, max, step, .. } => SettingControl::Slider {
            min: *min,
            max: *max,
            step: *step,
            value: match value {
                SettingValue::Number(number) => *number,
                _ => *min,
            },
        },
        SettingSpecKind::Choice { options, .. } => SettingControl::Choice {
            options: options
                .iter()
                .map(|option| (option.clone(), option.clone()))
                .collect(),
            selected: options
                .iter()
                .position(|option| option == text())
                .unwrap_or(0),
        },
        SettingSpecKind::Color { .. } => SettingControl::Color(text().to_owned()),
    }
}

#[cfg(test)]
mod tests;

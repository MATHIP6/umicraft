// SPDX-License-Identifier: MIT OR Apache-2.0

//! The per-server resource pack setting (MCP `ServerData.ServerResourceMode`).
//!
//! The Add/Edit Server screen cycles it with a button; the client stores it
//! with the server entry. Acting on a server's resource pack packet is P12
//! (`docs/parity.md`); until then the value is only edited and saved.

/// What to do when a server offers a resource pack.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ResourcePackMode {
    /// Ask the player each time (vanilla default).
    #[default]
    Prompt,
    /// Download and apply without asking.
    Enabled,
    /// Never accept.
    Disabled,
}

impl ResourcePackMode {
    /// The next mode in the order of the vanilla button, which cycles the
    /// enum `ENABLED, DISABLED, PROMPT` (`GuiScreenAddServer.actionPerformed`).
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Enabled => Self::Disabled,
            Self::Disabled => Self::Prompt,
            Self::Prompt => Self::Enabled,
        }
    }

    /// The language key of the mode's name (`addServer.resourcePack.<mode>`).
    #[must_use]
    pub fn lang_key(self) -> &'static str {
        match self {
            Self::Prompt => "addServer.resourcePack.prompt",
            Self::Enabled => "addServer.resourcePack.enabled",
            Self::Disabled => "addServer.resourcePack.disabled",
        }
    }

    /// The name stored in `servers.toml`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Prompt => "prompt",
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }

    /// The mode stored under `name`, or `None` for anything else.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Prompt, Self::Enabled, Self::Disabled]
            .into_iter()
            .find(|mode| mode.name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_button_cycles_in_the_vanilla_order() {
        let mut mode = ResourcePackMode::Enabled;
        let order: Vec<_> = (0..4)
            .map(|_| {
                let shown = mode;
                mode = mode.next();
                shown
            })
            .collect();
        assert_eq!(
            order,
            [
                ResourcePackMode::Enabled,
                ResourcePackMode::Disabled,
                ResourcePackMode::Prompt,
                ResourcePackMode::Enabled
            ]
        );
    }

    #[test]
    fn the_default_is_prompt_like_vanilla() {
        assert_eq!(ResourcePackMode::default(), ResourcePackMode::Prompt);
    }

    #[test]
    fn names_round_trip_and_unknown_names_are_refused() {
        for mode in [
            ResourcePackMode::Prompt,
            ResourcePackMode::Enabled,
            ResourcePackMode::Disabled,
        ] {
            assert_eq!(ResourcePackMode::from_name(mode.name()), Some(mode));
        }
        assert_eq!(ResourcePackMode::from_name("maybe"), None);
    }
}

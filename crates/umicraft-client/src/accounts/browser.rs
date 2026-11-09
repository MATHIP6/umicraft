// SPDX-License-Identifier: MIT OR Apache-2.0

//! Opens the Microsoft sign-in page in the player's browser.
//!
//! The address comes from Microsoft's answer, so it is checked before it is
//! handed to the system: only plain `https://` addresses are opened, never a
//! local file, a custom scheme or a string the shell could read as options.

/// Whether `url` is an `https://` address with a host and no space or control
/// character.
#[must_use]
pub fn is_openable(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    !host.is_empty() && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Opens `url` in the default browser. An address that fails the check, or a
/// system without a browser, is logged and ignored: the player can still copy
/// the link.
pub fn open(url: &str) {
    if !is_openable(url) {
        tracing::warn!("refusing to open a sign-in address that is not plain https");
        return;
    }
    if let Err(error) = webbrowser::open(url) {
        tracing::warn!(%error, "could not open the browser");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_https_addresses_are_openable() {
        assert!(is_openable("https://www.microsoft.com/link"));
        assert!(is_openable(
            "https://login.live.com/oauth20_remoteconnect.srf?otc=ABC"
        ));
    }

    #[test]
    fn anything_else_is_refused() {
        for url in [
            "http://www.microsoft.com/link",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https://",
            "https:///path",
            "https://host/with space",
            "https://host/\nnewline",
            "--help",
            "",
        ] {
            assert!(!is_openable(url), "{url:?}");
        }
    }
}

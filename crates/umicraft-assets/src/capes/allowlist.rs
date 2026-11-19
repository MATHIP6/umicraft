// SPDX-License-Identifier: MIT OR Apache-2.0

//! Which URLs the cape worker may fetch.
//!
//! A JSON answer or a redirect is chosen by a third party, so without a
//! check it could send the client to any host (a request to an internal
//! address, for instance). Only the cape services' own domains pass; see
//! `docs/sources.md`, section "Capes".

use reqwest::Url;

/// Domains whose subdomains may serve a cape over HTTPS.
const HTTPS_DOMAINS: [&str; 5] = [
    "labymod.net",
    "laby.net",
    "minecraftcapes.net",
    "cosmetica.cc",
    "optifine.net",
];

/// The only domain that may be fetched over plain HTTP: `s.optifine.net` does
/// not answer on HTTPS.
const HTTP_DOMAIN: &str = "optifine.net";

/// True when `host` is `domain` or a subdomain of it, on a label boundary
/// (`evil-optifine.net` is not `optifine.net`).
fn is_domain_or_subdomain(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

/// True for a URL the cape worker may request or follow.
///
/// `https` on the domains of the cape services, and `http` on `optifine.net`
/// only. A URL with credentials, an IP address as host or a port other than
/// the default is refused.
///
/// # Examples
///
/// ```
/// use umicraft_assets::capes::is_allowed_cape_url;
///
/// assert!(is_allowed_cape_url("http://s.optifine.net/capes/Notch.png"));
/// assert!(is_allowed_cape_url("https://items.laby.net/cape.png"));
/// assert!(!is_allowed_cape_url("http://api.cosmetica.cc/x"));
/// assert!(!is_allowed_cape_url("https://127.0.0.1/x"));
/// ```
#[must_use]
pub fn is_allowed_cape_url(url: &str) -> bool {
    let Ok(url) = Url::parse(url) else {
        return false;
    };
    let plain_text = match url.scheme() {
        "https" => false,
        "http" => true,
        _ => return false,
    };
    // The `url` crate drops a port equal to the scheme default, so any port
    // left is a non-default one.
    if !url.username().is_empty() || url.password().is_some() || url.port().is_some() {
        return false;
    }
    // `domain()` is `None` for an IP literal, which is refused for free.
    let Some(host) = url.domain() else {
        return false;
    };
    if plain_text {
        return is_domain_or_subdomain(host, HTTP_DOMAIN);
    }
    HTTPS_DOMAINS
        .iter()
        .any(|domain| is_domain_or_subdomain(host, domain))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_is_allowed_on_every_service_domain() {
        for url in [
            "https://dl.labymod.net/capes/x",
            "https://items.laby.net/a.png",
            "https://api.minecraftcapes.net/profile/x",
            "https://api.cosmetica.cc/users/x/cape",
            "https://optifine.net/x",
            "https://cosmetica.cc/x",
        ] {
            assert!(is_allowed_cape_url(url), "{url}");
        }
    }

    #[test]
    fn plain_http_is_only_for_optifine() {
        assert!(is_allowed_cape_url("http://s.optifine.net/capes/a.png"));
        assert!(!is_allowed_cape_url("http://dl.labymod.net/capes/x"));
        assert!(!is_allowed_cape_url("http://api.cosmetica.cc/x"));
    }

    #[test]
    fn lookalike_hosts_are_refused() {
        for url in [
            "https://evil-optifine.net/x",
            "https://optifine.net.evil.example/x",
            "https://notlaby.net/x",
            "https://laby.net.cn/x",
            "https://example.com/optifine.net",
        ] {
            assert!(!is_allowed_cape_url(url), "{url}");
        }
    }

    #[test]
    fn credentials_ports_and_ip_literals_are_refused() {
        for url in [
            "https://user@dl.labymod.net/x",
            "https://user:pw@dl.labymod.net/x",
            "https://dl.labymod.net:8443/x",
            "http://s.optifine.net:8080/x",
            "https://127.0.0.1/x",
            "https://[::1]/x",
            "http://1.2.3.4/x",
        ] {
            assert!(!is_allowed_cape_url(url), "{url}");
        }
    }

    #[test]
    fn the_default_port_written_out_is_fine() {
        assert!(is_allowed_cape_url("https://dl.labymod.net:443/x"));
    }

    #[test]
    fn other_schemes_and_garbage_are_refused() {
        for url in ["ftp://s.optifine.net/x", "file:///etc/passwd", "", "nope"] {
            assert!(!is_allowed_cape_url(url), "{url}");
        }
    }

    #[test]
    fn host_case_does_not_matter() {
        assert!(is_allowed_cape_url("https://DL.LabyMod.NET/x"));
    }
}

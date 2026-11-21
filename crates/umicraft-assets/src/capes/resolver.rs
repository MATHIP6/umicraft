// SPDX-License-Identifier: MIT OR Apache-2.0

//! The cape worker: looks players up on the services without ever blocking
//! the game or render thread.
//!
//! Same shape as [`TextureFetcher`](crate::profile_textures::TextureFetcher):
//! a dedicated OS thread with a current-thread tokio runtime, a bounded
//! request queue in, a bounded result queue out, and a capped number of
//! queries in flight. Each query tries its providers in order and stops at
//! the first cape; a provider that errors is logged and skipped.

use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::Duration;

use reqwest::redirect::Policy;
use reqwest::{Client, Url};
use tokio::sync::mpsc::{channel as async_channel, Receiver as AsyncReceiver, Sender};
use tokio::task::JoinSet;

use super::allowlist::is_allowed_cape_url;
use super::error::CapeError;
use super::image::{decode_cape, CapeImage};
use super::provider::{CapeOwner, CapeProvider, ResponseKind};
use super::response::{parse_cosmetica, parse_minecraftcapes, CapeSource, ImageSource, UrlCheck};

/// Capacity of the request and result queues.
const QUEUE_CAPACITY: usize = 256;
/// Queries in flight at the same time.
const MAX_CONCURRENT_QUERIES: usize = 4;
/// Largest PNG accepted, in bytes.
const MAX_IMAGE_BYTES: usize = 1 << 20;
/// Largest JSON answer accepted, in bytes: room for the base64 copy of the
/// largest image accepted, plus the other fields.
///
/// A MinecraftCapes profile looked up by UUID still carries the deprecated
/// `textures.cape` base64 copy next to `cape_url`; for an HD cape (2048x1024)
/// the answer is about 460 KB (observed traffic, `docs/sources.md`, "Capes").
const MAX_JSON_BYTES: usize = MAX_IMAGE_BYTES.div_ceil(3) * 4 + (64 << 10);
/// Redirects followed before giving up.
const MAX_REDIRECTS: usize = 5;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(10);

/// A cape lookup for one player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapeQuery {
    /// The player.
    pub owner: CapeOwner,
    /// The providers to ask, in order; the first cape found wins.
    pub providers: Vec<CapeProvider>,
}

/// A cape and the service it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundCape {
    /// The provider that had the cape.
    pub provider: CapeProvider,
    /// The decoded frames.
    pub image: CapeImage,
}

/// The answer to a [`CapeQuery`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapeResult {
    /// The UUID of the player asked about.
    pub uuid: u128,
    /// The cape, `None` when no provider has one (or the player may not be
    /// looked up).
    pub cape: Option<FoundCape>,
}

/// Where the providers are reached and which URLs they may name.
///
/// Production uses the real services; the tests aim every provider at a
/// local server, which the allowlist would refuse.
#[derive(Debug, Clone)]
pub(crate) struct CapeEndpoints {
    /// Replaces the scheme and host of every provider URL when set.
    origin: Option<String>,
    allow: UrlCheck,
}

impl CapeEndpoints {
    pub(crate) fn official() -> Self {
        Self {
            origin: None,
            allow: is_allowed_cape_url,
        }
    }

    /// Every provider at `origin` (`http://127.0.0.1:port`), any URL allowed.
    #[cfg(test)]
    pub(crate) fn local(origin: &str) -> Self {
        Self {
            origin: Some(origin.to_owned()),
            allow: |_| true,
        }
    }

    /// The URL to fetch for a provider URL: the same, unless an origin is set.
    fn rewrite(&self, url: String) -> String {
        let Some(origin) = &self.origin else {
            return url;
        };
        match Url::parse(&url) {
            Ok(parsed) => format!("{origin}{}", parsed.path()),
            Err(_) => url,
        }
    }
}

/// Whether a redirect to `next`, after `previous` earlier hops, is followed.
///
/// A redirect is the server's choice, so it must stay on an allowed host.
/// `previous` counts the URLs already requested, as `Policy::limited` does.
fn follows_redirect(previous: usize, next: &Url, allow: UrlCheck) -> bool {
    previous <= MAX_REDIRECTS && allow(next.as_str())
}

fn build_client(endpoints: &CapeEndpoints) -> Result<Client, reqwest::Error> {
    let allow = endpoints.allow;
    let builder = Client::builder();
    // The tests talk to a local server, which a proxy from the environment
    // must not intercept.
    let builder = if endpoints.origin.is_some() {
        builder.no_proxy()
    } else {
        builder
    };
    builder
        .user_agent(concat!("Umicraft/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(TOTAL_TIMEOUT)
        .redirect(Policy::custom(move |attempt| {
            if follows_redirect(attempt.previous().len(), attempt.url(), allow) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
}

/// One image or JSON body: bytes, plus whether the cape is animated.
type Fetched = Option<(Vec<u8>, bool)>;

/// Does the network and decoding work of the worker.
#[derive(Clone)]
struct Fetcher {
    client: Client,
    endpoints: CapeEndpoints,
}

impl Fetcher {
    /// Answers a query: the first provider with a cape wins.
    async fn resolve(&self, query: CapeQuery) -> CapeResult {
        let uuid = query.owner.uuid;
        if !query.owner.is_lookup_allowed() {
            return CapeResult { uuid, cape: None };
        }
        for provider in query.providers {
            if provider == CapeProvider::OptiFine && !query.owner.has_valid_name() {
                continue;
            }
            match self.provider_cape(provider, &query.owner).await {
                Ok(Some(image)) => {
                    return CapeResult {
                        uuid,
                        cape: Some(FoundCape { provider, image }),
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::debug!(provider = provider.key(), %error, "cape lookup failed")
                }
            }
        }
        CapeResult { uuid, cape: None }
    }

    /// The cape one provider has for the owner.
    async fn provider_cape(
        &self,
        provider: CapeProvider,
        owner: &CapeOwner,
    ) -> Result<Option<CapeImage>, CapeError> {
        let request = provider.request(owner);
        let url = self.endpoints.rewrite(request.url);
        let fetched = match request.kind {
            ResponseKind::Png => self
                .get(&url, MAX_IMAGE_BYTES)
                .await?
                .map(|png| (png, false)),
            ResponseKind::CosmeticaJson => self.json_cape(&url, parse_cosmetica).await?,
            ResponseKind::MinecraftCapesJson => self.json_cape(&url, parse_minecraftcapes).await?,
        };
        let Some((png, animated)) = fetched else {
            return Ok(None);
        };
        decode_cape(&png, animated, provider)
    }

    /// Fetches a JSON answer, then the image it names (or carries).
    async fn json_cape(
        &self,
        url: &str,
        parse: fn(&[u8], UrlCheck) -> Result<Option<CapeSource>, CapeError>,
    ) -> Result<Fetched, CapeError> {
        let Some(json) = self.get(url, MAX_JSON_BYTES).await? else {
            return Ok(None);
        };
        let Some(source) = parse(&json, self.endpoints.allow)? else {
            return Ok(None);
        };
        let animated = source.animated;
        let png = match source.image {
            ImageSource::Png(png) => Some(png),
            ImageSource::Url(url) => self.get(&url, MAX_IMAGE_BYTES).await?,
        };
        Ok(png.map(|png| (png, animated)))
    }

    /// Downloads `url`; `None` for any non-2xx status (a 404 is the normal
    /// "no cape").
    async fn get(&self, url: &str, limit: usize) -> Result<Option<Vec<u8>>, CapeError> {
        let http_error = |error: reqwest::Error| CapeError::Http(error.to_string());
        let mut response = self.client.get(url).send().await.map_err(http_error)?;
        if !response.status().is_success() {
            return Ok(None);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(http_error)? {
            if body.len() + chunk.len() > limit {
                return Err(CapeError::TooLarge { limit });
            }
            body.extend_from_slice(&chunk);
        }
        Ok(Some(body))
    }
}

/// Looks capes up on a worker thread. Ask with [`request`](Self::request),
/// collect the answers with [`poll`](Self::poll).
#[derive(Debug)]
pub struct CapeResolver {
    requests: Sender<CapeQuery>,
    results: Receiver<CapeResult>,
}

impl CapeResolver {
    /// Starts the worker thread `cape-fetch`, talking to the real services.
    #[must_use]
    pub fn spawn() -> Self {
        Self::spawn_with(CapeEndpoints::official())
    }

    pub(crate) fn spawn_with(endpoints: CapeEndpoints) -> Self {
        let (requests, queue) = async_channel(QUEUE_CAPACITY);
        let (sender, results) = mpsc::sync_channel(QUEUE_CAPACITY);
        thread::Builder::new()
            .name("cape-fetch".to_owned())
            .spawn(move || worker(queue, &sender, endpoints))
            .inspect_err(|error| tracing::warn!(%error, "cape worker did not start"))
            .ok();
        Self { requests, results }
    }

    /// Queues a lookup. `false` when the queue is full or the worker is gone;
    /// the caller asks again later.
    pub fn request(&self, query: CapeQuery) -> bool {
        self.requests.try_send(query).is_ok()
    }

    /// The answers that arrived since the last call. Never blocks.
    pub fn poll(&self) -> impl Iterator<Item = CapeResult> + '_ {
        self.results.try_iter()
    }
}

fn worker(
    queue: AsyncReceiver<CapeQuery>,
    results: &SyncSender<CapeResult>,
    endpoints: CapeEndpoints,
) {
    let client = match build_client(&endpoints) {
        Ok(client) => client,
        Err(error) => {
            tracing::warn!(%error, "cape worker has no HTTP client");
            return;
        }
    };
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return;
    };
    let fetcher = Fetcher { client, endpoints };
    runtime.block_on(drive(queue, results, fetcher));
}

/// Takes queries from the queue while fewer than [`MAX_CONCURRENT_QUERIES`]
/// run, and forwards their results, until the resolver is dropped and
/// everything has finished. Queries left in the bounded queue wait there, so
/// the worker never holds an unbounded backlog.
async fn drive(
    mut queue: AsyncReceiver<CapeQuery>,
    results: &SyncSender<CapeResult>,
    fetcher: Fetcher,
) {
    let mut in_flight: JoinSet<CapeResult> = JoinSet::new();
    let mut open = true;
    loop {
        if !open && in_flight.is_empty() {
            return;
        }
        let has_room = in_flight.len() < MAX_CONCURRENT_QUERIES;
        tokio::select! {
            query = queue.recv(), if open && has_room => match query {
                Some(query) => {
                    let fetcher = fetcher.clone();
                    in_flight.spawn(async move { fetcher.resolve(query).await });
                }
                None => open = false,
            },
            Some(joined) = in_flight.join_next(), if !in_flight.is_empty() => {
                if let Ok(result) = joined {
                    if results.send(result).is_err() {
                        return;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;

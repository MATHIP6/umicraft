// SPDX-License-Identifier: MIT OR Apache-2.0
//! [`HttpTransport`] over `reqwest` with rustls.

use std::time::Duration;

use reqwest::header::{CONTENT_TYPE, USER_AGENT};

use crate::{
    error::AuthError,
    transport::{Body, HttpTransport, Request, Response},
};

const AGENT: &str = "Umicraft";
const TIMEOUT: Duration = Duration::from_secs(15);

/// The production [`HttpTransport`].
#[derive(Debug, Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    /// Builds a client with the Umicraft user agent and a 15 s timeout.
    ///
    /// # Errors
    /// [`AuthError::Transport`] when the TLS backend cannot start.
    pub fn new() -> Result<Self, AuthError> {
        let client = reqwest::Client::builder()
            .user_agent(AGENT)
            .timeout(TIMEOUT)
            .build()
            .map_err(transport_error)?;
        Ok(Self { client })
    }
}

/// Drops the URL from the message: it is not secret today, but a query string
/// could become one, and the text ends up in the UI.
fn transport_error(error: reqwest::Error) -> AuthError {
    AuthError::Transport(error.without_url().to_string())
}

impl HttpTransport for ReqwestTransport {
    async fn send(&self, request: Request) -> Result<Response, AuthError> {
        let mut builder = match request.method {
            "GET" => self.client.get(&request.url),
            _ => self.client.post(&request.url),
        }
        .header(USER_AGENT, AGENT);
        if let Some(token) = &request.bearer {
            builder = builder.bearer_auth(token.expose());
        }
        builder = match request.body {
            Body::None => builder,
            Body::Form(pairs) => builder.form(&pairs),
            Body::Json(text) => builder.header(CONTENT_TYPE, "application/json").body(text),
        };
        let response = builder.send().await.map_err(transport_error)?;
        let status = response.status().as_u16();
        let body = response.text().await.map_err(transport_error)?;
        Ok(Response { status, body })
    }
}

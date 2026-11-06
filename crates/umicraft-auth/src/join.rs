// SPDX-License-Identifier: MIT OR Apache-2.0
//! Session-server `join` call of the online-mode handshake.

use std::future::Future;

use reqwest::header::CONTENT_TYPE;
use serde::Serialize;

use crate::{error::AuthError, session::Session};

/// Default Mojang session server.
const DEFAULT_BASE_URL: &str = "https://sessionserver.mojang.com";
const USER_AGENT: &str = "Umicraft";

/// Tells the session server that this player is joining a server, so the
/// server's later `hasJoined` check succeeds.
pub trait SessionServer {
    /// Registers the join for `server_hash` (the signed-hex SHA-1 digest of
    /// the handshake secrets).
    ///
    /// # Errors
    /// [`AuthError::JoinRejected`] on a non-204 answer,
    /// [`AuthError::Transport`] when the request fails.
    fn join(
        &self,
        session: &Session,
        server_hash: &str,
    ) -> impl Future<Output = Result<(), AuthError>> + Send;
}

/// JSON body of `POST /session/minecraft/join`, as sent by vanilla
/// (`YggdrasilMinecraftSessionService.joinServer`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JoinRequest<'a> {
    access_token: &'a str,
    selected_profile: &'a str,
    server_id: &'a str,
}

/// [`SessionServer`] backed by HTTPS via `reqwest`.
#[derive(Debug, Clone)]
pub struct HttpSessionServer {
    client: reqwest::Client,
    base_url: String,
}

impl Default for HttpSessionServer {
    fn default() -> Self {
        Self::with_base_url(DEFAULT_BASE_URL)
    }
}

impl HttpSessionServer {
    /// Uses the official Mojang session server.
    pub fn new() -> Self {
        Self::default()
    }

    /// Uses another base URL (no trailing slash), e.g. a local test server.
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
        }
    }
}

impl SessionServer for HttpSessionServer {
    async fn join(&self, session: &Session, server_hash: &str) -> Result<(), AuthError> {
        let body = serde_json::to_vec(&JoinRequest {
            access_token: session.access_token.expose(),
            selected_profile: &session.uuid,
            server_id: server_hash,
        })
        .map_err(|e| AuthError::Transport(e.to_string()))?;
        let url = format!("{}/session/minecraft/join", self.base_url);
        // reqwest errors can embed the URL but never the body, so the token
        // stays out of this message.
        let response = self
            .client
            .post(url)
            .header(CONTENT_TYPE, "application/json")
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .body(body)
            .send()
            .await
            .map_err(|e| AuthError::Transport(e.without_url().to_string()))?;
        let status = response.status();
        tracing::debug!(status = status.as_u16(), user = %session.username, "session join");
        if status.as_u16() == 204 {
            Ok(())
        } else {
            Err(AuthError::JoinRejected {
                status: status.as_u16(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::Secret;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    const TOKEN: &str = "tok-123-secret";

    fn session() -> Session {
        Session::new(
            "Notch",
            "069a79f444e94726a5befca90e38aaf5",
            Secret::new(TOKEN),
        )
        .unwrap()
    }

    /// Serves one request with `status_line`, returning the raw request text.
    async fn serve_once(status_line: &'static str) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                let n = sock.read(&mut chunk).await.unwrap();
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf).into_owned();
                if let Some((head, body)) = text.split_once("\r\n\r\n") {
                    let len = head
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if body.len() >= len || n == 0 {
                        break;
                    }
                } else if n == 0 {
                    break;
                }
            }
            let reply =
                format!("HTTP/1.1 {status_line}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
            sock.write_all(reply.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&buf).into_owned()
        });
        (url, handle)
    }

    #[tokio::test]
    async fn join_success_sends_expected_body() {
        let (url, handle) = serve_once("204 No Content").await;
        HttpSessionServer::with_base_url(url)
            .join(&session(), "abc123")
            .await
            .unwrap();
        let request = handle.await.unwrap();
        assert!(request.starts_with("POST /session/minecraft/join "));
        let body = request.split_once("\r\n\r\n").unwrap().1;
        let json: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(json["accessToken"], TOKEN);
        assert_eq!(json["selectedProfile"], "069a79f444e94726a5befca90e38aaf5");
        assert_eq!(json["serverId"], "abc123");
    }

    #[tokio::test]
    async fn join_rejected_on_403_without_leaking_token() {
        let (url, _handle) = serve_once("403 Forbidden").await;
        let err = HttpSessionServer::with_base_url(url)
            .join(&session(), "h")
            .await
            .unwrap_err();
        assert!(matches!(err, AuthError::JoinRejected { status: 403 }));
        assert!(!err.to_string().contains(TOKEN));
        assert!(!format!("{err:?}").contains(TOKEN));
    }

    #[tokio::test]
    async fn transport_error_does_not_leak_token() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let err = HttpSessionServer::with_base_url(url)
            .join(&session(), "h")
            .await
            .unwrap_err();
        assert!(matches!(err, AuthError::Transport(_)));
        assert!(!err.to_string().contains(TOKEN));
    }
}

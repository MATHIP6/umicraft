// SPDX-License-Identifier: MIT OR Apache-2.0

//! A minimal HTTP/1.1 responder used by the asset transport tests.
//!
//! The test suite must never touch the network (spec §11), so the tests spin
//! up a tiny server on the loopback interface that answers with canned
//! bodies. It only implements what the transport layer needs: `GET` with a
//! path, a `Content-Length`, and a connection close afterwards.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// What the fake server answers for one path.
#[derive(Debug, Clone)]
pub enum Route {
    /// A complete response body.
    Body(Vec<u8>),
    /// A response that announces `declared_len` bytes but sends fewer.
    Truncated {
        /// The bytes that are actually sent.
        body: Vec<u8>,
        /// The length announced in the `Content-Length` header.
        declared_len: usize,
    },
    /// `404 Not Found`.
    NotFound,
}

/// A running fake HTTP server.
#[derive(Debug)]
pub struct FakeServer {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
}

impl FakeServer {
    /// The base URL of the server, e.g. `http://127.0.0.1:34567`.
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Every path that was requested so far.
    pub fn requests(&self) -> Vec<String> {
        self.requests
            .lock()
            .map(|requests| requests.clone())
            .unwrap_or_default()
    }

    /// How often `path` was requested.
    pub fn request_count(&self, path: &str) -> usize {
        self.requests()
            .iter()
            .filter(|request| request.as_str() == path)
            .count()
    }
}

/// Starts a fake server; `build` receives the base URL and returns the
/// routes.
pub async fn start<F>(build: F) -> FakeServer
where
    F: FnOnce(&str) -> HashMap<String, Route>,
{
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("the fake server binds");
    let addr = listener
        .local_addr()
        .expect("the fake server has an address");
    let base = format!("http://{addr}");

    let routes = Arc::new(build(&base));
    let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let requests_for_task = Arc::clone(&requests);

    tokio::spawn(async move {
        loop {
            let Ok((stream, _peer)) = listener.accept().await else {
                break;
            };
            let routes = Arc::clone(&routes);
            let requests = Arc::clone(&requests_for_task);
            tokio::spawn(async move {
                serve_connection(stream, routes, requests).await;
            });
        }
    });

    FakeServer { addr, requests }
}

/// Answers one request.
async fn serve_connection(
    mut stream: TcpStream,
    routes: Arc<HashMap<String, Route>>,
    requests: Arc<Mutex<Vec<String>>>,
) {
    let Some(path) = read_request_path(&mut stream).await else {
        return;
    };
    if let Ok(mut seen) = requests.lock() {
        seen.push(path.clone());
    }

    let route = routes.get(&path).cloned().unwrap_or(Route::NotFound);
    match route {
        Route::Body(body) => {
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(header.as_bytes()).await;
            let _ = stream.write_all(&body).await;
        }
        Route::Truncated { body, declared_len } => {
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {declared_len}\r\nConnection: close\r\n\r\n"
            );
            let _ = stream.write_all(header.as_bytes()).await;
            let _ = stream.write_all(&body).await;
        }
        Route::NotFound => {
            let header = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(header.as_bytes()).await;
        }
    }
    let _ = stream.flush().await;
    let _ = stream.shutdown().await;
}

/// Reads the request head and returns the requested path.
async fn read_request_path(stream: &mut TcpStream) -> Option<String> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if buffer.len() > 64 * 1024 {
            return None;
        }
    }
    let request = String::from_utf8_lossy(&buffer);
    let path = request.split_whitespace().nth(1)?;
    Some(path.to_string())
}

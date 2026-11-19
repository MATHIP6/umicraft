// SPDX-License-Identifier: MIT OR Apache-2.0

//! A tiny local HTTP server for the resolver tests, so they run against a
//! real socket without touching the network (spec §11).

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

/// What the test server answers for one path.
pub(crate) struct Reply {
    status: u16,
    body: Vec<u8>,
}

impl Reply {
    /// `200` with `body`.
    pub(crate) fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            body: body.into(),
        }
    }

    /// An empty answer with `status`.
    pub(crate) fn status(status: u16) -> Self {
        Self {
            status,
            body: Vec::new(),
        }
    }
}

/// A local HTTP/1.1 server on a free port, answering fixed paths with
/// `Connection: close`. It lives until the test process ends.
pub(crate) struct TestServer {
    /// `http://127.0.0.1:<port>`.
    pub(crate) origin: String,
    hits: Arc<Mutex<Vec<String>>>,
}

impl TestServer {
    /// Starts the server; `routes` gets the origin so a JSON body can point
    /// back at the server, and returns the `(path, reply)` pairs to serve.
    /// Any other path is a `404`.
    pub(crate) fn start(routes: impl FnOnce(&str) -> Vec<(String, Reply)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let origin = format!("http://{}", listener.local_addr().expect("addr"));
        let routes = Arc::new(routes(&origin));
        let hits = Arc::new(Mutex::new(Vec::new()));
        let served = Arc::clone(&hits);
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (routes, hits) = (Arc::clone(&routes), Arc::clone(&served));
                thread::spawn(move || answer(stream, &routes, &hits));
            }
        });
        Self { origin, hits }
    }

    /// The paths requested so far, in arrival order.
    pub(crate) fn hits(&self) -> Vec<String> {
        self.hits.lock().expect("hits").clone()
    }
}

fn answer(mut stream: TcpStream, routes: &[(String, Reply)], hits: &Mutex<Vec<String>>) {
    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let mut header = String::new();
    while reader.read_line(&mut header).is_ok_and(|n| n > 2) {
        header.clear();
    }
    let path = request_line.split(' ').nth(1).unwrap_or("").to_owned();
    hits.lock().expect("hits").push(path.clone());
    let not_found = Reply::status(404);
    let reply = routes
        .iter()
        .find(|(route, _)| *route == path)
        .map_or(&not_found, |(_, reply)| reply);
    let head = format!(
        "HTTP/1.1 {} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        reply.status,
        reply.body.len()
    );
    stream.write_all(head.as_bytes()).ok();
    stream.write_all(&reply.body).ok();
}

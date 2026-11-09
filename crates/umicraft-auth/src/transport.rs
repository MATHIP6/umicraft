// SPDX-License-Identifier: MIT OR Apache-2.0
//! The HTTP seam of the Microsoft flow, so each step is tested with canned answers.

use std::future::Future;

use crate::{error::AuthError, secret::Secret};

/// A request body. No `Debug`: forms and answers carry tokens.
#[derive(Clone, PartialEq, Eq)]
pub enum Body {
    /// No body.
    None,
    /// `application/x-www-form-urlencoded` pairs.
    Form(Vec<(String, String)>),
    /// `application/json`, already serialised.
    Json(String),
}

/// One HTTP request. Bearer tokens go in `bearer`, never in the URL.
#[derive(Clone, PartialEq, Eq)]
pub struct Request {
    /// `GET` or `POST`.
    pub method: &'static str,
    /// Absolute URL.
    pub url: String,
    /// Optional `Authorization: Bearer` token; a [`Secret`] so that logging a
    /// request cannot leak it.
    pub bearer: Option<Secret>,
    /// The body.
    pub body: Body,
}

/// A response: status code and body text. Error statuses are returned, not
/// raised, because the Microsoft flow reads their JSON (`authorization_pending`,
/// XSTS `XErr`).
#[derive(Clone, PartialEq, Eq)]
pub struct Response {
    /// HTTP status code.
    pub status: u16,
    /// Body as text.
    pub body: String,
}

/// Sends requests.
pub trait HttpTransport: Send + Sync {
    /// Performs `request`.
    ///
    /// # Errors
    /// [`AuthError::Transport`] when no response was received.
    fn send(&self, request: Request) -> impl Future<Output = Result<Response, AuthError>> + Send;
}

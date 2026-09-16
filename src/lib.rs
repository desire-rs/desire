//! # desire
//!
//! A minimal and ergonomic Rust web framework on hyper.
//!
//! ```no_run
//! use desire::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> desire::Result<()> {
//!     let app = App::new()
//!         .route("/hello/{name}", get(hello));
//!     app.run("0.0.0.0:3000").await
//! }
//!
//! async fn hello(ctx: Context) -> Result<Resp<String>> {
//!     let name = ctx.param_raw("name")?;
//!     Ok(Resp::ok(format!("hello, {name}!")))
//! }
//! ```
//!
//! Core concepts (that is all of them):
//! - [`App`] — routes, state, middleware, and the server entry point
//! - [`Context`] — everything a handler needs about the request
//! - [`Handler`] — what a route maps to (`async fn(Context) -> impl IntoResponse`)
//! - [`Middleware`] / [`Next`] — the onion around handlers
//! - [`Router`] / [`MethodRouter`] — composable route trees
//! - [`Resp`] — the unified `{code, msg, data}` response envelope
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod app;
pub mod body;
pub mod context;
pub mod error;
pub mod form;
pub mod fs;
pub mod handler;
pub mod into_response;
pub mod middleware;
#[cfg(feature = "openapi")]
pub mod openapi;
pub mod resp;
pub mod response;
pub mod router;
pub mod server;
pub mod sse;
pub mod state;
#[cfg(feature = "tls")]
pub mod tls;
pub mod types;
#[cfg(feature = "ws")]
pub mod ws;

/// In-memory test client: `TestClient::new(app).get("/x").send()`.
pub mod test;

pub use app::App;
pub use body::Body;
pub use bytes::Bytes;
pub use context::Context;
pub use cookie;
pub use error::Error;
pub use form::{FormData, UploadedFile};
pub use fs::{ServeDir, ServeFile};
pub use handler::{Handler, WithContext};
pub use into_response::{Html, IntoResponse, Json};
pub use middleware::{Middleware, Next};
pub use resp::{Page, Resp};
pub use response::Response;
pub use router::{MethodRouter, Router};
pub use server::Server;
pub use types::{BoxError, BoxFuture, Result};

#[cfg(feature = "tls")]
pub use tls::TlsConfig;
#[cfg(feature = "ws")]
pub use ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};

/// Re-exported HTTP primitives (`http::Method`, `http::StatusCode`, ...).
pub use hyper::http;

/// The batteries-included import: everything a handler needs.
pub mod prelude {
  pub use crate::Bytes;
  #[cfg(feature = "tls")]
  pub use crate::TlsConfig;
  pub use crate::body;
  pub use crate::body::Body;
  pub use crate::context::Context;
  pub use crate::error::Error;
  pub use crate::fs::{ServeDir, ServeFile};
  pub use crate::handler::Handler;
  pub use crate::into_response::{Html, IntoResponse, Json};
  pub use crate::middleware::{CorsConfig, Middleware, Next, body_limit, cors, logger, timeout};
  pub use crate::resp::{Page, Resp};
  pub use crate::response::Response;
  pub use crate::router::{
    MethodRouter, Router, connect, delete, get, head, options, patch, post, put, trace,
  };
  pub use crate::sse::{Event, Sse};
  pub use crate::test::TestClient;
  pub use crate::{App, Result, Server};
  #[cfg(feature = "ws")]
  pub use crate::{WebSocket, WebSocketUpgrade};
  pub use hyper::http::Method;
  pub use hyper::http::StatusCode;
}

pub mod error;
pub mod kernel;
pub mod router;
pub mod server;
pub mod types;

pub use error::Error;
pub use kernel::{Context, DynEndpoint, Endpoint, IntoResponse, Middleware, Next};
pub use router::Router;
pub use server::Server;
use std::net::SocketAddr;
pub use types::{AnyResult, HyperRequest, HyperResponse, Result};
// re-export
pub use async_trait::async_trait;
pub use bytes::Bytes;
pub use http_body_util::Full;
pub use hyper::{header, http, Method, StatusCode};
#[must_use]
pub fn new(addr: SocketAddr) -> Server {
  Server::bind(addr)
}

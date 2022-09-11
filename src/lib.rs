pub mod error;
pub mod fs;
pub mod kernel;
pub mod request;
pub mod response;
pub mod router;
pub mod server;
pub mod types;
pub mod utils;

pub use error::Error;
pub use fs::{ServeDir, ServeFile};
pub use kernel::{DynEndpoint, Endpoint, IntoResponse, Middleware, Next};
pub use request::Request;
pub use response::Response;
pub use router::Router;
pub use server::Server;
pub use types::{AnyResult, HyperRequest, HyperResponse, Result};
#[must_use]
pub fn new(addr: &str) -> Server {
  Server::bind(addr)
}

// re-export
pub use hyper::http;
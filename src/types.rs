//! Shared type aliases.

use std::future::Future;
use std::pin::Pin;

use bytes::Bytes;

use crate::{Error, Response};

/// A boxed, sendable future — the return type of dynamic dispatch
/// for handlers and middleware.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The framework result. The `Ok` side defaults to [`Response`], so
/// handlers and middleware can simply write `Result`.
///
/// ```ignore
/// async fn auth(ctx: Context, next: Next) -> desire::Result {
///     next.run(ctx).await
/// }
/// ```
pub type Result<T = Response, E = Error> = std::result::Result<T, E>;

/// A type-erased error.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A boxed body with a type-erased error, used for request bodies.
pub(crate) type AnyBody = http_body_util::combinators::UnsyncBoxBody<Bytes, BoxError>;

/// A raw `hyper` request with a streaming body.
pub type HyperRequest = hyper::Request<hyper::body::Incoming>;

/// A `hyper` response carrying the framework [`Body`](crate::Body).
pub type HyperResponse = hyper::Response<crate::Body>;

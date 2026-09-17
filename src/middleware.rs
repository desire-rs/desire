//! The `Middleware` trait, the `Next` runner, and built-in middleware.

use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use hyper::header::{
  ACCESS_CONTROL_ALLOW_CREDENTIALS, ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS,
  ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_MAX_AGE, ACCESS_CONTROL_REQUEST_METHOD, HeaderValue,
  ORIGIN, VARY,
};

use crate::context::Context;
use crate::handler::AnyHandler;
use crate::into_response::IntoResponse;
use crate::types::{BoxFuture, Result};

/// Middleware in the onion model. Plain `async fn`s implement it:
///
/// ```ignore
/// async fn auth(mut ctx: Context, next: Next) -> Result {
///     let token = ctx.header("authorization").ok_or_else(Error::unauthorized)?;
///     ctx.insert(CurrentUser(verify(token)?));
///     next.run(ctx).await
/// }
///
/// app.with(auth);
/// ```
/// Middleware in the onion model. Plain `async fn`s implement it:
pub trait Middleware: Send + Sync + 'static {
  /// Run the middleware. Call [`Next::run`] to continue the chain.
  fn call(self: Arc<Self>, ctx: Context, next: Next) -> BoxFuture<'static, Result>;
}

impl<F, Fut, R> Middleware for F
where
  F: Fn(Context, Next) -> Fut + Send + Sync + 'static,
  Fut: Future<Output = R> + Send + 'static,
  R: IntoResponse,
{
  fn call(self: Arc<Self>, ctx: Context, next: Next) -> BoxFuture<'static, Result> {
    Box::pin(async move { Ok((self)(ctx, next).await.into_response()) })
  }
}

/// The remaining middleware chain plus the terminal handler. Call
/// [`Next::run`] to pass the request down the chain.
pub struct Next {
  pub(crate) middlewares: Arc<[Arc<dyn Middleware>]>,
  pub(crate) handler: AnyHandler,
  pub(crate) index: usize,
}

impl Next {
  /// Continue down the chain: the next middleware, or the handler.
  pub async fn run(self, ctx: Context) -> Result {
    if self.index < self.middlewares.len() {
      let middleware = Arc::clone(&self.middlewares[self.index]);
      let next = Next {
        middlewares: Arc::clone(&self.middlewares),
        handler: Arc::clone(&self.handler),
        index: self.index + 1,
      };
      middleware.call(ctx, next).await
    } else {
      (self.handler)(ctx).await
    }
  }
}

// ---- built-in middleware ----

/// Request logging: `method`, `path`, `status` and elapsed milliseconds
/// via `tracing::info!`.
/// Request logging: `method`, `path`, `status` and elapsed milliseconds
/// via `tracing::info!`.
pub fn logger() -> impl Middleware {
  |ctx: Context, next: Next| async move {
    let start = Instant::now();
    let method = ctx.method().clone();
    let path = ctx.path().to_owned();
    let res = next.run(ctx).await;
    let status = match &res {
      Ok(res) => res.status().as_u16(),
      Err(err) => err.status().as_u16(),
    };
    tracing::info!(
      method = %method,
      path = %path,
      status,
      elapsed_ms = start.elapsed().as_millis() as u64,
      "request"
    );
    res
  }
}

/// Abort the request with a 504 envelope if the downstream chain takes
/// longer than `duration`.
/// Abort the request with a 504 envelope if the downstream chain takes
/// longer than `duration`.
pub fn timeout(duration: Duration) -> impl Middleware {
  move |ctx: Context, next: Next| async move {
    match tokio::time::timeout(duration, next.run(ctx)).await {
      Ok(res) => res,
      Err(_) => Err(crate::Error::Timeout),
    }
  }
}

/// Override the request body size limit for the downstream chain (the
/// framework default is 2 MiB). Oversized bodies fail with 413.
/// Override the request body size limit for the downstream chain (the
/// framework default is 2 MiB). Oversized bodies fail with 413.
pub fn body_limit(limit: usize) -> impl Middleware {
  move |ctx: Context, next: Next| async move {
    ctx.set_body_limit(limit);
    next.run(ctx).await
  }
}

/// Configuration for [`cors`].
#[derive(Debug, Clone)]
pub struct CorsConfig {
  /// Allowed origins; `"*"` allows any.
  pub origins: Vec<String>,
  /// Allowed methods for preflight.
  pub methods: Vec<&'static str>,
  /// Allowed request headers for preflight.
  pub headers: Vec<&'static str>,
  /// Whether to allow credentials.
  pub credentials: bool,
  /// Preflight cache duration in seconds.
  pub max_age_secs: u32,
}

/// Sensible defaults: any origin, common methods, any headers.
impl Default for CorsConfig {
  fn default() -> Self {
    CorsConfig {
      origins: vec!["*".to_owned()],
      methods: vec!["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"],
      headers: vec!["*"],
      credentials: false,
      max_age_secs: 3600,
    }
  }
}

/// Cross-origin resource sharing. Handles preflight `OPTIONS` requests
/// and appends CORS headers to other responses.
/// Cross-origin resource sharing. Handles preflight `OPTIONS` requests
/// and appends CORS headers to other responses.
pub fn cors(config: CorsConfig) -> impl Middleware {
  move |ctx: Context, next: Next| {
    let config = config.clone();
    async move {
      let origin = ctx.header(ORIGIN.as_str()).map(ToOwned::to_owned);
      let is_preflight = ctx.method() == hyper::Method::OPTIONS
        && ctx.header(ACCESS_CONTROL_REQUEST_METHOD.as_str()).is_some();

      let res: Result = if is_preflight {
        let mut res = crate::Response::text(String::new());
        *res.status_mut() = hyper::StatusCode::NO_CONTENT;
        apply_cors(&mut res, config, origin.as_deref());
        Ok(res)
      } else {
        let mut res = next.run(ctx).await?;
        apply_cors(&mut res, config, origin.as_deref());
        Ok(res)
      };
      res
    }
  }
}

fn apply_cors(res: &mut crate::Response, config: CorsConfig, origin: Option<&str>) {
  let mut headers = std::mem::take(res.headers_mut());
  let allow_origin = match origin {
    Some(req_origin) => {
      if config.origins.iter().any(|o| o == "*" || o == req_origin) {
        Some(req_origin.to_owned())
      } else {
        None
      }
    }
    // Non-CORS request (no Origin header): still emit the configured origin.
    None => config.origins.first().cloned(),
  };
  if let Some(origin) = allow_origin {
    if let Ok(v) = HeaderValue::from_str(&origin) {
      headers.insert(ACCESS_CONTROL_ALLOW_ORIGIN, v);
    }
    if config.credentials {
      headers.insert(
        ACCESS_CONTROL_ALLOW_CREDENTIALS,
        HeaderValue::from_static("true"),
      );
    }
    if !config.methods.is_empty() {
      if let Ok(v) = HeaderValue::from_str(&config.methods.join(", ")) {
        headers.insert(ACCESS_CONTROL_ALLOW_METHODS, v);
      }
    }
    if !config.headers.is_empty() {
      if let Ok(v) = HeaderValue::from_str(&config.headers.join(", ")) {
        headers.insert(ACCESS_CONTROL_ALLOW_HEADERS, v);
      }
    }
    if let Ok(v) = HeaderValue::from_str(&config.max_age_secs.to_string()) {
      headers.insert(ACCESS_CONTROL_MAX_AGE, v);
    }
    if config.origins.iter().any(|o| o != "*") {
      headers.append(VARY, HeaderValue::from_static("Origin"));
    }
  }
  *res.headers_mut() = headers;
}

/// gzip response compression for clients that send
/// `Accept-Encoding: gzip` (requires the `gzip` feature).
///
/// Bodies smaller than 1 KiB are passed through uncompressed —
/// compressing them costs more CPU than it saves and usually grows the
/// payload. Tune the threshold with [`gzip_with`]. Responses that
/// already carry a `Content-Encoding`, or have no body (204/304), are
/// skipped. When compressing, the `Content-Length` header is dropped
/// (the stream length is unknown until encoding completes).
#[cfg(feature = "gzip")]
pub fn gzip() -> impl Middleware {
  gzip_with(1024)
}

/// [`gzip`] with a configurable minimum body size in bytes. Bodies
/// with a known size (buffered bodies report one via their size hint)
/// below `min_size` are passed through uncompressed; streamed bodies
/// (unknown size) are always compressed. `min_size == 0` compresses
/// everything.
#[cfg(feature = "gzip")]
pub fn gzip_with(min_size: usize) -> impl Middleware {
  move |ctx: Context, next: Next| async move {
    let accepts_gzip = ctx
      .header(hyper::header::ACCEPT_ENCODING.as_str())
      .is_some_and(|v| {
        v.split(',')
          .any(|part| part.split(';').next().unwrap_or("").trim() == "gzip")
      });

    let inner = async {
      let mut res = next.run(ctx).await?;
      let status = res.status();
      let body_too_small = http_body::Body::size_hint(res.body())
        .exact()
        .is_some_and(|len| (len as usize) < min_size);
      let bodyless = status == hyper::StatusCode::NO_CONTENT
        || status == hyper::StatusCode::NOT_MODIFIED
        || status.is_informational();
      if accepts_gzip
        && !body_too_small
        && !bodyless
        && !res.headers().contains_key(hyper::header::CONTENT_ENCODING)
      {
        use http_body_util::BodyExt as _;
        let body = std::mem::replace(res.body_mut(), crate::body::empty());
        let reader = tokio_util::io::StreamReader::new(body.into_data_stream());
        let encoder = async_compression::tokio::bufread::GzipEncoder::new(reader);
        *res.body_mut() = crate::body::from_stream(tokio_util::io::ReaderStream::new(encoder));

        let headers = res.headers_mut();
        headers.remove(hyper::header::CONTENT_LENGTH);
        headers.insert(
          hyper::header::CONTENT_ENCODING,
          HeaderValue::from_static("gzip"),
        );
        headers.append(
          hyper::header::VARY,
          HeaderValue::from_static("Accept-Encoding"),
        );
      }
      Ok(res)
    };
    let out: Result = inner.await;
    out
  }
}

//! The concrete HTTP response type.

use bytes::Bytes;
use hyper::StatusCode;
use hyper::header::{CONTENT_TYPE, HeaderMap, HeaderValue, LOCATION};
use serde::Serialize;

use crate::body::{self, Body};
use crate::types::HyperResponse;

/// An HTTP response: status, headers, and a streaming [`Body`].
#[derive(Debug)]
pub struct Response {
  pub(crate) inner: HyperResponse,
}

impl Response {
  pub(crate) fn from_hyper(inner: HyperResponse) -> Self {
    Response { inner }
  }

  /// Unwrap into the raw `hyper` response.
  /// Unwrap into the raw `hyper` response.
  pub fn into_hyper(self) -> HyperResponse {
    self.inner
  }

  /// The response status.
  pub fn status(&self) -> StatusCode {
    self.inner.status()
  }

  /// Mutable access to the response status.
  pub fn status_mut(&mut self) -> &mut StatusCode {
    self.inner.status_mut()
  }

  /// The response headers.
  pub fn headers(&self) -> &HeaderMap {
    self.inner.headers()
  }

  /// Mutable access to the response headers.
  pub fn headers_mut(&mut self) -> &mut HeaderMap<HeaderValue> {
    self.inner.headers_mut()
  }

  /// A 200 `text/plain; charset=utf-8` response.
  pub fn text(s: impl Into<String>) -> Self {
    raw_response(
      StatusCode::OK,
      Some(TEXT_PLAIN),
      body::full(s.into().into_bytes()),
    )
  }

  /// A 200 `text/html; charset=utf-8` response.
  pub fn html(s: impl Into<String>) -> Self {
    raw_response(
      StatusCode::OK,
      Some(TEXT_HTML),
      body::full(s.into().into_bytes()),
    )
  }

  /// A 200 `application/json` response (no envelope).
  pub fn json<T: Serialize + ?Sized>(value: &T) -> Self {
    raw_response(StatusCode::OK, Some(APPLICATION_JSON), body::json(value))
  }

  /// A 200 `application/octet-stream` response.
  pub fn bytes(b: impl Into<Bytes>) -> Self {
    raw_response(
      StatusCode::OK,
      Some(APPLICATION_OCTET_STREAM),
      body::full(b),
    )
  }

  /// A redirect response to `location`.
  /// A redirect response to `location`.
  pub fn redirect(status: StatusCode, location: &str) -> Self {
    let mut res = raw_response(status, None, body::empty());
    if let Ok(value) = HeaderValue::from_str(location) {
      res.headers_mut().insert(LOCATION, value);
    }
    res
  }
}

impl From<HyperResponse> for Response {
  fn from(inner: HyperResponse) -> Self {
    Response::from_hyper(inner)
  }
}

pub(crate) const TEXT_PLAIN: &str = "text/plain; charset=utf-8";
pub(crate) const TEXT_HTML: &str = "text/html; charset=utf-8";
pub(crate) const APPLICATION_JSON: &str = "application/json";
pub(crate) const APPLICATION_OCTET_STREAM: &str = "application/octet-stream";

/// Build a response, tolerating only impossible failures gracefully.
pub(crate) fn raw_response(status: StatusCode, content_type: Option<&str>, body: Body) -> Response {
  let mut builder = hyper::http::Response::builder().status(status);
  if let Some(ct) = content_type {
    builder = builder.header(CONTENT_TYPE, ct);
  }
  match builder.body(body) {
    Ok(inner) => Response::from_hyper(inner),
    // Only reachable with an invalid header value; fall back to a bare 500.
    Err(_) => Response::from_hyper(
      hyper::http::Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(body::empty())
        .expect("bare 500 response cannot fail"),
    ),
  }
}

pub(crate) fn with_content_type(res: &mut Response, content_type: &str) {
  if let Ok(value) = HeaderValue::from_str(content_type) {
    res.headers_mut().insert(CONTENT_TYPE, value);
  }
}

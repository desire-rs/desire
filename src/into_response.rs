//! `IntoResponse` — anything a handler can return.

use std::borrow::Cow;

use bytes::Bytes;
use hyper::StatusCode;
use serde::Serialize;

use crate::body;
use crate::response::{APPLICATION_JSON, TEXT_HTML, TEXT_PLAIN, raw_response, with_content_type};
use crate::{Error, Resp, Response};

/// Conversion into an HTTP [`Response`]. This is what handlers return:
/// `Resp<T>`, strings, bytes, JSON values, tuples of status + body, and
/// `Result<T, E>` where `E: Into<Error>` all implement it.
///
/// The conversion is infallible — errors are themselves rendered into
/// the unified envelope.
pub trait IntoResponse {
  /// Convert into an HTTP response.
  fn into_response(self) -> Response;
}

impl IntoResponse for Response {
  fn into_response(self) -> Response {
    self
  }
}

impl IntoResponse for crate::Body {
  fn into_response(self) -> Response {
    raw_response(StatusCode::OK, None, self)
  }
}

impl IntoResponse for Error {
  fn into_response(self) -> Response {
    self.to_resp().into_response()
  }
}

impl<T, E> IntoResponse for Result<T, E>
where
  T: IntoResponse,
  E: Into<Error>,
{
  fn into_response(self) -> Response {
    match self {
      Ok(resp) => resp.into_response(),
      Err(err) => err.into().into_response(),
    }
  }
}

impl IntoResponse for () {
  fn into_response(self) -> Response {
    raw_response(StatusCode::OK, None, body::empty())
  }
}

impl IntoResponse for StatusCode {
  fn into_response(self) -> Response {
    raw_response(self, None, body::empty())
  }
}

impl IntoResponse for &'static str {
  fn into_response(self) -> Response {
    raw_response(
      StatusCode::OK,
      Some(TEXT_PLAIN),
      body::full(Bytes::from_static(self.as_bytes())),
    )
  }
}

impl IntoResponse for String {
  fn into_response(self) -> Response {
    Response::text(self)
  }
}

impl IntoResponse for Cow<'static, str> {
  fn into_response(self) -> Response {
    match self {
      Cow::Borrowed(s) => s.into_response(),
      Cow::Owned(s) => s.into_response(),
    }
  }
}

impl IntoResponse for Bytes {
  fn into_response(self) -> Response {
    Response::bytes(self)
  }
}

impl IntoResponse for Vec<u8> {
  fn into_response(self) -> Response {
    Response::bytes(self)
  }
}

impl IntoResponse for &'static [u8] {
  fn into_response(self) -> Response {
    Response::bytes(Bytes::from_static(self))
  }
}

impl IntoResponse for serde_json::Value {
  fn into_response(self) -> Response {
    let mut res = Response::json(&self);
    with_content_type(&mut res, APPLICATION_JSON);
    res
  }
}

/// A JSON response outside the envelope: `Json(user)` serializes `user`
/// directly, for APIs that do not use the `Resp` convention.
///
/// ```no_run
/// use desire::prelude::*;
/// # use serde::Serialize;
/// # #[derive(Serialize)] struct Stats { requests: u64 }
/// # fn stats() -> Stats { Stats { requests: 1 } }
/// async fn raw_json() -> impl IntoResponse {
///     Json(stats()) // {"requests":1}, no {"code":..} wrapper
/// }
/// # fn main() {}
/// ```
#[derive(Debug, Clone)]
pub struct Json<T>(pub T);

impl<T> IntoResponse for Json<T>
where
  T: Serialize,
{
  fn into_response(self) -> Response {
    let mut res = Response::json(&self.0);
    with_content_type(&mut res, APPLICATION_JSON);
    res
  }
}

/// An HTML response: `Html("<h1>hi</h1>")`.
///
/// ```no_run
/// use desire::prelude::*;
/// async fn page() -> impl IntoResponse {
///     Html("<h1>hello</h1>") // text/html; charset=utf-8
/// }
/// # fn main() {}
/// ```
#[derive(Debug, Clone)]
pub struct Html<T>(pub T);

impl<T> IntoResponse for Html<T>
where
  T: Into<Cow<'static, str>>,
{
  fn into_response(self) -> Response {
    let mut res = Response::html(self.0.into());
    with_content_type(&mut res, TEXT_HTML);
    res
  }
}

impl<T> IntoResponse for Resp<T>
where
  T: Serialize,
{
  fn into_response(self) -> Response {
    let status = self.status();
    let body = body::json(&self);
    raw_response(status, Some(APPLICATION_JSON), body)
  }
}

impl<T> IntoResponse for (StatusCode, T)
where
  T: IntoResponse,
{
  fn into_response(self) -> Response {
    let mut res = self.1.into_response();
    *res.status_mut() = self.0;
    res
  }
}

impl<T> IntoResponse for (u16, T)
where
  T: IntoResponse,
{
  fn into_response(self) -> Response {
    match StatusCode::from_u16(self.0) {
      Ok(status) => (status, self.1).into_response(),
      Err(e) => Error::internal(e).into_response(),
    }
  }
}

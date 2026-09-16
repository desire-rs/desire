//! An in-memory test client: drive the app's full middleware + routing
//! pipeline without sockets or ports.
//!
//! ```ignore
//! use desire::test::TestClient;
//!
//! #[tokio::test]
//! async fn get_user_ok() {
//!     let tc = TestClient::new(app_under_test());
//!     let res = tc.get("/users/1").send().await;
//!     res.assert_status_ok();
//!     let body: Resp<User> = res.json().await;
//! }
//! ```

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::header::CONTENT_TYPE;
use hyper::{HeaderMap, Method, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::app::dispatch;
use crate::resp::Resp;
use crate::{App, Error, Result};

/// A client bound to one compiled app. Cheap to clone.
#[derive(Clone)]
pub struct TestClient {
  app: std::sync::Arc<crate::app::BuiltApp>,
}

impl TestClient {
  /// Compile the app and bind a client to it. Panics on route
  /// conflicts, just like a real server start would.
  pub fn new(app: App) -> Self {
    TestClient {
      app: std::sync::Arc::new(app.build()),
    }
  }

  /// Build a request with an explicit method.
  pub fn request(&self, method: Method, path: &str) -> TestRequest {
    TestRequest {
      app: std::sync::Arc::clone(&self.app),
      method,
      path: path.to_owned(),
      headers: HeaderMap::new(),
      body: Bytes::new(),
    }
  }

  /// A `GET` request.
  pub fn get(&self, path: &str) -> TestRequest {
    self.request(Method::GET, path)
  }

  /// A `POST` request.
  pub fn post(&self, path: &str) -> TestRequest {
    self.request(Method::POST, path)
  }

  /// A `PUT` request.
  pub fn put(&self, path: &str) -> TestRequest {
    self.request(Method::PUT, path)
  }

  /// A `PATCH` request.
  pub fn patch(&self, path: &str) -> TestRequest {
    self.request(Method::PATCH, path)
  }

  /// A `DELETE` request.
  pub fn delete(&self, path: &str) -> TestRequest {
    self.request(Method::DELETE, path)
  }

  /// A `HEAD` request.
  pub fn head(&self, path: &str) -> TestRequest {
    self.request(Method::HEAD, path)
  }

  /// An `OPTIONS` request.
  pub fn options(&self, path: &str) -> TestRequest {
    self.request(Method::OPTIONS, path)
  }
}

/// A request under construction.
pub struct TestRequest {
  app: std::sync::Arc<crate::app::BuiltApp>,
  method: Method,
  path: String,
  headers: HeaderMap,
  body: Bytes,
}

impl TestRequest {
  /// Append a request header.
  pub fn header(mut self, name: &str, value: impl AsRef<str>) -> Self {
    if let (Ok(name), Ok(value)) = (
      hyper::header::HeaderName::try_from(name),
      hyper::header::HeaderValue::try_from(value.as_ref()),
    ) {
      self.headers.insert(name, value);
    }
    self
  }

  /// Serialize as the JSON body and set the content-type.
  /// Serialize as the JSON body and set the content-type.
  pub fn json<T: Serialize + ?Sized>(mut self, value: &T) -> Self {
    match serde_json::to_vec(value) {
      Ok(bytes) => {
        self.body = Bytes::from(bytes);
        self.headers.insert(
          CONTENT_TYPE,
          hyper::header::HeaderValue::from_static("application/json"),
        );
      }
      Err(e) => tracing::error!(error = %e, "test request serialization failed"),
    }
    self
  }

  /// Serialize as `application/x-www-form-urlencoded`.
  /// Serialize as `application/x-www-form-urlencoded`.
  pub fn form<T: Serialize + ?Sized>(mut self, value: &T) -> Self {
    match serde_urlencoded::to_string(value) {
      Ok(s) => {
        self.body = Bytes::from(s);
        self.headers.insert(
          CONTENT_TYPE,
          hyper::header::HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
      }
      Err(e) => tracing::error!(error = %e, "test request serialization failed"),
    }
    self
  }

  /// Set a raw request body.
  pub fn body(mut self, bytes: impl Into<Bytes>) -> Self {
    self.body = bytes.into();
    self
  }

  /// Drive the request through the app's dispatch pipeline.
  pub async fn send(self) -> TestResponse {
    let uri: hyper::Uri = self
      .path
      .parse()
      .unwrap_or_else(|_| "/".parse().expect("fallback uri"));
    let body = Full::new(self.body)
      .map_err(|e: std::convert::Infallible| match e {})
      .boxed_unsync();
    let mut builder = hyper::Request::builder().method(self.method).uri(uri);
    *builder.headers_mut().expect("builder is valid") = self.headers;
    let req = builder.body(body).expect("valid test request");
    let remote: std::net::SocketAddr = "127.0.0.1:65000".parse().expect("remote addr");
    let response = dispatch(&self.app, req, Some(remote)).await;
    let (parts, body) = response.into_parts();
    let bytes = body
      .collect()
      .await
      .map(|c| c.to_bytes())
      .unwrap_or_default();
    TestResponse {
      status: parts.status,
      headers: parts.headers,
      bytes,
    }
  }
}

/// The buffered response of a [`TestRequest`].
pub struct TestResponse {
  status: StatusCode,
  headers: HeaderMap,
  bytes: Bytes,
}

impl TestResponse {
  /// The response status.
  pub fn status(&self) -> StatusCode {
    self.status
  }

  /// The response headers.
  pub fn headers(&self) -> &HeaderMap {
    &self.headers
  }

  /// One response header as a UTF-8 string.
  pub fn header(&self, name: &str) -> Option<&str> {
    self.headers.get(name)?.to_str().ok()
  }

  /// The raw response body.
  pub fn bytes(&self) -> Bytes {
    self.bytes.clone()
  }

  /// The response body as a UTF-8 string.
  pub fn text(&self) -> String {
    String::from_utf8_lossy(&self.bytes).into_owned()
  }

  /// Deserialize the body as plain JSON.
  /// Deserialize the body as plain JSON.
  pub fn json<T: DeserializeOwned>(&self) -> Result<T> {
    serde_json::from_slice(&self.bytes).map_err(Error::from)
  }

  /// Deserialize the body as a `Resp` envelope.
  /// Deserialize the body as a `Resp` envelope.
  pub fn resp<T: DeserializeOwned>(&self) -> Result<Resp<T>> {
    self.json()
  }

  /// Assert the status is 200.
  pub fn assert_status_ok(&self) {
    assert_eq!(
      self.status,
      StatusCode::OK,
      "expected 200, got {}: {}",
      self.status,
      self.text()
    );
  }

  /// Assert the status.
  pub fn assert_status(&self, expected: StatusCode) {
    assert_eq!(
      self.status,
      expected,
      "expected {expected}, got {}: {}",
      self.status,
      self.text()
    );
  }

  /// Assert the envelope succeeded (code 0) and return the data.
  /// Assert the envelope succeeded (code 0) and return the data.
  pub fn assert_ok_data<T: DeserializeOwned>(&self) -> T {
    self.assert_status_ok();
    let resp: Resp<T> = self.json().expect("envelope body");
    assert_eq!(
      resp.code, 0,
      "expected code 0, got {}: {}",
      resp.code, resp.msg
    );
    resp.data.expect("envelope data")
  }
}

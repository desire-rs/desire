//! The per-request context: request data extraction and typed storage.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::net::SocketAddr;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use bytes::{Buf, Bytes};
use cookie::Cookie;
use http_body_util::BodyExt;
use hyper::header::COOKIE;
use hyper::{HeaderMap, Method, Uri};
use serde::de::DeserializeOwned;

use crate::error::{missing_param, param_error};
use crate::state::{self, StateMap};
use crate::types::AnyBody;
use crate::{Error, Result};

/// Everything a handler needs about the current request.
///
/// Handlers receive it by value: `async fn handler(ctx: Context)`.
/// Extraction methods take `&self` — the body is read through interior
/// mutability, cached after the first read, and shared by all extractors.
pub struct Context {
  method: Method,
  uri: Uri,
  headers: HeaderMap,
  params: HashMap<String, String>,
  body: Mutex<BodySlot>,
  extensions: crate::state::TypeMap,
  on_upgrade: Mutex<Option<hyper::upgrade::OnUpgrade>>,
  state: Arc<StateMap>,
  remote_addr: Option<SocketAddr>,
}

struct BodySlot {
  inner: Option<AnyBody>,
  cache: Option<Bytes>,
  limit: usize,
}

impl Context {
  #[allow(clippy::too_many_arguments)] // internal constructor; all fields are required
  pub(crate) fn new(
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    params: HashMap<String, String>,
    incoming: AnyBody,
    max_body_size: usize,
    state: Arc<StateMap>,
    remote_addr: Option<SocketAddr>,
    on_upgrade: Option<hyper::upgrade::OnUpgrade>,
  ) -> Self {
    Context {
      method,
      uri,
      headers,
      params,
      body: Mutex::new(BodySlot {
        inner: Some(incoming),
        cache: None,
        limit: max_body_size,
      }),
      extensions: crate::state::TypeMap::default(),
      on_upgrade: Mutex::new(on_upgrade),
      state,
      remote_addr,
    }
  }

  // ---- request view ----

  /// The HTTP method of the request.
  pub fn method(&self) -> &Method {
    &self.method
  }

  /// The request path (no query string).
  pub fn path(&self) -> &str {
    self.uri.path()
  }

  /// The raw query string, if any.
  pub fn query_string(&self) -> Option<&str> {
    self.uri.query()
  }

  /// All request headers.
  pub fn headers(&self) -> &HeaderMap {
    &self.headers
  }

  /// One request header as a UTF-8 string.
  pub fn header(&self, name: &str) -> Option<&str> {
    self.headers.get(name)?.to_str().ok()
  }

  /// The peer address, when the server provides it.
  pub fn remote_addr(&self) -> Option<&SocketAddr> {
    self.remote_addr.as_ref()
  }

  // ---- path params ----

  /// One path parameter, parsed via `FromStr`.
  ///
  /// ```ignore
  /// let id: i64 = ctx.param("id")?;
  /// ```
  pub fn param<T: FromStr>(&self, name: &str) -> Result<T>
  where
    T::Err: std::fmt::Display,
  {
    let raw = self.param_raw(name)?;
    raw
      .parse()
      .map_err(|e| param_error(name, std::any::type_name::<T>(), e))
  }

  /// One path parameter as a raw string.
  pub fn param_raw(&self, name: &str) -> Result<&str> {
    self
      .params
      .get(name)
      .map(String::as_str)
      .ok_or_else(|| missing_param(name))
  }

  /// Deserialize a struct from all path parameters at once.
  ///
  /// ```ignore
  /// #[derive(Deserialize)]
  /// struct Args { id: i64 }
  /// let args: Args = ctx.params()?;
  /// ```
  pub fn params<T: DeserializeOwned>(&self) -> Result<T> {
    let encoded: Vec<String> = self
      .params
      .iter()
      .map(|(k, v)| format!("{k}={}", percent_encode(v)))
      .collect();
    serde_urlencoded::from_str(&encoded.join("&")).map_err(Error::from)
  }

  // ---- query / body ----

  /// Deserialize the query string into a struct.
  ///
  /// Missing keys become errors for required fields; use `Option<T>`
  /// fields for optional query parameters.
  pub fn query<T: DeserializeOwned>(&self) -> Result<T> {
    let query = self.uri.query().unwrap_or("");
    serde_urlencoded::from_str(query).map_err(Error::from)
  }

  /// Read and deserialize the body as JSON.
  ///
  /// Errors carry the serde path, e.g. `body.email: missing field`.
  pub async fn json<T: DeserializeOwned>(&self) -> Result<T> {
    if let Some(ct) = self.header_str(hyper::header::CONTENT_TYPE.as_str()) {
      if !ct.contains("json") {
        return Err(Error::Body(format!(
          "expected a JSON content-type, got `{ct}`"
        )));
      }
    }
    let bytes = self.body_bytes().await?;
    serde_json::from_slice(&bytes).map_err(Error::from)
  }

  /// Read and deserialize an `application/x-www-form-urlencoded` body.
  pub async fn form<T: DeserializeOwned>(&self) -> Result<T> {
    if let Some(ct) = self.header_str(hyper::header::CONTENT_TYPE.as_str()) {
      if !ct.contains("urlencoded") {
        return Err(Error::Body(format!(
          "expected an urlencoded content-type, got `{ct}`"
        )));
      }
    }
    let bytes = self.body_bytes().await?;
    let s = std::str::from_utf8(&bytes)
      .map_err(|e| Error::Body(format!("body is not valid utf-8: {e}")))?;
    serde_urlencoded::from_str(s).map_err(Error::from)
  }

  /// The raw request body. Cached after the first read, so `json`,
  /// `form`, and `body_bytes` can be mixed freely.
  pub async fn body_bytes(&self) -> Result<Bytes> {
    let (incoming, limit) = {
      let mut slot = self.body.lock().expect("body lock poisoned");
      if let Some(bytes) = &slot.cache {
        return Ok(bytes.clone());
      }
      (slot.inner.take(), slot.limit)
    };
    let Some(incoming) = incoming else {
      return Ok(Bytes::new());
    };
    let collected = Bounded {
      inner: incoming,
      remaining: limit as u64,
    }
    .collect()
    .await;
    let bytes = match collected {
      Ok(c) => c.to_bytes(),
      Err(e) => {
        if e.downcast_ref::<BodyTooLarge>().is_some() {
          return Err(Error::PayloadTooLarge);
        }
        return Err(Error::Body(format!("failed to read request body: {e}")));
      }
    };
    self.body.lock().expect("body lock poisoned").cache = Some(bytes.clone());
    Ok(bytes)
  }

  /// The request body as a UTF-8 string.
  pub async fn body_text(&self) -> Result<String> {
    let bytes = self.body_bytes().await?;
    String::from_utf8(bytes.to_vec())
      .map_err(|e| Error::Body(format!("body is not valid utf-8: {e}")))
  }

  /// Adjust the request body size limit (bytes). The framework default
  /// is 2 MiB; the [`body_limit`](crate::middleware::body_limit)
  /// middleware is a nicer way to change it.
  pub fn set_body_limit(&self, limit: usize) {
    self.body.lock().expect("body lock poisoned").limit = limit;
  }

  fn header_str(&self, name: &str) -> Option<String> {
    self.headers.get(name)?.to_str().ok().map(ToOwned::to_owned)
  }

  // ---- websocket ----

  /// Begin a WebSocket handshake. Returns a handle to complete it with
  /// [`WebSocketUpgrade::on_upgrade`](crate::ws::WebSocketUpgrade), or
  /// an error if this is not a websocket request (requires the `ws`
  /// feature).
  #[cfg(feature = "ws")]
  pub fn websocket(&self) -> Result<crate::ws::WebSocketUpgrade> {
    let is_websocket = self
      .header("upgrade")
      .is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
    let version_ok = self
      .header("sec-websocket-version")
      .is_none_or(|v| v.trim() == "13");
    let key = self.header("sec-websocket-key");
    let on_upgrade = self
      .on_upgrade
      .lock()
      .expect("upgrade lock poisoned")
      .take();

    if !is_websocket || !version_ok || key.is_none() {
      return Err(Error::bad_request("not a websocket handshake"));
    }
    let Some(on_upgrade) = on_upgrade else {
      return Err(Error::bad_request(
        "connection upgrade is unavailable on this route",
      ));
    };
    Ok(crate::ws::WebSocketUpgrade {
      key: key.expect("checked above").to_owned(),
      on_upgrade,
    })
  }

  // ---- cookies ----

  /// Read a request cookie.
  pub fn cookie(&self, name: &str) -> Option<Cookie<'static>> {
    for value in self.headers.get_all(COOKIE) {
      let Ok(s) = value.to_str() else { continue };
      for cookie in Cookie::split_parse(s).flatten() {
        if cookie.name() == name {
          return Some(cookie.into_owned());
        }
      }
    }
    None
  }

  // ---- shared state & per-request data ----

  /// Borrow shared application state registered via
  /// [`App::state`](crate::App::state). Zero clones — the returned
  /// reference lives as long as the context.
  pub fn state<T: Send + Sync + 'static>(&self) -> Result<&T> {
    state::get(&self.state).ok_or_else(|| Error::Internal {
      logged: format!(
        "state `{}` is not registered; call App::state first",
        std::any::type_name::<T>()
      ),
    })
  }

  /// Store a typed per-request value (e.g. the authenticated user set
  /// by an auth middleware).
  pub fn insert<T: Send + Sync + 'static>(&mut self, val: T) {
    self.extensions.insert(val);
  }
  /// Read a typed per-request value previously inserted.
  pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
    self.extensions.get::<T>()
  }
}

/// Marker error emitted when the request body exceeds its limit.
#[derive(Debug)]
pub(crate) struct BodyTooLarge;

impl std::fmt::Display for BodyTooLarge {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str("request body exceeded its size limit")
  }
}

impl std::error::Error for BodyTooLarge {}

/// A request body capped at a byte limit. Unlike `http_body_util::Limited`
/// the error type is fixed, which keeps handler type-checking simple.
struct Bounded {
  inner: AnyBody,
  remaining: u64,
}

impl hyper::body::Body for Bounded {
  type Data = Bytes;
  type Error = crate::types::BoxError;

  fn poll_frame(
    self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<Option<Result<hyper::body::Frame<Bytes>, Self::Error>>> {
    use std::task::Poll;
    // Bounded and AnyBody are both Unpin.
    let this = self.get_mut();
    match Pin::new(&mut this.inner).poll_frame(cx) {
      Poll::Pending => Poll::Pending,
      Poll::Ready(None) => Poll::Ready(None),
      Poll::Ready(Some(Ok(frame))) => {
        if let Some(data) = frame.data_ref() {
          let len = data.remaining() as u64;
          if len > this.remaining {
            this.remaining = 0;
            return Poll::Ready(Some(Err(Box::new(BodyTooLarge))));
          }
          this.remaining -= len;
        }
        Poll::Ready(Some(Ok(frame)))
      }
      Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
    }
  }
}

// ---- percent encoding (matchit does not decode params) ----

pub(crate) fn percent_decode(s: &str) -> String {
  let bytes = s.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      b'%' if i + 2 < bytes.len() => match (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
        (Some(hi), Some(lo)) => {
          out.push(hi * 16 + lo);
          i += 3;
        }
        _ => {
          out.push(b'%');
          i += 1;
        }
      },
      b => {
        out.push(b);
        i += 1;
      }
    }
  }
  String::from_utf8_lossy(&out).into_owned()
}

pub(crate) fn percent_encode(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  for &b in s.as_bytes() {
    match b {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
        out.push(b as char);
      }
      _ => {
        let _ = write!(out, "%{b:02X}");
      }
    }
  }
  out
}

fn hex_val(b: u8) -> Option<u8> {
  match b {
    b'0'..=b'9' => Some(b - b'0'),
    b'a'..=b'f' => Some(b - b'a' + 10),
    b'A'..=b'F' => Some(b - b'A' + 10),
    _ => None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde::Deserialize;

  #[test]
  fn decode_roundtrip() {
    assert_eq!(percent_decode("a%20b"), "a b");
    assert_eq!(percent_decode("caf%C3%A9"), "café");
    assert_eq!(percent_decode("100%"), "100%");
    assert_eq!(percent_encode("a b&c=d"), "a%20b%26c%3Dd");
  }

  #[test]
  fn params_deserialize() {
    let map: HashMap<String, String> = [("id".to_owned(), "42".to_owned())].into_iter().collect();
    let encoded: Vec<String> = map
      .iter()
      .map(|(k, v)| format!("{k}={}", percent_encode(v)))
      .collect();
    #[derive(Deserialize)]
    struct Args {
      id: i64,
    }
    let args: Args = serde_urlencoded::from_str(&encoded.join("&")).unwrap();
    assert_eq!(args.id, 42);
  }
}

//! Server-Sent Events: the `text/event-stream` helper on top of
//! [`Body::from_stream`](crate::body::from_stream).
//!
//! ```ignore
//! use desire::sse::{Event, Sse};
//!
//! async fn ticks(_ctx: Context) -> Sse<impl Stream<Item = Result<Event, Error>>> {
//!   let stream = tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(Duration::from_secs(1)))
//!     .map(|_| Ok(Event::new().data("tick")));
//!   Sse::new(stream)
//! }
//!
//! App::new().route("/events", get(ticks));
//! ```

use std::fmt::Write as _;
use std::pin::Pin;

use futures_core::Stream;
use http_body::Frame;
use hyper::header::{CACHE_CONTROL, CONTENT_TYPE, HeaderValue};
use serde_json::to_string;
use tokio_stream::StreamExt as _;

use crate::response::raw_response;
use http_body_util::BodyExt;

use crate::{Error, IntoResponse, Response};
use hyper::StatusCode;

/// One server-sent event, built fluently.
///
/// ```ignore
/// Event::new()
///     .event("price")
///     .data("42")
///     .id("stock-1")
///     .retry(5_000)
/// ```
#[derive(Debug, Clone, Default)]
pub struct Event {
  event: Option<String>,
  data: Option<String>,
  id: Option<String>,
  retry: Option<u64>,
  comment: Option<String>,
}

impl Event {
  /// An empty event.
  pub fn new() -> Self {
    Event::default()
  }

  /// The `event:` field (the event type the client's listener sees).
  pub fn event(mut self, name: impl Into<String>) -> Self {
    self.event = Some(name.into());
    self
  }

  /// The `data:` field. Multiple `data` calls append one line each.
  pub fn data(mut self, data: impl Into<String>) -> Self {
    let line = data.into();
    match &mut self.data {
      Some(existing) => {
        existing.push('\n');
        existing.push_str(&line);
      }
      None => self.data = Some(line),
    }
    self
  }

  /// The `data:` field with the value serialized as JSON.
  pub fn json_data<T: serde::Serialize>(self, value: &T) -> Self {
    match to_string(value) {
      Ok(json) => self.data(json),
      Err(e) => {
        tracing::error!(error = %e, "failed to serialize SSE data");
        self.data("{\"error\":\"internal\"}")
      }
    }
  }

  /// The `id:` field (becomes the client's `Last-Event-ID`).
  pub fn id(mut self, id: impl Into<String>) -> Self {
    self.id = Some(id.into());
    self
  }

  /// The `retry:` field — the client reconnect delay in milliseconds.
  pub fn retry(mut self, millis: u64) -> Self {
    self.retry = Some(millis);
    self
  }

  /// A comment line (`:` prefix) — useful for keep-alives.
  pub fn comment(mut self, text: impl Into<String>) -> Self {
    self.comment = Some(text.into());
    self
  }

  /// Render the wire format, including the trailing blank line.
  pub fn render(&self) -> String {
    let mut out = String::new();
    if let Some(comment) = &self.comment {
      let _ = writeln!(out, ":{comment}");
    }
    if let Some(event) = &self.event {
      let _ = writeln!(out, "event:{event}");
    }
    if let Some(id) = &self.id {
      let _ = writeln!(out, "id:{id}");
    }
    if let Some(retry) = self.retry {
      let _ = writeln!(out, "retry:{retry}");
    }
    if let Some(data) = &self.data {
      for line in data.split('\n') {
        let _ = writeln!(out, "data:{line}");
      }
    }
    let _ = writeln!(out);
    out
  }
}

/// A streaming `text/event-stream` response. The stream item is an
/// [`Event`] (or an error, which ends the stream).
pub struct Sse<S> {
  stream: S,
}

impl<S> Sse<S> {
  /// Wrap a stream of events into an SSE response.
  pub fn new(stream: S) -> Self {
    Sse { stream }
  }

  /// Emit a `:keep-alive` comment every `interval` while the event
  /// stream is idle, so proxies and browsers do not time the
  /// connection out.
  ///
  /// ```ignore
  /// Sse::new(events).keep_alive(Duration::from_secs(15))
  /// ```
  pub fn keep_alive(self, interval: std::time::Duration) -> KeepAliveSse<S> {
    KeepAliveSse {
      events: self.stream,
      interval,
    }
  }
}

/// The response type of [`Sse::keep_alive`].
pub struct KeepAliveSse<S> {
  events: S,
  interval: std::time::Duration,
}

impl<S, E> IntoResponse for KeepAliveSse<S>
where
  S: Stream<Item = Result<Event, E>> + Send + 'static,
  E: Into<Error>,
{
  fn into_response(self) -> Response {
    let ticker = tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(self.interval))
      .map(|_| Ok(Event::new().comment("keep-alive")));

    let body = http_body_util::StreamBody::new(KeepAliveMerge {
      events: Box::pin(self.events),
      ticker: Box::pin(ticker),
    })
    .boxed_unsync();
    sse_response(body)
  }
}

/// Interleaves the event stream with the keep-alive ticker. Events are
/// preferred: if both are ready the event wins.
struct KeepAliveMerge<S, T> {
  events: Pin<Box<S>>,
  ticker: Pin<Box<T>>,
}

impl<S, T, E> Stream for KeepAliveMerge<S, T>
where
  S: Stream<Item = Result<Event, E>>,
  T: Stream<Item = Result<Event, Error>>,
  E: Into<Error>,
{
  type Item = Result<Frame<bytes::Bytes>, Error>;

  fn poll_next(
    self: Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<Option<Self::Item>> {
    use std::task::Poll;
    let this = self.get_mut();
    match this.events.as_mut().poll_next(cx) {
      Poll::Ready(Some(Ok(event))) => {
        return Poll::Ready(Some(Ok(Frame::data(bytes::Bytes::from(event.render())))));
      }
      Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(e.into()))),
      Poll::Ready(None) => return Poll::Ready(None),
      Poll::Pending => {}
    }
    match this.ticker.as_mut().poll_next(cx) {
      Poll::Ready(Some(Ok(event))) => {
        Poll::Ready(Some(Ok(Frame::data(bytes::Bytes::from(event.render())))))
      }
      Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
      Poll::Ready(None) | Poll::Pending => Poll::Pending,
    }
  }
}

impl<S, E> IntoResponse for Sse<S>
where
  S: Stream<Item = Result<Event, E>> + Send + 'static,
  E: Into<Error>,
{
  fn into_response(self) -> Response {
    let body = http_body_util::StreamBody::new(EventStream {
      inner: Box::pin(self.stream),
    })
    .boxed_unsync();
    sse_response(body)
  }
}

/// Shared response assembly for `Sse` and `KeepAliveSse`.
fn sse_response(body: crate::Body) -> Response {
  let mut res = raw_response(StatusCode::OK, Some("text/event-stream"), body);
  let headers = res.headers_mut();
  headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
  headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
  res
}

struct EventStream<S> {
  inner: std::pin::Pin<Box<S>>,
}

impl<S, E> Stream for EventStream<S>
where
  S: Stream<Item = Result<Event, E>>,
  E: Into<Error>,
{
  type Item = Result<Frame<bytes::Bytes>, Error>;

  fn poll_next(
    mut self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<Option<Self::Item>> {
    match self.inner.as_mut().poll_next(cx) {
      std::task::Poll::Pending => std::task::Poll::Pending,
      std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
      std::task::Poll::Ready(Some(Ok(event))) => {
        std::task::Poll::Ready(Some(Ok(Frame::data(bytes::Bytes::from(event.render())))))
      }
      std::task::Poll::Ready(Some(Err(e))) => std::task::Poll::Ready(Some(Err(e.into()))),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn renders_fields_in_order() {
    let event = Event::new().id("7").event("tick").retry(3000).data("hello");
    assert_eq!(
      event.render(),
      "event:tick\nid:7\nretry:3000\ndata:hello\n\n"
    );
  }

  #[test]
  fn multi_line_data_repeats_data_field() {
    let event = Event::new().data("line1\nline2");
    assert_eq!(event.render(), "data:line1\ndata:line2\n\n");
  }

  #[test]
  fn comment_and_empty_event() {
    let event = Event::new().comment("keep-alive");
    assert_eq!(event.render(), ":keep-alive\n\n");
  }

  #[test]
  fn json_data_serializes() {
    let event = Event::new().json_data(&serde_json::json!({ "ok": true }));
    assert_eq!(event.render(), "data:{\"ok\":true}\n\n");
  }
}

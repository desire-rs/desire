//! The response body type and its constructors.

use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;
use http_body_util::{BodyExt, Full, StreamBody};
use serde::Serialize;

use crate::Error;

/// The framework body: a boxed, streaming body of `Bytes`.
///
/// Responses are streamed, not buffered — see [`from_stream`].
pub type Body = http_body_util::combinators::UnsyncBoxBody<Bytes, Error>;

/// An empty body.
pub fn empty() -> Body {
  Full::new(Bytes::new())
    .map_err(|e: std::convert::Infallible| match e {})
    .boxed_unsync()
}

/// A body from raw bytes.
pub fn full(bytes: impl Into<Bytes>) -> Body {
  Full::new(bytes.into())
    .map_err(|e: std::convert::Infallible| match e {})
    .boxed_unsync()
}

/// A JSON body. On serialization failure the body becomes a generic
/// internal-error envelope (serializing ordinary data types cannot
/// fail; exotic map keys can).
pub fn json<T: Serialize + ?Sized>(value: &T) -> Body {
  match serde_json::to_vec(value) {
    Ok(bytes) => full(bytes),
    Err(e) => {
      tracing::error!(error = %e, "failed to serialize response body");
      full(r#"{"code":500,"msg":"internal server error"}"#)
    }
  }
}

/// A streaming body from any `Stream<Item = Result<B, E>>`.
///
/// ```ignore
/// use desire::body;
///
/// let stream = tokio_stream::iter((0..5).map(|i| Ok::<_, desire::Error>(Bytes::from(format!("{i}\n")))));
/// let res = (200, body::from_stream(stream)).into_response();
/// ```
pub fn from_stream<S, B, E>(stream: S) -> Body
where
  S: Stream<Item = Result<B, E>> + Send + 'static,
  B: Into<Bytes>,
  E: Into<Error>,
{
  UnsyncBoxBody::new(StreamBody::new(FrameStream {
    inner: Box::pin(stream),
  }))
}

use http_body::Frame;
use http_body_util::combinators::UnsyncBoxBody;

/// A stream of raw body chunks with the framework error type.
pub type BodyStream = Pin<Box<dyn Stream<Item = Result<Bytes, Error>> + Send>>;

/// Maps `Result<Bytes, BoxError>` chunks into the framework error type.
pub(crate) struct MapBoxError<S> {
  pub inner: Pin<Box<S>>,
}

impl<S> Stream for MapBoxError<S>
where
  S: Stream<Item = Result<Bytes, crate::types::BoxError>>,
{
  type Item = Result<Bytes, Error>;

  fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    let this = self.get_mut();
    match this.inner.as_mut().poll_next(cx) {
      Poll::Pending => Poll::Pending,
      Poll::Ready(None) => Poll::Ready(None),
      Poll::Ready(Some(Ok(bytes))) => Poll::Ready(Some(Ok(bytes))),
      Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(Error::Body(format!(
        "failed to read request body: {e}"
      ))))),
    }
  }
}

struct FrameStream<S> {
  inner: Pin<Box<S>>,
}

impl<S, B, E> Stream for FrameStream<S>
where
  S: Stream<Item = Result<B, E>>,
  B: Into<Bytes>,
  E: Into<Error>,
{
  type Item = Result<Frame<Bytes>, Error>;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    match self.inner.as_mut().poll_next(cx) {
      Poll::Pending => Poll::Pending,
      Poll::Ready(None) => Poll::Ready(None),
      Poll::Ready(Some(Ok(bytes))) => Poll::Ready(Some(Ok(Frame::data(bytes.into())))),
      Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e.into()))),
    }
  }
}

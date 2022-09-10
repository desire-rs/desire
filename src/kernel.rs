use crate::{Error, Request, Response};
use bytes::Bytes;
use http_body_util::Full;
use std::borrow::Cow;
use std::future::Future;
use std::sync::Arc;
#[async_trait::async_trait]
pub trait Endpoint: Send + Sync + 'static {
  async fn call(&self, req: Request) -> Response;
}

pub type DynEndpoint = dyn Endpoint;

#[async_trait::async_trait]
impl<F, Fut, Res> Endpoint for F
where
  F: Send + Sync + 'static + Fn(Request) -> Fut,
  Fut: Future<Output = Res> + Send + 'static,
  Res: IntoResponse + 'static,
{
  async fn call(&self, req: Request) -> Response {
    let fut = (self)(req);
    let res = fut.await;
    res.into_response()
  }
}

pub struct Next<'a> {
  pub endpoint: &'a DynEndpoint,
  pub middlewares: &'a [Arc<dyn Middleware>],
}

impl Next<'_> {
  pub async fn run(mut self, req: Request) -> Response {
    if let Some((cur, next)) = self.middlewares.split_first() {
      self.middlewares = next;
      cur.handle(req, self).await
    } else {
      self.endpoint.call(req).await
    }
  }
}

#[async_trait::async_trait]
pub trait Middleware: Send + Sync + 'static {
  async fn handle(&self, req: Request, next: Next<'_>) -> Response;
  fn name(&self) -> &str {
    std::any::type_name::<Self>()
  }
}

#[async_trait::async_trait]
impl<F, Fut, Res> Middleware for F
where
  F: Send + Sync + 'static + Fn(Request, Next) -> Fut,
  Fut: Future<Output = Res> + Send + 'static,
  Res: IntoResponse + 'static,
{
  async fn handle(&self, req: Request, next: Next<'_>) -> Response {
    let fut = (self)(req, next);
    let res = fut.await;
    res.into_response()
  }
}

pub trait IntoResponse {
  fn into_response(self) -> Response;
}

impl IntoResponse for Full<Bytes> {
  fn into_response(self) -> Response {
    hyper::http::Response::builder().body(self).unwrap().into()
  }
}

impl IntoResponse for &'static str {
  fn into_response(self) -> Response {
    Cow::Borrowed(self).into_response()
  }
}

impl IntoResponse for String {
  fn into_response(self) -> Response {
    Cow::<'static, str>::Owned(self).into_response()
  }
}
impl IntoResponse for Cow<'static, str> {
  fn into_response(self) -> Response {
    let mut res = Full::from(self).into_response();
    res.inner.headers_mut().insert(
      hyper::header::CONTENT_TYPE,
      hyper::header::HeaderValue::from_static(mime::TEXT_PLAIN_UTF_8.as_ref()),
    );
    res
  }
}

impl<T, E> IntoResponse for Result<T, E>
where
  T: IntoResponse,
  E: IntoResponse,
{
  fn into_response(self) -> Response {
    match self {
      Ok(value) => value.into_response(),
      Err(err) => err.into_response(),
    }
  }
}

impl IntoResponse for Error {
  fn into_response(self) -> Response {
    let val = self.to_string();
    Response::with_status(500, val)
  }
}

impl IntoResponse for Result<Response, Error> {
  fn into_response(self) -> Response {
    match self {
      Ok(value) => value,
      Err(err) => err.into_response(),
    }
  }
}

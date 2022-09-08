use crate::{HyperRequest, HyperResponse, Result};
use bytes::Bytes;
use http_body_util::Full;
use hyper::StatusCode;
use route_recognizer::Params;
use serde::Serialize;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

pub struct Context {
  pub inner: HyperRequest,
  pub params: Params,
  pub remote_addr: Option<SocketAddr>,
}

impl Context {
  pub fn new(req: HyperRequest, remote_addr: Option<SocketAddr>) -> Self {
    Context {
      inner: req,
      params: Params::new(),
      remote_addr,
    }
  }
  pub fn set_params(&mut self, params: Params) {
    self.params = params;
  }
  pub fn get_params(&self) -> &Params {
    &self.params
  }
  pub fn request(&self) -> &HyperRequest {
    &self.inner
  }
}

pub struct Response {
  pub inner: HyperResponse,
}

impl From<HyperResponse> for Response {
  fn from(res: HyperResponse) -> Self {
    Response { inner: res }
  }
}

impl Response {
  pub fn new(res: HyperResponse) -> Self {
    Response { inner: res }
  }
  pub fn response(&self) -> &HyperResponse {
    &self.inner
  }
  pub fn with_status(status: hyper::StatusCode, val: String) -> Self {
    hyper::http::Response::builder()
      .header(
        hyper::header::CONTENT_TYPE,
        mime::TEXT_PLAIN_UTF_8.to_string(),
      )
      .status(status)
      .body(Full::new(Bytes::from(val)))
      .unwrap()
      .into()
  }

  pub fn json<T: Serialize + Send>(val: T) -> Self {
    hyper::http::Response::builder()
      .header(
        hyper::header::CONTENT_TYPE,
        mime::APPLICATION_JSON.to_string(),
      )
      .status(StatusCode::OK)
      .body(Full::new(Bytes::from(serde_json::to_string(&val).unwrap())))
      .unwrap()
      .into()
  }
}
pub trait IntoResponse {
  fn into_response(&self) -> Result<Response>;
}

#[async_trait::async_trait]
pub trait Endpoint: Send + Sync + 'static {
  async fn call(&self, ctx: Context) -> Result;
}

pub type DynEndpoint = dyn Endpoint;

#[async_trait::async_trait]
impl<F, Fut, Res> Endpoint for F
where
  F: Send + Sync + 'static + Fn(Context) -> Fut,
  Fut: Future<Output = Result<Res>> + Send + Sync + 'static,
  Res: Into<Response> + 'static,
{
  async fn call(&self, ctx: Context) -> Result {
    let fut = (self)(ctx);
    let res = fut.await?;
    Ok(res.into())
  }
}

pub struct Next<'a> {
  pub endpoint: &'a DynEndpoint,
  pub middlewares: &'a Vec<Arc<dyn Middleware>>,
}

impl Next<'_> {
  pub async fn run(self, ctx: Context) -> Result {
    if let Some((cur, _next)) = self.middlewares.split_first() {
      cur.handle(ctx, self).await
    } else {
      self.endpoint.call(ctx).await
    }
  }
}

#[async_trait::async_trait]
pub trait Middleware: Send + Sync + 'static {
  async fn handle(&self, ctx: Context, next: Next<'_>) -> Result;
  fn name(&self) -> &str {
    std::any::type_name::<Self>()
  }
}

#[async_trait::async_trait]
impl<F> Middleware for F
where
  F: Send
    + Sync
    + 'static
    + for<'a> Fn(Context, Next<'a>) -> Pin<Box<dyn Future<Output = Result> + 'a + Send>>,
{
  async fn handle(&self, ctx: Context, next: Next<'_>) -> Result {
    (self)(ctx, next).await
  }
}

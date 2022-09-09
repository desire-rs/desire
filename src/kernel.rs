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
  pub request: Option<HyperRequest>,
  pub response: Option<HyperResponse>,
  pub params: Option<Params>,
  pub remote_addr: Option<SocketAddr>,
}

impl Context {
  pub fn new(req: HyperRequest) -> Self {
    Context {
      request: Some(req),
      response: None,
      params: None,
      remote_addr: None,
    }
  }
  pub fn set_params(&mut self, params: Params) {
    self.params = Some(params);
  }
  pub fn set_response(&mut self, response: HyperResponse) {
    self.response = Some(response);
  }
  pub fn set_request(&mut self, request: HyperRequest) {
    self.request = Some(request);
  }
  pub fn get_params(&self) -> &Option<Params> {
    &self.params
  }
  pub fn request(&self) -> &Option<HyperRequest> {
    &self.request
  }
  pub fn response(&self) -> &Option<HyperResponse> {
    &self.response
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

impl From<HyperResponse> for Context {
  fn from(response: HyperResponse) -> Self {
    Context {
      response: Some(response),
      request: None,
      params: None,
      remote_addr: None,
    }
  }
}

impl From<&str> for Context {
  fn from(msg: &str) -> Self {
    Context::with_status(StatusCode::from_u16(200).unwrap(), msg.to_string())
  }
}

impl From<String> for Context {
  fn from(msg: String) -> Self {
    Context::with_status(StatusCode::from_u16(200).unwrap(), msg)
  }
}

#[async_trait::async_trait]
pub trait IntoResponse: Send + Sync + 'static {
  async fn into_response(&self) -> Result;
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
  Res: Into<Context> + 'static,
{
  async fn call(&self, ctx: Context) -> Result {
    let fut = (self)(ctx);
    let res = fut.await?;
    Ok(res.into())
  }
}

pub struct Next<'a> {
  pub endpoint: &'a DynEndpoint,
  pub middlewares: &'a [Arc<dyn Middleware>],
}

impl Next<'_> {
  pub async fn run(mut self, ctx: Context) -> Result {
    if let Some((cur, next)) = self.middlewares.split_first() {
      self.middlewares = next;
      println!("run 1");
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

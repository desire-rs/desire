use crate::context::Context;
use crate::{Request, Response, Result};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

#[async_trait::async_trait]
pub trait IntoResponse: Send + Sync + 'static {
  async fn into_response(self) -> Result;
}

#[async_trait::async_trait]
pub trait Endpoint: Send + Sync + 'static {
  async fn call(&self, req: Request, ctx: Context) -> Result;
}

pub type DynEndpoint = dyn Endpoint;

#[async_trait::async_trait]
impl<F, Fut, Res> Endpoint for F
where
  F: Send + Sync + 'static + Fn(Request, Context) -> Fut,
  Fut: Future<Output = Result<Res>> + Send + Sync + 'static,
  Res: Into<Response> + 'static,
{
  async fn call(&self, req: Request, ctx: Context) -> Result {
    let fut = (self)(req, ctx);
    let res = fut.await?;
    Ok(res.into())
  }
}

pub struct Next<'a> {
  pub endpoint: &'a DynEndpoint,
  pub middlewares: &'a [Arc<dyn Middleware>],
}

impl Next<'_> {
  pub async fn run(mut self, req: Request, ctx: Context) -> Result {
    if let Some((cur, next)) = self.middlewares.split_first() {
      self.middlewares = next;
      cur.handle(req, ctx, self).await
    } else {
      self.endpoint.call(req, ctx).await
    }
  }
}

#[async_trait::async_trait]
pub trait Middleware: Send + Sync + 'static {
  async fn handle(&self, req: Request, ctx: Context, next: Next<'_>) -> Result;
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
    + for<'a> Fn(Request, Context, Next<'a>) -> Pin<Box<dyn Future<Output = Result> + 'a + Send + Sync>>,
{
  async fn handle(&self, req: Request, ctx: Context, next: Next<'_>) -> Result {
    (self)(req, ctx, next).await
  }
}

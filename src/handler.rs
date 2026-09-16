//! The `Handler` trait — anything async that takes a `Context` and
//! returns something that converts to a response.

use std::future::Future;
use std::sync::Arc;

use crate::context::Context;
use crate::into_response::IntoResponse;
use crate::types::{BoxFuture, Result};

/// A request handler. Plain `async fn`s implement it automatically:
///
/// ```ignore
/// async fn get_user(ctx: Context) -> Resp<User> { /* ... */ }
///
/// app.route("/users/{id}", get(get_user));
/// ```
///
/// Zero-argument closures work too, for trivial routes:
///
/// ```ignore
/// app.route("/health", get(|| async { Resp::ok(()) }));
/// ```
///
/// Handlers must be `Clone` (function items are; closures capturing
/// `Arc`s are). The clone happens per request inside [`Handler::call`],
/// which is what lets the future be `'static`.
pub trait Handler<T = ()>: Send + Sync + 'static {
  /// Run the handler. The returned future is 'static: implementors
  /// clone what they need into it.
  fn call(&self, ctx: Context) -> BoxFuture<'static, Result>;
}

impl<F, Fut, R> Handler<()> for F
where
  F: Fn() -> Fut + Clone + Send + Sync + 'static,
  Fut: Future<Output = R> + Send + 'static,
  R: IntoResponse,
{
  fn call(&self, _ctx: Context) -> BoxFuture<'static, Result> {
    let f = self.clone();
    Box::pin(async move { Ok((f)().await.into_response()) })
  }
}

/// Marker type parameter carrier for the one-argument form. Never named
/// by user code — inference fills it in.
pub struct WithContext;

impl<F, Fut, R> Handler<(WithContext,)> for F
where
  F: Fn(Context) -> Fut + Clone + Send + Sync + 'static,
  Fut: Future<Output = R> + Send + 'static,
  R: IntoResponse,
{
  fn call(&self, ctx: Context) -> BoxFuture<'static, Result> {
    let f = self.clone();
    Box::pin(async move { Ok((f)(ctx).await.into_response()) })
  }
}

/// The type-erased handler stored by the router: just a boxed closure.
pub(crate) type AnyHandler = Arc<dyn Fn(Context) -> BoxFuture<'static, Result> + Send + Sync>;

/// Conversion into the erased form. Parameterized by the same marker as
/// [`Handler`] so both arities can implement it without overlap.
#[doc(hidden)]
pub trait IntoAnyHandler<T> {
  fn into_any_handler(self) -> AnyHandler;
}

impl<H> IntoAnyHandler<()> for H
where
  H: Handler<()>,
{
  fn into_any_handler(self) -> AnyHandler {
    to_any(self)
  }
}

impl<H> IntoAnyHandler<(WithContext,)> for H
where
  H: Handler<(WithContext,)>,
{
  fn into_any_handler(self) -> AnyHandler {
    to_any(self)
  }
}

/// Erase any handler into the boxed-closure form.
pub(crate) fn to_any<H, T>(handler: H) -> AnyHandler
where
  H: Handler<T>,
{
  let shared = Arc::new(handler);
  Arc::new(move |ctx: Context| shared.call(ctx))
}

//! Routing: [`MethodRouter`] (per-path method → handler map) and
//! [`Router`] (a composable tree of routes with group middleware).

use std::collections::HashMap;
use std::sync::Arc;

use hyper::Method;

use crate::handler::{AnyHandler, Handler, to_any};

/// The handlers for one path, keyed by HTTP method.
///
/// Built fluently: `get(list).post(create)`. Registering a plain
/// handler (via [`IntoMethodRouter`]) implies `GET` + `HEAD`.
#[derive(Default)]
pub struct MethodRouter {
  pub(crate) handlers: HashMap<Method, AnyHandler>,
  pub(crate) middlewares: Vec<Arc<dyn crate::Middleware>>,
}

impl MethodRouter {
  /// An empty router.
  pub fn new() -> Self {
    MethodRouter {
      handlers: HashMap::new(),
      middlewares: Vec::new(),
    }
  }

  fn at<H, T>(mut self, method: Method, handler: H) -> Self
  where
    H: Handler<T>,
  {
    self.handlers.insert(method, to_any(handler));
    self
  }

  /// Handle `GET`.
  pub fn get<H, T>(self, handler: H) -> Self
  where
    H: Handler<T>,
  {
    self.at(Method::GET, handler)
  }

  /// Handle `POST`.
  pub fn post<H, T>(self, handler: H) -> Self
  where
    H: Handler<T>,
  {
    self.at(Method::POST, handler)
  }

  /// Handle `PUT`.
  pub fn put<H, T>(self, handler: H) -> Self
  where
    H: Handler<T>,
  {
    self.at(Method::PUT, handler)
  }

  /// Handle `PATCH`.
  pub fn patch<H, T>(self, handler: H) -> Self
  where
    H: Handler<T>,
  {
    self.at(Method::PATCH, handler)
  }

  /// Handle `DELETE`.
  pub fn delete<H, T>(self, handler: H) -> Self
  where
    H: Handler<T>,
  {
    self.at(Method::DELETE, handler)
  }

  /// Handle `HEAD` (otherwise derived from `GET`).
  pub fn head<H, T>(self, handler: H) -> Self
  where
    H: Handler<T>,
  {
    self.at(Method::HEAD, handler)
  }

  /// Handle `OPTIONS`.
  pub fn options<H, T>(self, handler: H) -> Self
  where
    H: Handler<T>,
  {
    self.at(Method::OPTIONS, handler)
  }

  /// Handle `TRACE`.
  pub fn trace<H, T>(self, handler: H) -> Self
  where
    H: Handler<T>,
  {
    self.at(Method::TRACE, handler)
  }

  /// Handle `CONNECT`.
  pub fn connect<H, T>(self, handler: H) -> Self
  where
    H: Handler<T>,
  {
    self.at(Method::CONNECT, handler)
  }
}

/// A single handler registered with [`App::route`](crate::App::route)
/// becomes a GET (+HEAD) route. A [`MethodRouter`] passes through
/// unchanged.
///
/// The type parameter is an inference marker; user code never names it.
pub trait IntoMethodRouter<T = ()> {
  /// Convert into a [`MethodRouter`].
  fn into_method_router(self) -> MethodRouter;
}

/// Marker distinguishing the plain-handler form.
pub struct HandlerRoute;

impl IntoMethodRouter<()> for MethodRouter {
  /// Convert into a [`MethodRouter`].
  fn into_method_router(self) -> MethodRouter {
    self
  }
}

impl<H, U> IntoMethodRouter<(HandlerRoute, U)> for H
where
  H: Handler<U>,
{
  fn into_method_router(self) -> MethodRouter {
    MethodRouter::new().get(self)
  }
}

/// Method convenience constructors: `get(handler)` etc.
mod method_fns {
  use super::*;

  macro_rules! method_fn {
    ($name:ident, $method:expr) => {
      #[doc = concat!("A `", stringify!($method), "` route builder.")]
      pub fn $name<H, T>(handler: H) -> MethodRouter
      where
        H: Handler<T>,
      {
        MethodRouter::new().at($method, handler)
      }
    };
  }

  method_fn!(get, Method::GET);
  method_fn!(post, Method::POST);
  method_fn!(put, Method::PUT);
  method_fn!(patch, Method::PATCH);
  method_fn!(delete, Method::DELETE);
  method_fn!(head, Method::HEAD);
  method_fn!(options, Method::OPTIONS);
  method_fn!(trace, Method::TRACE);
  method_fn!(connect, Method::CONNECT);
}

pub use method_fns::{connect, delete, get, head, options, patch, post, put, trace};

/// A composable set of routes with group middleware. Nest it into an
/// [`App`](crate::App) with [`App::nest`](crate::App::nest), or combine
/// routers with [`App::merge`](crate::App::merge).
///
/// ```ignore
/// let api = Router::new()
///     .with(auth)                       // group middleware
///     .route("/users", get(list_users))
///     .route("/users/{id}", get(get_user));
///
/// let app = App::new().nest("/api", api);
/// ```
#[derive(Default)]
pub struct Router {
  pub(crate) routes: Vec<(String, MethodRouter)>,
  pub(crate) middlewares: Vec<Arc<dyn crate::Middleware>>,
}

impl Router {
  /// An empty router.
  pub fn new() -> Self {
    Router::default()
  }

  /// Register a route: a [`MethodRouter`] or a plain handler (which
  /// implies GET + HEAD). Conflicting paths fail fast at startup.
  pub fn route<P, M, T>(mut self, path: P, method_router: M) -> Self
  where
    P: Into<String>,
    M: IntoMethodRouter<T>,
  {
    let path = path.into();
    assert!(
      path.starts_with('/'),
      "route path must start with `/`: {path}"
    );
    self.routes.push((path, method_router.into_method_router()));
    self
  }

  /// Group middleware, applied to every route of this router when it is
  /// attached to the app. Inner groups run inside outer groups.
  pub fn with<M: crate::Middleware>(mut self, middleware: M) -> Self {
    self.middlewares.push(Arc::new(middleware));
    self
  }

  /// Mount `router` under `prefix`. The sub-router's middleware keeps
  /// applying to its routes.
  pub fn nest(mut self, prefix: impl Into<String>, router: Router) -> Self {
    let prefix = prefix.into();
    assert!(
      prefix.starts_with('/') && !prefix.ends_with('/'),
      "nest prefix must start with `/` and not end with `/`: {prefix}"
    );
    let Router {
      routes,
      middlewares,
    } = router;
    for (path, mut mr) in routes {
      mr.middlewares = middlewares
        .iter()
        .cloned()
        .chain(mr.middlewares.iter().cloned())
        .collect();
      self.routes.push((format!("{prefix}{path}"), mr));
    }
    self
  }

  /// Add all routes of `router` to this one. Group middleware of
  /// `router` travels with its routes. Conflicting paths fail fast at
  /// startup.
  pub fn merge(mut self, router: Router) -> Self {
    let Router {
      routes,
      middlewares,
    } = router;
    for (path, mut mr) in routes {
      mr.middlewares = middlewares
        .iter()
        .cloned()
        .chain(mr.middlewares.iter().cloned())
        .collect();
      self.routes.push((path, mr));
    }
    self
  }
}

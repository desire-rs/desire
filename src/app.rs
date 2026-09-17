//! `App` — routes, shared state, global middleware, and the dispatch
//! pipeline that ties everything together.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use http_body_util::BodyExt;
use hyper::Method;
use hyper::header::{ALLOW, HeaderValue};

use crate::context::{Context, percent_decode};
use crate::handler::{AnyHandler, Handler, to_any};
use crate::middleware::Next;
use crate::resp::Resp;
use crate::router::Router;
use crate::state::{self, StateMap};
use crate::{Error, Result, body, into_response::IntoResponse};

/// The application: register routes, state and middleware, then run.
///
/// ```ignore
/// let app = App::new()
///     .state(db)
///     .with(logger())
///     .route("/users/{id}", get(get_user));
/// app.run("0.0.0.0:3000").await?;
/// ```
pub struct App {
  router: Router,
  state: StateMap,
  global: Vec<Arc<dyn crate::Middleware>>,
  fallback: Option<AnyHandler>,
  max_body_size: usize,
}

impl Default for App {
  fn default() -> Self {
    App::new()
  }
}

impl App {
  /// A fresh application with no routes, state, or middleware.
  pub fn new() -> Self {
    App {
      router: Router::new(),
      state: StateMap::new(),
      global: Vec::new(),
      fallback: None,
      max_body_size: 2 * 1024 * 1024,
    }
  }

  /// Register shared application state, readable in handlers via
  /// [`Context::state`]. Registering the same type twice replaces it.
  pub fn state<T: Send + Sync + 'static>(mut self, value: T) -> Self {
    state::insert(&mut self.state, value);
    self
  }

  /// Global middleware — wraps every request, including the fallback.
  /// Order: first registered runs outermost.
  pub fn with<M: crate::Middleware>(mut self, middleware: M) -> Self {
    self.global.push(Arc::new(middleware));
    self
  }

  /// Register a route: a [`MethodRouter`](crate::MethodRouter) (`get(h).post(h2)`) or a
  /// plain handler (implies GET + HEAD).
  pub fn route<P, M, T>(mut self, path: P, method_router: M) -> Self
  where
    P: Into<String>,
    M: crate::router::IntoMethodRouter<T>,
  {
    self.router = self.router.route(path, method_router);
    self
  }

  /// Mount a [`Router`] under a path prefix.
  pub fn nest(mut self, prefix: impl Into<String>, router: Router) -> Self {
    self.router = self.router.nest(prefix, router);
    self
  }

  /// Merge another router's routes into the app (same path prefix).
  pub fn merge(mut self, router: Router) -> Self {
    self.router = self.router.merge(router);
    self
  }

  /// Custom 404 handler (default: a 404 envelope).
  pub fn fallback<H, T>(mut self, handler: H) -> Self
  where
    H: crate::handler::IntoAnyHandler<T>,
  {
    self.fallback = Some(handler.into_any_handler());
    self
  }

  /// The maximum accepted request body size, in bytes. Default 2 MiB;
  /// larger bodies fail with 413. Per-route overrides via
  /// [`body_limit`](crate::middleware::body_limit).
  pub fn max_body_size(mut self, bytes: usize) -> Self {
    self.max_body_size = bytes;
    self
  }

  /// Serve a generated OpenAPI document at `/openapi.json` and a
  /// Swagger UI at `/docs` (requires the `openapi` feature).
  #[cfg(feature = "openapi")]
  pub fn openapi(mut self, doc: crate::openapi::OpenApi) -> Self {
    use crate::router::{IntoMethodRouter as _, get};
    use crate::{Html, Response};

    let json = doc.to_json();
    let json_route = move || {
      let json = json.clone();
      async move { Response::json(&json) }
    };
    self.router = self
      .router
      .route("/openapi.json", json_route.into_method_router())
      .route(
        "/docs",
        get(|| async { Html(crate::openapi::SWAGGER_HTML) }),
      );
    self
  }

  /// Bind and serve, with Ctrl+C graceful shutdown by default.
  pub async fn run(self, addr: &str) -> Result<()> {
    crate::server::Server::new(self)
      .bind(addr)?
      .graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
      })
      .run()
      .await
  }

  pub(crate) fn build(self) -> BuiltApp {
    let App {
      router,
      state,
      global,
      fallback,
      max_body_size,
    } = self;
    let mut matcher = matchit::Router::new();
    for (path, mr) in router.routes {
      let allow = allow_header(&mr.handlers);
      // Bake the full per-route chain (global ++ group) once, so the
      // hot path never rebuilds it.
      let middlewares: Vec<Arc<dyn crate::Middleware>> = global
        .iter()
        .cloned()
        .chain(mr.middlewares.iter().cloned())
        .collect();
      let flat = FlatRoute {
        methods: mr.handlers,
        middlewares: middlewares.into(),
        allow,
      };
      if let Err(e) = matcher.insert(path.clone(), flat) {
        panic!("route conflict at `{path}`: {e}");
      }
    }
    BuiltApp {
      matcher,
      state: Arc::new(state),
      global: global.into(),
      fallback: fallback.unwrap_or_else(|| to_any(default_fallback)),
      max_body_size,
    }
  }
}

async fn default_fallback(_ctx: Context) -> Resp<()> {
  Resp::err(404, "not found").with_status(hyper::StatusCode::NOT_FOUND)
}

/// A flattened route, the value stored in the matcher.
pub(crate) struct FlatRoute {
  pub methods: HashMap<Method, AnyHandler>,
  /// The complete chain for this route: global middleware ++ group
  /// middleware, baked at build time.
  pub middlewares: Arc<[Arc<dyn crate::Middleware>]>,
  /// Precomputed `Allow` header value for 405 responses.
  pub allow: Arc<str>,
}

/// The compiled, immutable app — shared across connections.
pub(crate) struct BuiltApp {
  pub matcher: matchit::Router<FlatRoute>,
  pub state: Arc<StateMap>,
  pub global: Arc<[Arc<dyn crate::Middleware>]>,
  pub fallback: AnyHandler,
  pub max_body_size: usize,
}

/// Route one request through middleware and a handler. Used by both
/// the server and the test client. Generic over the request body so
/// tests can drive it without a socket.
pub(crate) async fn dispatch<B>(
  app: &BuiltApp,
  req: hyper::Request<B>,
  remote_addr: Option<SocketAddr>,
) -> crate::types::HyperResponse
where
  B: hyper::body::Body<Data = Bytes> + Send + 'static,
  B::Error: Into<crate::types::BoxError>,
{
  let (mut parts, incoming) = req.into_parts();
  let inner = incoming
    .map_err(|e| -> crate::types::BoxError { e.into() })
    .boxed_unsync();
  // hyper parks the upgrade handle in request extensions.
  let on_upgrade = parts.extensions.remove::<hyper::upgrade::OnUpgrade>();
  let cookies_out: Arc<Mutex<Vec<cookie::Cookie<'static>>>> = Arc::default();
  let method = parts.method;
  // Borrow the path straight out of the request parts — no copy.
  let matched = app.matcher.at(parts.uri.path());
  match matched {
    Ok(m) => {
      let route = &m.value;
      let effective = if method == Method::HEAD && !route.methods.contains_key(&Method::HEAD) {
        Method::GET
      } else {
        method.clone()
      };

      // A path hit with an unregistered method still flows through its
      // middleware (so CORS preflight and logging see it); the terminal
      // handler renders the precomputed 405 + Allow response.
      let (handler, middlewares): (AnyHandler, Arc<[Arc<dyn crate::Middleware>]>) =
        match route.methods.get(&effective) {
          Some(handler) => (Arc::clone(handler), Arc::clone(&route.middlewares)),
          None => (
            to_any(not_allowed(Arc::clone(&route.allow))),
            Arc::clone(&route.middlewares),
          ),
        };

      let mut params = HashMap::new();
      for (name, value) in m.params.iter() {
        params.insert(name.to_owned(), percent_decode(value));
      }

      let ctx = Context::new(
        method.clone(),
        parts.uri,
        parts.headers,
        params,
        inner,
        app.max_body_size,
        Arc::clone(&app.state),
        remote_addr,
        on_upgrade,
        Arc::clone(&cookies_out),
      );

      let next = Next {
        middlewares,
        handler,
        index: 0,
      };

      let response = match next.run(ctx).await {
        Ok(res) => res,
        Err(err) => err.into_response(),
      };

      let mut hyper_res = response.into_hyper();
      apply_set_cookies(&mut hyper_res, &cookies_out);
      if method == Method::HEAD {
        *hyper_res.body_mut() = body::empty();
      }
      hyper_res
    }
    Err(_) => {
      // NotFound (or a malformed path) — run the fallback through the
      // global middleware.
      let ctx = Context::new(
        method,
        parts.uri,
        parts.headers,
        HashMap::new(),
        inner,
        app.max_body_size,
        Arc::clone(&app.state),
        remote_addr,
        on_upgrade,
        Arc::clone(&cookies_out),
      );
      let next = Next {
        middlewares: Arc::clone(&app.global),
        handler: Arc::clone(&app.fallback),
        index: 0,
      };
      let response = match next.run(ctx).await {
        Ok(res) => res,
        Err(err) => err.into_response(),
      };
      let mut hyper_res = response.into_hyper();
      apply_set_cookies(&mut hyper_res, &cookies_out);
      hyper_res
    }
  }
}

/// Drain the request's queued `Set-Cookie` values onto the response.
fn apply_set_cookies(
  res: &mut crate::types::HyperResponse,
  cookies_out: &Mutex<Vec<cookie::Cookie<'static>>>,
) {
  let jar = cookies_out.lock().expect("cookie lock poisoned");
  for cookie in jar.iter() {
    if let Ok(value) = hyper::header::HeaderValue::from_str(&cookie.to_string()) {
      res.headers_mut().append(hyper::header::SET_COOKIE, value);
    }
  }
}

/// The `Allow` header value for a route's registered methods.
fn allow_header(methods: &HashMap<Method, AnyHandler>) -> Arc<str> {
  let mut allow: Vec<&str> = methods.keys().map(Method::as_str).collect();
  if !allow.contains(&"HEAD") && allow.contains(&"GET") {
    allow.push("HEAD");
  }
  allow.sort_unstable();
  allow.dedup();
  Arc::from(allow.join(", ").as_str())
}

/// The 405 response as a handler, so it flows through middleware.
fn not_allowed(allow: Arc<str>) -> impl Handler<()> {
  move || {
    let allow = Arc::clone(&allow);
    async move {
      let mut res = Error::MethodNotAllowed.to_resp().into_response();
      if let Ok(value) = HeaderValue::from_str(&allow) {
        res.headers_mut().insert(ALLOW, value);
      }
      res
    }
  }
}

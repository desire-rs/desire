//! # desire
//!
//! A minimal and ergonomic Rust web framework on hyper.
//!
//! desire bets on **fewer concepts and friendlier diagnostics**: handlers are
//! plain `async fn`s taking a [`Context`], every response — success or
//! failure — shares one JSON envelope, and errors tell you exactly which
//! layer, field, and type went wrong.
//!
//! ## Quick start
//!
//! ```no_run
//! use desire::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let app = App::new().route("/hello/{name}", get(hello));
//!     app.run("0.0.0.0:3000").await
//! }
//!
//! async fn hello(ctx: Context) -> Result<Resp<String>> {
//!     let name = ctx.param_raw("name")?;
//!     Ok(Resp::ok(format!("hello, {name}!")))
//! }
//! ```
//!
//! ```text
//! $ curl localhost:3000/hello/desire
//! {"code":0,"msg":"ok","data":"hello, desire!"}
//! ```
//!
//! ## The six concepts
//!
//! That is all of them:
//!
//! | Concept | One line |
//! |---------|----------|
//! | [`App`] | routes + state + middleware + the server entry point |
//! | [`Context`] | everything about the current request |
//! | [`Handler`] | any `async fn(Context) -> impl IntoResponse` |
//! | [`Middleware`] / [`Next`] | the onion around handlers |
//! | [`Router`] / [`MethodRouter`] | composable route trees |
//! | [`Resp`] | the unified `{code, msg, data}` envelope |
//!
//! # Guide
//!
//! ## Routing
//!
//! Route paths use matchit syntax: `{param}` segments and `{*wildcard}`
//! tails. A [`MethodRouter`] chains handlers per HTTP method; a plain
//! handler registered directly implies GET + HEAD.
//!
//! ```no_run
//! use desire::prelude::*;
//!
//! # async fn list(ctx: Context) -> Resp<()> { Resp::ok(()) }
//! # async fn create(ctx: Context) -> Resp<()> { Resp::ok(()) }
//! # async fn get_one(ctx: Context) -> Resp<()> { Resp::ok(()) }
//! # async fn health() -> Resp<()> { Resp::ok(()) }
//! # async fn not_found(ctx: Context) -> Resp<()> { Resp::ok(()) }
//! let app = App::new()
//!     .route("/users", get(list).post(create))
//!     .route("/users/{id}", get(get_one))
//!     .route("/health", health)        // plain handler = GET + HEAD
//!     .fallback(not_found);            // default: a 404 envelope
//!
//! // compose: groups with their own middleware, mounted under a prefix
//! # let auth = |ctx: Context, next: Next| async move { next.run(ctx).await };
//! # let extra = Router::new();
//! let api = Router::new().with(auth).route("/users", get(list));
//! let app = App::new().nest("/api", api).merge(extra);
//! ```
//!
//! Wrong methods return `405` with an `Allow` header (through the global
//! middleware, so CORS preflight works). `HEAD` is derived from `GET`.
//! Conflicting paths panic at startup — fail fast, like actix.
//!
//! ## Reading requests
//!
//! Extraction methods live on [`Context`], take `&self`, and share one
//! cached body — `json`, `form`, and `body_bytes` can be mixed freely.
//!
//! ```ignore
//! let id: i64 = ctx.param("id")?;        // path param, FromStr parsed
//! let args: Args = ctx.params()?;        // whole struct from path params
//! let q: ListQuery = ctx.query()?;       // query string → struct
//! let input: CreateUser = ctx.json().await?;   // JSON body
//! let form: LoginForm = ctx.form().await?;     // urlencoded body
//! let bytes = ctx.body_bytes().await?;   // raw body (cached, idempotent)
//! let stream = ctx.body_stream();        // streaming (consumes the body)
//! let token = ctx.header("authorization");
//! let session = ctx.cookie("session");
//! let db = ctx.state::<Db>()?;           // shared state, borrowed
//! ```
//!
//! Failures carry the layer, field, and expected type:
//! `invalid param "id", expected "i64": invalid digit found in string`,
//! `invalid json body: missing field "email" at line 1 column 20`.
//!
//! ## Writing responses
//!
//! [`IntoResponse`] is implemented for [`Resp<T>`](crate::Resp), strings,
//! bytes, [`Json<T>`], [`Html<T>`], `serde_json::Value`,
//! `(StatusCode, body)` tuples, and `Result<T, E: Into<Error>>`:
//!
//! ```ignore
//! Ok(Resp::ok(user))                        // {"code":0,"msg":"ok","data":…}
//! Ok(Resp::created(user))                   // HTTP 201
//! Ok(Resp::page(users, 1, 20, total))       // + "page":{"index","size","total"}
//! Ok(Resp::err(40001, "余额不足"))           // business error, HTTP 200
//! Err(Error::not_found("user"))             // rendered as the same envelope
//! ```
//!
//! Streaming bodies work with any chunk stream — this is also how
//! [SSE](crate::sse) is built:
//!
//! ```ignore
//! async fn file(ctx: Context) -> Body {
//!     body::from_stream(read_chunks("big.bin"))
//! }
//! ```
//!
//! ## Shared state
//!
//! Register once at startup; borrow per request with zero clones:
//!
//! ```no_run
//! use desire::prelude::*;
//!
//! #[derive(Clone)]
//! struct Db; // pools and handles are cheap to share
//!
//! # async fn get_user(ctx: Context) -> Result<Resp<()>> { Ok(Resp::ok(())) }
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let app = App::new()
//!         .state(Db)
//!         .route("/users/{id}", get(get_user));
//!     app.run("0.0.0.0:3000").await
//! }
//! ```
//!
//! ## Middleware
//!
//! Middleware is an onion, and plain async fns are middleware. Handlers
//! and middleware share one mental model:
//!
//! ```no_run
//! use desire::prelude::*;
//!
//! struct CurrentUser(String);
//!
//! async fn auth(mut ctx: Context, next: Next) -> Result {
//!     let token = ctx.header("authorization").ok_or_else(Error::unauthorized)?;
//!     ctx.insert(CurrentUser(token[7..].to_owned()));
//!     let mut res = next.run(ctx).await?;
//!     // post-processing: the response is fully built here
//!     res.headers_mut().insert("x-frame-options", "DENY".parse().unwrap());
//!     Ok(res)
//! }
//!
//! # async fn me(ctx: Context) -> Resp<String> { Resp::ok(String::new()) }
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let app = App::new()
//!         .with(auth)                                  // global
//!         .with(desire::middleware::logger())          // built-in
//!         .route("/me", get(me));
//!     app.run("0.0.0.0:3000").await
//! }
//! ```
//!
//! Built-ins: [`middleware::logger`](middleware::logger()),
//! [`middleware::cors`](middleware::cors()),
//! [`middleware::timeout`](middleware::timeout()),
//! [`middleware::body_limit`](middleware::body_limit()), and
//! [`middleware::gzip`](middleware::gzip()) (feature `gzip`).
//!
//! ## Error handling
//!
//! One error type renders every failure as the unified envelope.
//! `Error::internal` and `Error::Io` are sanitized — the client sees
//! `internal server error`, the details go to `tracing`:
//!
//! ```ignore
//! Err(Error::unauthorized())?;
//! Err(Error::business(40001, "余额不足"))?;    // custom code, HTTP 200
//! Err(Error::internal(db_failure))?;          // logged, sanitized 500
//! ```
//!
//! Wire your own error type with `From<MyError> for desire::Error` and
//! `?` works everywhere — see the cookbook below.
//!
//! ## Static files
//!
//! [`ServeDir`] is hardened: percent-decoded traversal, backslash and NUL
//! segments, dotfiles, and symlink escapes are all rejected (covered by
//! tests); conditional requests and single-range requests are supported.
//!
//! ```no_run
//! use desire::prelude::*;
//!
//! # fn static_files() -> Result<()> {
//! let app = App::new()
//!     .route("/static/{*path}", ServeDir::new("assets").cache_control("public, max-age=86400"))
//!     .route("/favicon.ico", ServeFile::new("assets/favicon.ico"));
//! # let _ = app;
//! # Ok(())
//! # }
//! # fn main() { let _ = static_files(); }
//! ```
//!
//! ## Server-sent events
//!
//! [`Event`](crate::sse::Event) builds frames;
//! [`Sse`](crate::sse::Sse) turns any stream into a
//! `text/event-stream` response; `keep_alive` emits comment frames while
//! the stream is idle so proxies keep the connection open.
//!
//! ```no_run
//! use desire::prelude::*;
//! use futures_core::Stream;
//! use std::{convert::Infallible, time::Duration};
//! use tokio_stream::wrappers::IntervalStream;
//! use tokio_stream::StreamExt as _;
//!
//! async fn ticks(_ctx: Context) -> desire::sse::KeepAliveSse<impl Stream<Item = Result<Event, Infallible>> + Send> {
//!     let stream = IntervalStream::new(tokio::time::interval(Duration::from_secs(1)))
//!         .map(|_| Ok(Event::new().event("tick").data("…")));
//!     Sse::new(stream).keep_alive(Duration::from_secs(15))
//! }
//!
//! # fn main() {}
//! ```
//!
//! ## WebSockets
//!
//! Feature `ws` (tokio-tungstenite under the hood). Check the feature
//! gate at compile time in user code the same way:
//!
//! ```ignore
//! async fn echo(ctx: Context) -> Result {
//!     let ws = ctx.websocket()?;
//!     Ok(ws.on_upgrade(|mut socket| async move {
//!         while let Some(Ok(msg)) = socket.next().await {
//!             if (msg.is_text() || msg.is_binary()) && socket.send(msg).await.is_err() {
//!                 break;
//!             }
//!         }
//!     }))
//! }
//! ```
//!
//! ## Testing
//!
//! [`TestClient`](crate::test::TestClient) drives the real
//! middleware + routing + handler pipeline in-memory — no sockets:
//!
//! ```no_run
//! use desire::prelude::*;
//!
//! # fn app() -> App { App::new() }
//! #[tokio::test]
//! async fn get_user_ok() {
//!     let tc = TestClient::new(app());
//!     let res = tc.get("/users/1").send().await;
//!     res.assert_status_ok();
//! }
//! # fn main() {}
//! ```
//!
//! ## Deployment
//!
//! ```no_run
//! # async fn deploy() -> desire::Result<()> {
//! # let app = desire::App::new();
//! desire::Server::new(app)
//!     .bind("0.0.0.0:443")?
//!     .tls(desire::TlsConfig::builder()
//!         .cert_file("cert.pem")?
//!         .key_file("key.pem")?
//!         .build()?)
//!     .concurrency_limit(1024)
//!     .graceful_shutdown(async { let _ = tokio::signal::ctrl_c().await; })
//!     .run()
//!     .await
//! # }
//! # fn main() {}
//! ```
//!
//! HTTP/1.1 and HTTP/2 share one port (hyper-util auto; ALPN with TLS).
//! The request body is capped at 2 MiB by default
//! ([`App::max_body_size`](App::max_body_size)).
//!
//! # Cookbook
//!
//! ## Custom application errors
//!
//! ```no_run
//! use desire::prelude::*;
//!
//! enum AppError {
//!     NotFound,
//!     PaymentRequired(String),
//! }
//!
//! impl From<AppError> for Error {
//!     fn from(e: AppError) -> Self {
//!         match e {
//!             AppError::NotFound => Error::not_found("resource"),
//!             AppError::PaymentRequired(why) => Error::business(40200, why),
//!         }
//!     }
//! }
//!
//! # struct Db;
//! # impl Db { async fn find(&self, _id: i64) -> Result<(), std::io::Error> { Ok(()) } }
//! # async fn get_user(ctx: Context) -> Result<Resp<()>> {
//! let db = ctx.state::<Db>()?;
//! db.find(1).await?;               // io::Error → sanitized 500
//! # /*
//! Err(AppError::NotFound)?;        // → {"code":404,…}
//! Ok(Resp::ok(()))
//! # */
//! # Err(Error::not_found("demo"))
//! # }
//! # fn main() {}
//! ```
//!
//! ## Serving a single-page app
//!
//! ```no_run
//! # use desire::prelude::*;
//! # fn spa() -> App {
//! App::new().route(
//!     "/{*path}",
//!     ServeDir::new("dist").fallback_file("dist/index.html"),
//! )
//! # }
//! # fn main() { let _ = spa(); }
//! ```
//!
//! Unknown paths serve `index.html` with 200 so the client router takes
//! over; real assets still win.
//!
//! ## OpenAPI documentation
//!
//! Feature `openapi`; payload types need
//! `#[derive(schemars::JsonSchema)]`.
//!
//! ```no_run
//! # use desire::prelude::*;
//! # use desire::openapi::{OpenApi, PathDoc};
//! # #[derive(schemars::JsonSchema)] struct User;
//! # fn docs() -> App {
//! let docs = OpenApi::new("User API", "1.0.0").path(
//!     PathDoc::get("/users/{id}")
//!         .summary("Fetch one user")
//!         .path_param("id", "numeric user id")
//!         .resp::<User>(200, "the user"),
//! );
//! App::new().openapi(docs)   // /openapi.json + Swagger UI /docs
//! # }
//! # fn main() { let _ = docs(); }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod app;
pub mod body;
pub mod context;
pub mod error;
pub mod form;
pub mod fs;
pub mod handler;
pub mod into_response;
pub mod middleware;
#[cfg(feature = "openapi")]
#[cfg_attr(docsrs, doc(cfg(feature = "openapi")))]
pub mod openapi;
pub mod resp;
pub mod response;
pub mod router;
pub mod server;
pub mod sse;
pub mod state;
#[cfg(feature = "tls")]
#[cfg_attr(docsrs, doc(cfg(feature = "tls")))]
pub mod tls;
pub mod types;
#[cfg(feature = "ws")]
#[cfg_attr(docsrs, doc(cfg(feature = "ws")))]
pub mod ws;

/// In-memory test client: `TestClient::new(app).get("/x").send()`.
pub mod test;

pub use app::App;
pub use body::Body;
pub use bytes::Bytes;
pub use context::Context;
pub use cookie;
pub use error::Error;
pub use fs::{ServeDir, ServeFile};
pub use handler::{Handler, WithContext};
pub use into_response::{Html, IntoResponse, Json};
pub use middleware::{Middleware, Next};
pub use resp::{Page, Resp};
pub use response::Response;
pub use router::{MethodRouter, Router};
pub use server::Server;
pub use types::{BoxError, BoxFuture, Result};

#[cfg(feature = "tls")]
pub use tls::TlsConfig;
#[cfg(feature = "ws")]
pub use ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};

/// Re-exported HTTP primitives (`http::Method`, `http::StatusCode`, ...).
pub use hyper::http;

/// The batteries-included import: everything a handler needs.
pub mod prelude {
  pub use crate::Bytes;
  #[cfg(feature = "tls")]
  pub use crate::TlsConfig;
  pub use crate::body;
  pub use crate::body::Body;
  pub use crate::context::Context;
  pub use crate::error::Error;
  pub use crate::fs::{ServeDir, ServeFile};
  pub use crate::handler::Handler;
  pub use crate::into_response::{Html, IntoResponse, Json};
  pub use crate::middleware::{CorsConfig, Middleware, Next, body_limit, cors, logger, timeout};
  pub use crate::resp::{Page, Resp};
  pub use crate::response::Response;
  pub use crate::router::{
    MethodRouter, Router, connect, delete, get, head, options, patch, post, put, trace,
  };
  pub use crate::sse::{Event, Sse};
  pub use crate::test::TestClient;
  pub use crate::{App, Result, Server};
  #[cfg(feature = "ws")]
  pub use crate::{WebSocket, WebSocketUpgrade};
  pub use hyper::http::Method;
  pub use hyper::http::StatusCode;
}

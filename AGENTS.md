# AGENTS.md

> Guidelines for agentic coding in the desire-rs repository.

## Project Overview

**desire** is a minimal and ergonomic Rust web framework built on hyper.
- Repository: https://github.com/desire-rs/desire
- Edition: 2024 (NOT 2021), MSRV 1.85
- License: Apache-2.0
- Core concepts (that is all of them): `App`, `Context`, `Handler`,
  `Middleware`/`Next`, `Router`/`MethodRouter`, `Resp<T>`.

## Build / Lint / Test Commands

```bash
cargo build                       # build
cargo test                        # all tests (unit + integration + doctests)
cargo test get_user               # tests matching a name
cargo test --test integration     # one integration file
cargo clippy --all-targets --all-features   # lint (must be clean)
cargo fmt                         # format (rustfmt.toml: edition 2024, 2 spaces)
cargo fmt -- --check              # verify formatting
cargo doc --no-deps               # docs (must be warning-free)
cargo run --example hello         # run an example
cargo run --example tls --features tls      # TLS example
```

## Code Style Guidelines

### Formatting
- Edition 2024, `tab_spaces = 2` (see `rustfmt.toml`)
- Run `cargo fmt` before committing; CI runs fmt/clippy/test/doc with `-D warnings`

### Naming
Standard Rust: modules `snake_case`, types/traits `PascalCase`, fns `snake_case`,
constants `SCREAMING_SNAKE`.

### Architecture (src/)

```
app.rs            App: routes + state + global middleware + dispatch pipeline
context.rs        Context: request view + extraction + per-request typed map
handler.rs        Handler trait (plain async fns impl it) + erased dispatch
middleware.rs     Middleware trait, Next runner, builtins (logger/cors/timeout/body_limit)
router.rs         Router/MethodRouter/IntoMethodRouter + method fns (get/post/…)
resp.rs           Resp<T> unified envelope {"code","msg","data","page"}
response.rs       concrete Response type
into_response.rs  IntoResponse trait + impls (Json<T>, Html<T>, tuples, Result)
error.rs          Error enum → renders as the Resp envelope (Io/Json/Query/Param/…)
body.rs           Body type + constructors (empty/full/json/from_stream)
server.rs         hyper-util auto server (H1+H2), graceful shutdown, concurrency
sse.rs            Server-Sent Events (Event builder + Sse response)
openapi.rs        OpenAPI doc builder + Swagger UI (feature = "openapi", schemars)
ws.rs             WebSocket upgrade (feature = "ws", tokio-tungstenite)
tls.rs            rustls config (feature = "tls")
fs.rs             ServeDir/ServeFile (traversal-safe, ETag/304, index.html)
test.rs           TestClient: in-memory requests through the real pipeline
types.rs          BoxFuture/Result/BoxError/HyperRequest/HyperResponse aliases
state.rs          StateMap (App state) + TypeMap (per-request data)
```

### Key design facts (do not fight these)

- **Handlers take owned `Context`**: `async fn h(ctx: Context) -> impl IntoResponse`.
  Borrowed (`&Context`) signatures cannot work — the HRTB makes dyn dispatch
  impossible without a proc macro. Handlers must be `Clone` (fn items are).
- **Extraction methods take `&self`** — the body is read through interior
  mutability and cached, so `json()`/`form()`/`body_bytes()` can be mixed.
- **Errors are one type**: `desire::Result<T = Response, E = Error>`. Any error
  renders as the unified envelope; `Error::Internal`/`Io` are sanitized (500 +
  "internal server error"), details go to `tracing`.
- **Do not add `async-trait` or a proc macro** — dyn dispatch uses
  `Handler<T=()>`/`Handler<(WithContext,)>` marker impls + blanket closures.
- **Avoid `http_body_util::Limited`** and other wrappers with
  `B::Error: Into<Box<dyn Error>>` where-clauses — they trip rustc's
  "implementation of From is not general enough" inside handler futures
  (see `Bounded` in `context.rs` for the fixed-error pattern).
- matchit does **not** percent-decode params; `context::percent_decode` handles
  it (see `src/context.rs`).
- A plain handler registered via `App::route` implies GET + HEAD. HEAD falls
  back to GET automatically; 405 responses flow through the global middleware
  chain (so CORS preflight works).
- WebSocket: hyper parks `OnUpgrade` in request extensions; dispatch removes it
  into `Context` before building. `Upgraded` speaks hyper's rt traits — bridge
  to tungstenite with `TokioIo`. Tests need a real socket (see
  `tests/websocket.rs`); TestClient cannot drive upgrades.
- ServeDir supports single-range requests (206 + Content-Range, 416 on
  unsatisfiable); malformed/multi ranges serve the full body (RFC-compliant).
- `ctx.set_cookie` queues into a per-request jar (`Arc<Mutex<Vec>>`); dispatch
  drains it onto the response after the handler — that indirection exists
  because handlers only hold `&Context` while the response is built later.
- `ctx.body_stream()` consumes the raw body (streaming and buffered
  extraction are mutually exclusive); after a buffered read it yields the
  cache as a single chunk. `Pin<Box<dyn Stream…>>` type annotations in
  handlers MUST include `+ Send` or the future stops being `Send` (the
  Handler bound then fails confusingly at route registration).
- OpenAPI is builder-declared (no macro): `OpenApi::new(..).path(PathDoc::get(..)
  .query::<T>().resp::<R>(200, "ok"))` — payload types need
  `#[derive(schemars::JsonSchema)]`; `App::openapi(doc)` mounts
  `/openapi.json` + `/docs`.

## Common Patterns

### Adding a route
```rust
let app = App::new()
  .state(db)                                  // shared state
  .route("/users", get(list).post(create))    // MethodRouter chain
  .route("/users/{id}", get(get_user))        // plain handler = GET + HEAD
  .nest("/api", api_router())                 // group + middleware
  .merge(other);
```

### Handler
```rust
async fn get_user(ctx: Context) -> Result<Resp<User>> {
  let id: i64 = ctx.param("id")?;
  let db = ctx.state::<Db>()?;
  let user = db.find(id).await?;              // From<DbError> for Error ⇒ ?
  Ok(Resp::ok(user))                          // .created(v) / .page(v, i, n, t)
}
```
In closures returning `Result`, annotate the error: `Ok::<_, Error>(...)`.

### Middleware
```rust
async fn auth(mut ctx: Context, next: Next) -> Result {
  let token = ctx.header("authorization").ok_or_else(Error::unauthorized)?;
  ctx.insert(CurrentUser(token));             // per-request typed data
  next.run(ctx).await                         // onion; may edit response after
}
app.with(auth);                               // global; Router::with for a group
```

### Testing
```rust
let tc = TestClient::new(app);                // no sockets
let res = tc.get("/users/1").send().await;
res.assert_status_ok();
let user: User = res.assert_ok_data();
```

## Before Committing
1. `cargo fmt`
2. `cargo clippy --all-targets --all-features` (zero warnings)
3. `cargo test`
4. `cargo doc --no-deps` (zero warnings)

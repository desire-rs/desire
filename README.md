# Desire

> A minimal and ergonomic Rust web framework, on hyper.

```rust
use desire::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
  let app = App::new().route("/hello/{name}", get(hello));
  app.run("0.0.0.0:3000").await
}

async fn hello(ctx: Context) -> Result<Resp<String>> {
  let name = ctx.param_raw("name")?;
  Ok(Resp::ok(format!("hello, {name}!")))
}
```

```text
$ curl localhost:3000/hello/desire
{"code":0,"msg":"ok","data":"hello, desire!"}
```

## Why desire

desire bets on **fewer concepts and friendlier diagnostics**:

- **Context-style handlers** — `async fn(ctx: Context) -> impl IntoResponse`.
  No extractor tuples, no trait-solver puzzles; read what you need from the
  context, in any order.
- **One response envelope** — `Resp<T>` renders every API reply (and every
  framework error) as `{"code": 0, "msg": "ok", "data": ...}`.
- **Precise extraction errors** — a bad body or param names its layer, field
  and expected type: `invalid param "id", expected "i64": invalid digit`.
- **Production server in one line** — HTTP/1.1 + HTTP/2 on the same port
  (hyper-util auto), optional rustls TLS with ALPN, graceful shutdown,
  body-size limits.
- **Testing without sockets** — `TestClient` drives the real middleware +
  routing pipeline in-memory, millisecond-fast.

The whole framework is six concepts: [`App`], [`Context`], [`Handler`],
[`Middleware`]/[`Next`], [`Router`]/[`MethodRouter`], [`Resp`].

## Installation

```toml
[dependencies]
desire = "0.1"
tokio = { version = "1", features = ["full"] }
```

## A taste of the API

Shared state, method routing, JSON bodies and business errors:

```rust
use desire::prelude::*;

#[derive(Clone)]
struct Db { /* pools are cheap to clone */ }

async fn get_user(ctx: Context) -> Result<Resp<User>> {
  let id: i64 = ctx.param("id")?;          // 404-style errors carry detail
  let db = ctx.state::<Db>()?;             // shared state, zero clones
  let user = db.find(id).await?;
  Ok(Resp::ok(user))
}

async fn create_user(ctx: Context) -> Result<Resp<User>> {
  let input: CreateUser = ctx.json().await?;  // serde detail on failure
  let db = ctx.state::<Db>()?;
  Ok(Resp::created(db.insert(input).await?))
}

let app = App::new()
  .state(db)
  .with(logger())
  .route("/users", get(list_users).post(create_user))
  .route("/users/{id}", get(get_user).delete(delete_user));
app.run("0.0.0.0:3000").await?;            // Ctrl-C drains gracefully
```

Middleware is an onion, and plain async fns are middleware:

```rust
async fn auth(mut ctx: Context, next: Next) -> Result {
  let token = ctx.header("authorization").ok_or_else(Error::unauthorized)?;
  ctx.insert(CurrentUser::verify(token)?);   // visible to handlers via ctx.get
  next.run(ctx).await                        // …and post-process the response
}
```

Composable route trees with group middleware:

```rust
let api = Router::new()
  .with(auth)
  .route("/users", get(list_users))
  .route("/users/{id}", get(get_user));

let app = App::new().nest("/api", api).merge(health_router());
```

## Feature highlights

| Area | What you get |
|------|--------------|
| Extraction | `param` / `params::<T>` / `query::<T>` / `json` / `form` / `body_bytes` / `cookie` / `header` |
| Responses | `Resp<T>` envelope, `Json<T>`, `Html<T>`, text/bytes, `(StatusCode, body)`, streaming via `body::from_stream` |
| Middleware | built-in `logger()`, `cors(CorsConfig)`, `timeout(d)`, `body_limit(n)`; group-level via `Router::with` |
| Errors | `Error::unauthorized()`, `Error::business(code, msg)`, `Error::internal(...)` (sanitized + logged), custom `From` impls |
| Server | H1+H2 auto, `tls` feature (rustls + ALPN), graceful shutdown, concurrency limit, 2 MiB default body cap |
| Static files | `ServeDir` / `ServeFile` with traversal protection, ETag/304, `index.html` |
| Testing | `TestClient::new(app).get("/x").send()` — no sockets |

## Examples

- [`hello`](examples/hello.rs) — the smallest app
- [`json_api`](examples/json_api.rs) — CRUD with state and the envelope
- [`middleware`](examples/middleware.rs) — custom + built-in middleware
- [`static_files`](examples/static_files.rs) — serving a directory
- [`graceful`](examples/graceful.rs) — graceful shutdown
- [`tls`](examples/tls.rs) — HTTPS (requires `--features tls`)

## Testing your app

```rust
#[tokio::test]
async fn get_user_ok() {
  let tc = TestClient::new(app());
  let res = tc.get("/users/1").send().await;
  res.assert_status_ok();
  let user: User = res.assert_ok_data();
  assert_eq!(user.id, 1);
}
```

## License

Apache-2.0. See [LICENSE](LICENSE).

//! End-to-end tests driven through `desire::test::TestClient` — the
//! full middleware + routing + handler pipeline without sockets.

use desire::prelude::*;
use serde::{Deserialize, Serialize};

// ---- test app fixtures ----

#[derive(Clone)]
struct Db {
  users: Vec<(i64, String)>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct User {
  id: i64,
  name: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct CreateUser {
  name: String,
}

#[derive(Deserialize)]
struct ListQuery {
  #[serde(default = "default_page")]
  page: i64,
  #[serde(default)]
  #[allow(dead_code)] // parsed from the query to assert deserialization
  q: Option<String>,
}

fn default_page() -> i64 {
  1
}

struct CurrentUser(String);

fn test_app() -> App {
  App::new()
    .state(Db {
      users: vec![(1, "alice".to_owned()), (2, "bob".to_owned())],
    })
    .route("/health", get(|| async { Resp::ok(()) }))
    .route(
      "/users/{id}",
      get(|ctx: Context| async move {
        let id: i64 = ctx.param("id")?;
        let db = ctx.state::<Db>()?;
        let user = db
          .users
          .iter()
          .find(|(uid, _)| *uid == id)
          .map(|(id, name)| User {
            id: *id,
            name: name.clone(),
          })
          .ok_or_else(|| Error::not_found("user"))?;
        Ok::<_, Error>(Resp::ok(user))
      }),
    )
    .route(
      "/users",
      get(|ctx: Context| async move {
        let _q: ListQuery = ctx.query()?;
        let db = ctx.state::<Db>()?;
        let users: Vec<User> = db
          .users
          .iter()
          .map(|(id, name)| User {
            id: *id,
            name: name.clone(),
          })
          .collect();
        let total = users.len() as i64;
        Ok::<_, Error>(Resp::page(users, 1, 20, total))
      })
      .post(|ctx: Context| async move {
        let input: CreateUser = ctx.json().await?;
        let db = ctx.state::<Db>()?;
        let user = User {
          id: db.users.len() as i64 + 1,
          name: input.name,
        };
        Ok::<_, Error>(Resp::created(user))
      }),
    )
    .route("/text", get(|| async { "plain" }))
    .route(
      "/redirect",
      get(|| async { Response::redirect(StatusCode::MOVED_PERMANENTLY, "/health") }),
    )
}

// ---- basic routing ----

#[tokio::test]
async fn zero_arity_handler_and_envelope() {
  let tc = TestClient::new(test_app());
  let res = tc.get("/health").send().await;
  res.assert_status_ok();
  assert_eq!(res.text(), r#"{"code":0,"msg":"ok","data":null}"#);
}

#[tokio::test]
async fn path_param_and_state() {
  let tc = TestClient::new(test_app());
  let res = tc.get("/users/1").send().await;
  res.assert_status_ok();
  let user: User = res.assert_ok_data();
  assert_eq!(
    user,
    User {
      id: 1,
      name: "alice".to_owned()
    }
  );
}

#[tokio::test]
async fn param_parse_error_renders_envelope() {
  let tc = TestClient::new(test_app());
  let res = tc.get("/users/notanumber").send().await;
  res.assert_status(StatusCode::BAD_REQUEST);
  let body: serde_json::Value = res.json().unwrap();
  assert_eq!(body["code"], 400);
  let msg = body["msg"].as_str().unwrap();
  assert!(msg.contains("id"), "message should name the param: {msg}");
  assert!(msg.contains("i64"), "message should name the type: {msg}");
}

#[tokio::test]
async fn fallback_404_envelope() {
  let tc = TestClient::new(test_app());
  let res = tc.get("/nope").send().await;
  res.assert_status(StatusCode::NOT_FOUND);
  let body: serde_json::Value = res.json().unwrap();
  assert_eq!(body["code"], 404);
}

#[tokio::test]
async fn method_not_allowed_with_allow_header() {
  let tc = TestClient::new(test_app());
  let res = tc.delete("/users").send().await;
  res.assert_status(StatusCode::METHOD_NOT_ALLOWED);
  let allow = res.header("allow").unwrap();
  assert!(allow.contains("GET"), "allow: {allow}");
  assert!(allow.contains("POST"), "allow: {allow}");
}

#[tokio::test]
async fn head_derived_from_get_has_empty_body() {
  let tc = TestClient::new(test_app());
  let res = tc.head("/health").send().await;
  res.assert_status_ok();
  assert!(res.bytes().is_empty());
}

#[tokio::test]
async fn plain_text_response() {
  let tc = TestClient::new(test_app());
  let res = tc.get("/text").send().await;
  res.assert_status_ok();
  assert_eq!(res.text(), "plain");
  assert_eq!(
    res.header("content-type"),
    Some("text/plain; charset=utf-8")
  );
}

#[tokio::test]
async fn redirect() {
  let tc = TestClient::new(test_app());
  let res = tc.get("/redirect").send().await;
  res.assert_status(StatusCode::MOVED_PERMANENTLY);
  assert_eq!(res.header("location"), Some("/health"));
}

// ---- body extraction ----

#[tokio::test]
async fn json_body_extraction() {
  let tc = TestClient::new(test_app());
  let res = tc
    .post("/users")
    .json(&CreateUser {
      name: "carol".to_owned(),
    })
    .send()
    .await;
  res.assert_status(StatusCode::CREATED);
  let body: Resp<User> = res.json().unwrap();
  assert_eq!(body.code, 0);
  assert_eq!(body.data.unwrap().name, "carol");
}

#[tokio::test]
async fn json_error_names_the_field() {
  let tc = TestClient::new(test_app());
  let res = tc
    .post("/users")
    .header("content-type", "application/json")
    .body(Bytes::from_static(b"{\"age\": 3}"))
    .send()
    .await;
  res.assert_status(StatusCode::BAD_REQUEST);
  let body: serde_json::Value = res.json().unwrap();
  let msg = body["msg"].as_str().unwrap();
  assert!(msg.contains("name"), "should name the missing field: {msg}");
}

#[tokio::test]
async fn wrong_content_type_rejected() {
  let tc = TestClient::new(test_app());
  let res = tc
    .post("/users")
    .header("content-type", "text/plain")
    .body(Bytes::from_static(b"{}"))
    .send()
    .await;
  res.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn body_bytes_are_cached_across_extractions() {
  async fn handler(ctx: Context) -> Result<Resp<usize>> {
    let first = ctx.body_bytes().await?;
    let second = ctx.body_bytes().await?;
    assert_eq!(first, second);
    Ok(Resp::ok(first.len()))
  }

  let app = App::new().route("/echo", post(handler));
  let tc = TestClient::new(app);
  let res = tc
    .post("/echo")
    .body(Bytes::from_static(b"hello"))
    .send()
    .await;
  res.assert_status_ok();
  assert_eq!(res.assert_ok_data::<usize>(), 5);
}

// ---- middleware ----

#[tokio::test]
async fn middleware_inserts_data_for_handler() {
  async fn auth(mut ctx: Context, next: Next) -> Result {
    ctx.insert(CurrentUser("alice".to_owned()));
    next.run(ctx).await
  }

  async fn whoami(ctx: Context) -> Resp<String> {
    let user = ctx.get::<CurrentUser>().expect("current user");
    Resp::ok(user.0.clone())
  }

  let app = App::new().with(auth).route("/me", get(whoami));
  let tc = TestClient::new(app);
  let res = tc.get("/me").send().await;
  res.assert_status_ok();
  assert_eq!(res.assert_ok_data::<String>(), "alice");
}

#[tokio::test]
async fn middleware_short_circuit() {
  async fn deny(_ctx: Context, _next: Next) -> Result {
    Err(Error::forbidden())
  }

  async fn handler(_ctx: Context) -> Resp<&'static str> {
    Resp::ok("should not run")
  }

  let app = App::new().with(deny).route("/x", get(handler));
  let tc = TestClient::new(app);
  let res = tc.get("/x").send().await;
  res.assert_status(StatusCode::FORBIDDEN);
  let body: serde_json::Value = res.json().unwrap();
  assert_eq!(body["code"], 403);
}

#[tokio::test]
async fn middleware_modifies_response_after_next() {
  async fn add_header(ctx: Context, next: Next) -> Result {
    let mut res = next.run(ctx).await?;
    res
      .headers_mut()
      .insert("x-desire", hyper::header::HeaderValue::from_static("yes"));
    Ok(res)
  }

  let app = App::new()
    .with(add_header)
    .route("/", get(|| async { Resp::ok(()) }));
  let tc = TestClient::new(app);
  let res = tc.get("/").send().await;
  assert_eq!(res.header("x-desire"), Some("yes"));
}

#[tokio::test]
async fn fallback_runs_through_global_middleware() {
  async fn tagged(ctx: Context, next: Next) -> Result {
    let mut res = next.run(ctx).await?;
    res
      .headers_mut()
      .insert("x-tag", hyper::header::HeaderValue::from_static("1"));
    Ok(res)
  }

  let app = App::new().with(tagged);
  let tc = TestClient::new(app);
  let res = tc.get("/missing").send().await;
  res.assert_status(StatusCode::NOT_FOUND);
  assert_eq!(res.header("x-tag"), Some("1"));
}

#[tokio::test]
async fn timeout_returns_504() {
  async fn slow(ctx: Context, next: Next) -> Result {
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    next.run(ctx).await
  }

  let app = App::new()
    .with(desire::middleware::timeout(
      std::time::Duration::from_millis(20),
    ))
    .with(slow)
    .route("/", get(|| async { Resp::ok(()) }));
  let tc = TestClient::new(app);
  let res = tc.get("/").send().await;
  res.assert_status(StatusCode::GATEWAY_TIMEOUT);
}

#[tokio::test]
async fn cors_preflight() {
  let app = App::new()
    .with(desire::middleware::cors(
      desire::middleware::CorsConfig::default(),
    ))
    .route("/", get(|| async { Resp::ok(()) }));
  let tc = TestClient::new(app);
  let res = tc
    .options("/")
    .header("origin", "https://example.com")
    .header("access-control-request-method", "GET")
    .send()
    .await;
  res.assert_status(StatusCode::NO_CONTENT);
  assert_eq!(
    res.header("access-control-allow-origin"),
    Some("https://example.com")
  );
}

#[tokio::test]
async fn cors_adds_headers_to_normal_response() {
  let app = App::new()
    .with(desire::middleware::cors(
      desire::middleware::CorsConfig::default(),
    ))
    .route("/", get(|| async { Resp::ok(()) }));
  let tc = TestClient::new(app);
  let res = tc
    .get("/")
    .header("origin", "https://example.com")
    .send()
    .await;
  res.assert_status_ok();
  assert_eq!(
    res.header("access-control-allow-origin"),
    Some("https://example.com")
  );
}

// ---- body limit ----

#[tokio::test]
async fn body_over_limit_is_413() {
  async fn echo(ctx: Context) -> Result<Resp<usize>> {
    Ok(Resp::ok(ctx.body_bytes().await?.len()))
  }

  let app = App::new().max_body_size(8).route("/echo", post(echo));
  let tc = TestClient::new(app);
  let res = tc
    .post("/echo")
    .body(Bytes::from(vec![b'a'; 64]))
    .send()
    .await;
  res.assert_status(StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn body_limit_middleware_allows_larger() {
  async fn echo(ctx: Context) -> Result<Resp<usize>> {
    Ok(Resp::ok(ctx.body_bytes().await?.len()))
  }

  let app = App::new()
    .max_body_size(8)
    .with(desire::middleware::body_limit(1024))
    .route("/echo", post(echo));
  let tc = TestClient::new(app);
  let res = tc
    .post("/echo")
    .body(Bytes::from(vec![b'a'; 64]))
    .send()
    .await;
  res.assert_status_ok();
  assert_eq!(res.assert_ok_data::<usize>(), 64);
}

// ---- nest / merge / 405 ----

#[tokio::test]
async fn nest_prefixes_routes_and_applies_group_middleware() {
  async fn tag(ctx: Context, next: Next) -> Result {
    let mut res = next.run(ctx).await?;
    res
      .headers_mut()
      .insert("x-api", hyper::header::HeaderValue::from_static("v1"));
    Ok(res)
  }

  let api = Router::new()
    .with(tag)
    .route("/ping", get(|| async { Resp::ok("pong") }));

  let app = App::new().nest("/api", api);
  let tc = TestClient::new(app);

  let res = tc.get("/api/ping").send().await;
  res.assert_status_ok();
  assert_eq!(res.header("x-api"), Some("v1"));
  assert_eq!(res.assert_ok_data::<String>(), "pong");

  // outer path is untouched
  let res = tc.get("/ping").send().await;
  res.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn merge_combines_routers_without_losing_routes() {
  let a = Router::new().route("/a", get(|| async { Resp::ok("a") }));
  let b = Router::new().route("/b", get(|| async { Resp::ok("b") }));

  let app = App::new().merge(a).merge(b);
  let tc = TestClient::new(app);

  let res = tc.get("/a").send().await;
  res.assert_status_ok();
  let res = tc.get("/b").send().await;
  res.assert_status_ok();
}

// ---- query ----

#[tokio::test]
async fn query_defaults() {
  async fn handler(ctx: Context) -> Result<Resp<i64>> {
    let q: ListQuery = ctx.query()?;
    Ok(Resp::ok(q.page))
  }

  let app = App::new().route("/q", get(handler));
  let tc = TestClient::new(app);
  let res = tc.get("/q").send().await;
  res.assert_status_ok();
  assert_eq!(res.assert_ok_data::<i64>(), 1);
}

// ---- streaming ----

#[tokio::test]
async fn streaming_body() {
  use bytes::Bytes;

  async fn stream(_ctx: Context) -> Body {
    let chunks: Vec<Result<Bytes, Error>> = vec![
      Ok(Bytes::from_static(b"hello ")),
      Ok(Bytes::from_static(b"stream")),
    ];
    body::from_stream(tokio_stream::iter(chunks))
  }

  let app = App::new().route("/stream", get(stream));
  let tc = TestClient::new(app);
  let res = tc.get("/stream").send().await;
  res.assert_status_ok();
  assert_eq!(res.text(), "hello stream");
}

// ---- errors ----

#[tokio::test]
async fn business_error_renders_code_with_http_200() {
  async fn handler(_ctx: Context) -> Result<Resp<()>> {
    Err(Error::business(40001, "余额不足"))
  }

  let app = App::new().route("/pay", post(handler));
  let tc = TestClient::new(app);
  let res = tc.post("/pay").send().await;
  res.assert_status_ok();
  let body: serde_json::Value = res.json().unwrap();
  assert_eq!(body["code"], 40001);
  assert_eq!(body["msg"], "余额不足");
}

#[tokio::test]
async fn internal_error_is_sanitized() {
  async fn handler(_ctx: Context) -> Result<Resp<()>> {
    Err(Error::internal("db password is hunter2"))
  }

  let app = App::new().route("/boom", get(handler));
  let tc = TestClient::new(app);
  let res = tc.get("/boom").send().await;
  res.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
  let text = res.text();
  assert!(!text.contains("hunter2"), "must not leak internals: {text}");
  assert!(text.contains("internal server error"));
}

#[tokio::test]
async fn custom_fallback_handler() {
  let app = App::new().fallback(|| async { (StatusCode::IM_A_TEAPOT, "short and stout") });
  let tc = TestClient::new(app);
  let res = tc.get("/nowhere").send().await;
  res.assert_status(StatusCode::IM_A_TEAPOT);
  assert_eq!(res.text(), "short and stout");
}

// ---- cookies ----

#[tokio::test]
async fn set_cookie_reaches_response_headers() {
  async fn login(ctx: Context) -> Resp<String> {
    ctx.set_cookie(
      desire::cookie::Cookie::build(("session", "abc123"))
        .http_only(true)
        .path("/")
        .build(),
    );
    Resp::ok("logged in".to_owned())
  }

  let app = App::new().route("/login", post(login));
  let tc = TestClient::new(app);
  let res = tc.post("/login").send().await;
  res.assert_status_ok();
  assert_eq!(
    res.header("set-cookie"),
    Some("session=abc123; HttpOnly; Path=/")
  );
}

#[tokio::test]
async fn cookie_read_back() {
  async fn whoami(ctx: Context) -> Resp<String> {
    let session = ctx.cookie("session").map(|c| c.value().to_owned());
    Resp::ok(session.unwrap_or_else(|| "anonymous".to_owned()))
  }

  let app = App::new().route("/me", get(whoami));
  let tc = TestClient::new(app);

  let res = tc.get("/me").send().await;
  res.assert_status_ok();
  assert_eq!(res.assert_ok_data::<String>(), "anonymous");

  let res = tc.get("/me").header("cookie", "session=xyz").send().await;
  res.assert_status_ok();
  assert_eq!(res.assert_ok_data::<String>(), "xyz");
}

// ---- multipart ----

#[tokio::test]
async fn multipart_form_fields_and_files() {
  async fn upload(ctx: Context) -> Result<Resp<String>> {
    let form = ctx.form_data().await?;
    let title = form
      .field("title")
      .ok_or_else(|| Error::msg("missing title"))?;
    let file = form
      .file("attachment")
      .ok_or_else(|| Error::msg("missing file"))?;
    assert_eq!(file.filename.as_deref(), Some("notes.txt"));
    assert_eq!(file.content_type.as_deref(), Some("text/plain"));
    assert_eq!(file.bytes.as_ref(), b"FILE DATA");
    Ok(Resp::ok(format!("{title}:{}", file.bytes.len())))
  }

  let body = "--XBOUNDARY\r\n\
              Content-Disposition: form-data; name=\"title\"\r\n\
              \r\n\
              my upload\r\n\
              --XBOUNDARY\r\n\
              Content-Disposition: form-data; name=\"attachment\"; filename=\"notes.txt\"\r\n\
              Content-Type: text/plain\r\n\
              \r\n\
              FILE DATA\r\n\
              --XBOUNDARY--\r\n";

  let app = App::new().route("/upload", post(upload));
  let tc = TestClient::new(app);
  let res = tc
    .post("/upload")
    .header("content-type", "multipart/form-data; boundary=XBOUNDARY")
    .body(Bytes::from_static(body.as_bytes()))
    .send()
    .await;
  res.assert_status_ok();
  assert_eq!(res.assert_ok_data::<String>(), "my upload:9");
}

#[tokio::test]
async fn multipart_wrong_content_type_is_400() {
  async fn upload(ctx: Context) -> Result<Resp<()>> {
    let _form = ctx.form_data().await?;
    Ok(Resp::ok(()))
  }

  let app = App::new().route("/upload", post(upload));
  let tc = TestClient::new(app);
  let res = tc
    .post("/upload")
    .header("content-type", "application/json")
    .body(Bytes::from_static(b"{}"))
    .send()
    .await;
  res.assert_status(StatusCode::BAD_REQUEST);
}

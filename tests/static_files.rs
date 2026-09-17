//! Static file serving tests: MIME detection, ETag conditional
//! requests, and path-traversal protection.

use desire::prelude::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static DIR_SEQ: AtomicUsize = AtomicUsize::new(0);

fn unique_dir(prefix: &str) -> PathBuf {
  let n = DIR_SEQ.fetch_add(1, Ordering::SeqCst);
  std::env::temp_dir().join(format!("desire-test-{prefix}-{}-{n}", std::process::id()))
}

fn fixture_app() -> (App, PathBuf) {
  let dir = unique_dir("dir");
  let assets = dir.join("assets");
  std::fs::create_dir_all(&assets).unwrap();
  std::fs::write(assets.join("hello.txt"), b"hello static").unwrap();
  std::fs::write(assets.join("data.json"), b"{\"ok\":true}").unwrap();
  std::fs::write(assets.join("index.html"), b"<h1>index</h1>").unwrap();
  // A secret OUTSIDE the served root — traversal must never reach it.
  std::fs::write(dir.join("secret.txt"), b"SECRET").unwrap();
  let app = App::new()
    .route("/static", ServeDir::new(assets.clone()))
    .route("/static/{*path}", ServeDir::new(assets.clone()));
  (app, dir)
}

#[tokio::test]
async fn serves_file_with_mime_type() {
  let (app, dir) = fixture_app();
  let tc = TestClient::new(app);
  let res = tc.get("/static/hello.txt").send().await;
  res.assert_status_ok();
  assert_eq!(res.text(), "hello static");
  assert_eq!(res.header("content-type"), Some("text/plain"));
  assert!(res.header("etag").is_some());
  std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn json_mime_type() {
  let (app, dir) = fixture_app();
  let tc = TestClient::new(app);
  let res = tc.get("/static/data.json").send().await;
  res.assert_status_ok();
  assert_eq!(res.header("content-type"), Some("application/json"));
  std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn missing_file_is_404_envelope() {
  let (app, dir) = fixture_app();
  let tc = TestClient::new(app);
  let res = tc.get("/static/nope.txt").send().await;
  res.assert_status(StatusCode::NOT_FOUND);
  std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn etag_conditional_get_returns_304() {
  let (app, dir) = fixture_app();
  let tc = TestClient::new(app);
  let first = tc.get("/static/hello.txt").send().await;
  let etag = first.header("etag").expect("etag").to_owned();
  let second = tc
    .get("/static/hello.txt")
    .header("if-none-match", &etag)
    .send()
    .await;
  second.assert_status(StatusCode::NOT_MODIFIED);
  assert!(second.bytes().is_empty());
  std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn directory_serves_index_html() {
  let (app, dir) = fixture_app();
  let tc = TestClient::new(app);
  let res = tc.get("/static").send().await;
  res.assert_status_ok();
  assert_eq!(res.text(), "<h1>index</h1>");
  std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn traversal_attempts_are_rejected() {
  let (app, dir) = fixture_app();
  let tc = TestClient::new(app);

  // percent-encoded ..
  let res = tc.get("/static/%2e%2e/secret.txt").send().await;
  res.assert_status(StatusCode::NOT_FOUND);

  // raw .. segments (no percent encoding)
  let res = tc.get("/static/../secret.txt").send().await;
  res.assert_status(StatusCode::NOT_FOUND);

  // deep traversal with encoding
  let res = tc.get("/static/%2e%2e%2f%2e%2e%2fsecret.txt").send().await;
  res.assert_status(StatusCode::NOT_FOUND);

  // backslash trick
  let res = tc.get("/static/..%5Csecret.txt").send().await;
  res.assert_status(StatusCode::NOT_FOUND);

  // NUL byte
  let res = tc.get("/static/%00.txt").send().await;
  res.assert_status(StatusCode::NOT_FOUND);

  // The secret really is where we think it is (sanity check the fixture).
  let secret = std::fs::read_to_string(dir.join("secret.txt")).unwrap();
  assert_eq!(secret, "SECRET");

  std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn serve_file_by_exact_path() {
  let dir = unique_dir("file");
  let assets = dir.join("assets");
  std::fs::create_dir_all(&assets).unwrap();
  std::fs::write(assets.join("favicon.ico"), b"ico").unwrap();

  let app = App::new().route("/favicon.ico", ServeFile::new(assets.join("favicon.ico")));
  let tc = TestClient::new(app);
  let res = tc.get("/favicon.ico").send().await;
  res.assert_status_ok();
  assert_eq!(res.text(), "ico");

  std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn range_request_returns_partial_content() {
  let (app, dir) = fixture_app();
  let tc = TestClient::new(app);

  // "hello static" is 12 bytes.
  let res = tc
    .get("/static/hello.txt")
    .header("range", "bytes=0-4")
    .send()
    .await;
  res.assert_status(StatusCode::PARTIAL_CONTENT);
  assert_eq!(res.text(), "hello");
  assert_eq!(res.header("content-range"), Some("bytes 0-4/12"));

  let res = tc
    .get("/static/hello.txt")
    .header("range", "bytes=6-")
    .send()
    .await;
  res.assert_status(StatusCode::PARTIAL_CONTENT);
  assert_eq!(res.text(), "static");

  // suffix: last 6 bytes
  let res = tc
    .get("/static/hello.txt")
    .header("range", "bytes=-6")
    .send()
    .await;
  res.assert_status(StatusCode::PARTIAL_CONTENT);
  assert_eq!(res.text(), "static");

  std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn range_unsatisfiable_is_416() {
  let (app, dir) = fixture_app();
  let tc = TestClient::new(app);
  let res = tc
    .get("/static/hello.txt")
    .header("range", "bytes=999-")
    .send()
    .await;
  res.assert_status(StatusCode::RANGE_NOT_SATISFIABLE);
  assert_eq!(res.header("content-range"), Some("bytes */12"));
  std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn malformed_range_serves_full_body() {
  let (app, dir) = fixture_app();
  let tc = TestClient::new(app);

  for garbage in ["bytes=abc", "items=0-2", "bytes=5-2", "bytes=0-1,3-4"] {
    let res = tc
      .get("/static/hello.txt")
      .header("range", garbage)
      .send()
      .await;
    res.assert_status(StatusCode::OK);
    assert_eq!(res.text(), "hello static", "garbage range: {garbage}");
  }

  std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn full_response_advertises_accept_ranges() {
  let (app, dir) = fixture_app();
  let tc = TestClient::new(app);
  let res = tc.get("/static/hello.txt").send().await;
  res.assert_status_ok();
  assert_eq!(res.header("accept-ranges"), Some("bytes"));
  std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn dotfiles_are_rejected_by_default() {
  let dir = unique_dir("dot");
  let assets = dir.join("assets");
  std::fs::create_dir_all(&assets).unwrap();
  std::fs::write(assets.join(".env"), b"SECRET=1").unwrap();
  std::fs::create_dir_all(assets.join(".git")).unwrap();
  std::fs::write(assets.join(".git").join("config"), b"repo config").unwrap();

  let app = App::new().route("/static/{*path}", ServeDir::new(assets.clone()));
  let tc = TestClient::new(app);

  let res = tc.get("/static/.env").send().await;
  res.assert_status(StatusCode::NOT_FOUND);
  let res = tc.get("/static/.git/config").send().await;
  res.assert_status(StatusCode::NOT_FOUND);

  // Opt-in: dotfiles are served.
  let app = App::new().route("/static/{*path}", ServeDir::new(assets).allow_dotfiles());
  let tc = TestClient::new(app);
  let res = tc.get("/static/.env").send().await;
  res.assert_status_ok();
  assert_eq!(res.text(), "SECRET=1");

  std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn cache_control_header_is_emitted() {
  let (app, dir) = fixture_app();
  let tc = TestClient::new(app);
  let res = tc.get("/static/hello.txt").send().await;
  assert_eq!(res.header("cache-control"), None);

  let dir2 = unique_dir("cc");
  let assets = dir2.join("assets");
  std::fs::create_dir_all(&assets).unwrap();
  std::fs::write(assets.join("a.txt"), b"a").unwrap();
  let app = App::new().route(
    "/static/{*path}",
    ServeDir::new(assets).cache_control("public, max-age=3600"),
  );
  let tc = TestClient::new(app);
  let res = tc.get("/static/a.txt").send().await;
  res.assert_status_ok();
  assert_eq!(res.header("cache-control"), Some("public, max-age=3600"));

  std::fs::remove_dir_all(dir).ok();
  std::fs::remove_dir_all(dir2).ok();
}

#[tokio::test]
async fn files_listing_renders_escaped_names() {
  let dir = unique_dir("listing");
  let assets = dir.join("assets");
  std::fs::create_dir_all(assets.join("sub")).unwrap();
  std::fs::write(assets.join("a.txt"), b"a").unwrap();
  std::fs::write(assets.join("a&b<c>.txt"), b"tricky").unwrap();
  std::fs::write(assets.join(".hidden"), b"x").unwrap();

  let app = App::new()
    .route("/static", ServeDir::new(assets.clone()).files_listing())
    .route("/static/{*path}", ServeDir::new(assets).files_listing());
  let tc = TestClient::new(app);

  let res = tc.get("/static").send().await;
  res.assert_status_ok();
  let html = res.text();
  assert!(html.contains("a.txt"));
  assert!(
    html.contains("a&amp;b&lt;c&gt;.txt"),
    "escaped name expected: {html}"
  );
  assert!(!html.contains(".hidden"), "dotfiles hidden from listing");
  assert!(
    html.contains("<td>dir</td>"),
    "subdir marked as dir: {html}"
  );

  // dotfile request still rejected even with listing on
  let res = tc.get("/static/.hidden").send().await;
  res.assert_status(StatusCode::NOT_FOUND);

  std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn index_html_wins_over_listing() {
  let dir = unique_dir("listing-index");
  let assets = dir.join("assets");
  std::fs::create_dir_all(&assets).unwrap();
  std::fs::write(assets.join("index.html"), b"<h1>index</h1>").unwrap();
  std::fs::write(assets.join("b.txt"), b"b").unwrap();

  let app = App::new().route("/static", ServeDir::new(assets).files_listing());
  let tc = TestClient::new(app);
  let res = tc.get("/static").send().await;
  res.assert_status_ok();
  assert_eq!(res.text(), "<h1>index</h1>");

  std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn listing_disabled_defaults_to_404() {
  let dir = unique_dir("listing-off");
  let assets = dir.join("assets");
  std::fs::create_dir_all(&assets).unwrap();
  std::fs::write(assets.join("c.txt"), b"c").unwrap();

  let app = App::new().route("/static", ServeDir::new(assets));
  let tc = TestClient::new(app);
  let res = tc.get("/static").send().await;
  res.assert_status(StatusCode::NOT_FOUND);

  std::fs::remove_dir_all(dir).ok();
}

/// An adversarial battery of hostile path encodings. None of them may
/// ever read the secret outside the root (or inside it, for dotfiles).
#[tokio::test]
async fn hostile_path_battery_never_leaks() {
  let dir = unique_dir("battery");
  let assets = dir.join("assets");
  std::fs::create_dir_all(&assets).unwrap();
  std::fs::write(assets.join("ok.txt"), b"fine").unwrap();
  std::fs::write(dir.join("secret.txt"), b"SECRET").unwrap();

  let app = App::new().route("/static/{*path}", ServeDir::new(assets.clone()));
  let tc = TestClient::new(app);

  let hostile = [
    // classic traversal
    "/static/../secret.txt",
    "/static/../../secret.txt",
    "/static/../../../secret.txt",
    // percent-encoded
    "/static/%2e%2e/secret.txt",
    "/static/%2E%2E/secret.txt",
    "/static/%2e%2e%2fsecret.txt",
    "/static/..%2fsecret.txt",
    "/static/%2e%2e%5csecret.txt",
    // double-encoded
    "/static/%252e%252e/secret.txt",
    "/static/%25252e%25252e/secret.txt",
    // overlong / invalid utf-8 tricks
    "/static/%c0%ae%c0%ae/secret.txt",
    "/static/..%c0%afsecret.txt",
    // separators and misc
    "/static/..\\secret.txt",
    "/static/....//secret.txt",
    "/static/..;/secret.txt",
    "/static//..//secret.txt",
    "/static/./../secret.txt",
    "/static/..%00.txt",
    "/static/secret.txt%00",
    "/static/%00",
    // absolute and windows-flavored
    "/static//etc/passwd",
    "/static/C:/windows/win.ini",
    "/static/c%3A%5Cwindows%5Cwin.ini",
    // dotfile guard
    "/static/.env",
    "/static/%2eenv",
    // long path abuse
    "/static/../../../a/b/c/d/e/f/../../../../../../secret.txt",
  ];

  for path in hostile {
    let res = tc.get(path).send().await;
    let body = res.text();
    assert!(
      !body.contains("SECRET"),
      "hostile path leaked secret: {path} -> {body}"
    );
    // anything that slipped past the guard must still not be the secret file
    if res.status() == StatusCode::OK {
      assert_eq!(
        res.text(),
        "fine",
        "hostile path resolved outside allowlist: {path}"
      );
    }
  }

  // sanity: the real file is still there and still served
  let res = tc.get("/static/ok.txt").send().await;
  res.assert_status_ok();
  assert_eq!(res.text(), "fine");

  std::fs::remove_dir_all(dir).ok();
}

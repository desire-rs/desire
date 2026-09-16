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

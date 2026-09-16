//! gzip response compression (requires the `gzip` feature).
#![cfg(feature = "gzip")]

use desire::prelude::*;
use std::io::Read as _;

async fn big_text(_ctx: Context) -> String {
  "desire ".repeat(500) // 3.5 KB of very compressible text
}

fn app() -> App {
  App::new()
    .with(desire::middleware::gzip())
    .route("/big", get(big_text))
    .route("/small", get(|| async { "tiny" }))
}

#[tokio::test]
async fn gzip_compresses_when_accepted() {
  let tc = TestClient::new(app());
  let res = tc
    .get("/big")
    .header("accept-encoding", "gzip")
    .send()
    .await;
  res.assert_status_ok();
  assert_eq!(res.header("content-encoding"), Some("gzip"));

  // The body really is gzip: decode it and compare.
  let gz = res.bytes();
  let mut decoder = flate2::read::GzDecoder::new(gz.as_ref());
  let mut plain = Vec::new();
  decoder.read_to_end(&mut plain).unwrap();
  let plain = String::from_utf8(plain).unwrap();
  assert_eq!(plain.len(), 3500);
  assert!(plain.starts_with("desire desire"));
}

#[tokio::test]
async fn passthrough_without_accept_encoding() {
  let tc = TestClient::new(app());
  let res = tc.get("/big").send().await;
  res.assert_status_ok();
  assert_eq!(res.header("content-encoding"), None);
  assert_eq!(res.text().len(), 3500);
}

#[tokio::test]
async fn vary_header_present() {
  let tc = TestClient::new(app());
  let res = tc
    .get("/big")
    .header("accept-encoding", "gzip")
    .send()
    .await;
  assert_eq!(res.header("vary"), Some("Accept-Encoding"));
}

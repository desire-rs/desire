//! Throughput / allocation comparison harness (axum side).
//! Equivalent work to bench.rs: one path param + the same JSON envelope.
#![allow(clippy::missing_panics_doc)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
  unsafe fn alloc(&self, l: Layout) -> *mut u8 {
    ALLOCS.fetch_add(1, Ordering::Relaxed);
    System.alloc(l)
  }
  unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
    System.dealloc(p, l)
  }
  unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
    ALLOCS.fetch_add(1, Ordering::Relaxed);
    System.realloc(p, l, n)
  }
  unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
    ALLOCS.fetch_add(1, Ordering::Relaxed);
    System.alloc_zeroed(l)
  }
}

#[global_allocator]
static A: Counting = Counting;

use axum::{Json, Router, extract::Path, routing::get};
use serde_json::json;

#[tokio::main]
async fn main() {
  let app = Router::new().route("/hello/{name}", get(hello)).route(
    "/stats",
    get(|| async { Json(ALLOCS.load(Ordering::Relaxed)) }),
  );

  let listener = tokio::net::TcpListener::bind("0.0.0.0:3001")
    .await
    .expect("bind 3001");
  axum::serve(listener, app).await.expect("serve");
}

async fn hello(Path(name): Path<String>) -> Json<serde_json::Value> {
  Json(json!({ "code": 0, "msg": "ok", "data": format!("hello, {name}!") }))
}
